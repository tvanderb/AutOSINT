use neo4rs::query;

use autosint_common::types::{AttributionDepth, Claim};
use autosint_common::ClaimId;

use super::conversions::{format_datetime, node_to_claim, parse_entity_id};
use super::GraphError;

#[allow(dead_code)]
impl super::GraphClient {
    /// Create a new claim with PUBLISHED and REFERENCES edges.
    /// Transaction: validates source entity, creates claim node, creates PUBLISHED edge,
    /// creates REFERENCES edges for each referenced entity.
    pub async fn create_claim(
        &self,
        claim: &Claim,
        embedding: Option<Vec<f32>>,
    ) -> Result<Claim, GraphError> {
        let start = std::time::Instant::now();

        // Validate source entity exists before starting transaction.
        let source_check = query("MATCH (e:Entity {id: $id}) RETURN e.id AS id")
            .param("id", claim.source_entity_id.to_string());
        let mut check_result = self
            .graph
            .execute(source_check)
            .await
            .map_err(|e| GraphError::Query(e.to_string()))?;
        if check_result
            .next()
            .await
            .map_err(|e| GraphError::Query(e.to_string()))?
            .is_none()
        {
            return Err(GraphError::NotFound(format!(
                "Source entity {}",
                claim.source_entity_id
            )));
        }

        let has_embedding = embedding.is_some();
        let embedding_f64: Vec<f64> = embedding
            .as_ref()
            .map(|v| v.iter().map(|&f| f as f64).collect())
            .unwrap_or_default();

        let attribution_depth_str = match claim.attribution_depth {
            AttributionDepth::Primary => "primary",
            AttributionDepth::Secondhand => "secondhand",
        };

        let published_ts = format_datetime(&claim.published_timestamp);
        let ingested_ts = format_datetime(&claim.ingested_timestamp);
        let embedding_pending = !has_embedding;

        let mut txn = self
            .graph
            .start_txn()
            .await
            .map_err(|e| GraphError::Query(e.to_string()))?;

        // 1. Create claim node.
        let mut create_cypher = String::from(
            "CREATE (c:Claim { \
                id: $id, \
                content: $content, \
                published_timestamp: $published_timestamp, \
                ingested_timestamp: $ingested_timestamp, \
                attribution_depth: $attribution_depth, \
                embedding_pending: $embedding_pending \
            })",
        );

        if claim.raw_source_link.is_some() {
            create_cypher.push_str(" SET c.raw_source_link = $raw_source_link");
            if has_embedding {
                create_cypher.push_str(", c.embedding = $embedding");
            }
        } else if has_embedding {
            create_cypher.push_str(" SET c.embedding = $embedding");
        }

        let mut q1 = query(&create_cypher)
            .param("id", claim.id.to_string())
            .param("content", claim.content.as_str())
            .param("published_timestamp", published_ts.as_str())
            .param("ingested_timestamp", ingested_ts.as_str())
            .param("attribution_depth", attribution_depth_str)
            .param("embedding_pending", embedding_pending);

        if let Some(ref link) = claim.raw_source_link {
            q1 = q1.param("raw_source_link", link.as_str());
        }
        if has_embedding {
            q1 = q1.param("embedding", embedding_f64);
        }

        txn.run(q1)
            .await
            .map_err(|e| GraphError::Query(e.to_string()))?;

        // 2. Create PUBLISHED edge (source entity → claim).
        let q2 = query(
            "MATCH (e:Entity {id: $entity_id}), (c:Claim {id: $claim_id}) \
             CREATE (e)-[:PUBLISHED]->(c)",
        )
        .param("entity_id", claim.source_entity_id.to_string())
        .param("claim_id", claim.id.to_string());

        txn.run(q2)
            .await
            .map_err(|e| GraphError::Query(e.to_string()))?;

        // 3. Create REFERENCES edges (claim → referenced entities).
        for ref_id in &claim.referenced_entity_ids {
            let q3 = query(
                "MATCH (c:Claim {id: $claim_id}), (e:Entity {id: $entity_id}) \
                 CREATE (c)-[:REFERENCES]->(e)",
            )
            .param("claim_id", claim.id.to_string())
            .param("entity_id", ref_id.to_string());

            txn.run(q3)
                .await
                .map_err(|e| GraphError::Query(e.to_string()))?;
        }

        txn.commit()
            .await
            .map_err(|e| GraphError::Query(e.to_string()))?;

        metrics::histogram!("graph.claim.create.latency").record(start.elapsed().as_secs_f64());

        // Fetch and return the created claim.
        self.get_claim(claim.id).await
    }

    /// Get a claim by ID, including source entity and referenced entities from edges.
    pub async fn get_claim(&self, id: ClaimId) -> Result<Claim, GraphError> {
        let start = std::time::Instant::now();

        let q = query(
            "MATCH (c:Claim {id: $id}) \
             OPTIONAL MATCH (source:Entity)-[:PUBLISHED]->(c) \
             OPTIONAL MATCH (c)-[:REFERENCES]->(ref:Entity) \
             RETURN c, source.id AS source_id, collect(ref.id) AS ref_ids",
        )
        .param("id", id.to_string());

        let mut result = self
            .graph
            .execute(q)
            .await
            .map_err(|e| GraphError::Query(e.to_string()))?;

        let row = result
            .next()
            .await
            .map_err(|e| GraphError::Query(e.to_string()))?
            .ok_or_else(|| GraphError::NotFound(format!("Claim {}", id)))?;

        let node: neo4rs::Node = row
            .get("c")
            .map_err(|e| GraphError::Query(format!("Missing 'c' column: {}", e)))?;

        let source_id_str: String = row
            .get("source_id")
            .map_err(|e| GraphError::Query(format!("Missing 'source_id' column: {}", e)))?;
        let source_entity_id = parse_entity_id(&source_id_str)?;

        let ref_id_strs: Vec<String> = row
            .get("ref_ids")
            .map_err(|e| GraphError::Query(format!("Missing 'ref_ids' column: {}", e)))?;
        let referenced_entity_ids = ref_id_strs
            .iter()
            .map(|s| parse_entity_id(s))
            .collect::<Result<Vec<_>, _>>()?;

        let claim = node_to_claim(&node, source_entity_id, referenced_entity_ids)?;

        metrics::histogram!("graph.claim.get.latency").record(start.elapsed().as_secs_f64());

        Ok(claim)
    }
}
