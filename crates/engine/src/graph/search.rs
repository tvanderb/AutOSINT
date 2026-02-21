use neo4rs::query;

use autosint_common::types::{AttributionDepth, Claim, Entity, Relationship};
use autosint_common::EntityId;
use chrono::{DateTime, Utc};

use super::conversions::{
    format_datetime, node_to_claim, node_to_entity, parse_entity_id, relation_to_relationship,
};
use super::escape_lucene_query;
use super::GraphError;

/// How to search: semantic (vector) or keyword (fulltext).
#[allow(dead_code)]
pub enum SearchMode {
    Semantic,
    Keyword,
}

/// A search result with its relevance score.
#[allow(dead_code)]
pub struct SearchResult<T> {
    pub item: T,
    pub score: f64,
}

/// Parameters for searching entities.
#[allow(dead_code)]
pub struct EntitySearchParams {
    pub query: String,
    pub mode: SearchMode,
    pub kind_filter: Option<String>,
    pub updated_after: Option<DateTime<Utc>>,
    pub updated_before: Option<DateTime<Utc>>,
    pub limit: Option<u32>,
}

/// Parameters for searching claims.
#[allow(dead_code)]
pub struct ClaimSearchParams {
    pub query: Option<String>,
    pub mode: Option<SearchMode>,
    pub published_after: Option<DateTime<Utc>>,
    pub published_before: Option<DateTime<Utc>>,
    pub source_entity_id: Option<EntityId>,
    pub referenced_entity_id: Option<EntityId>,
    pub attribution_depth: Option<AttributionDepth>,
    pub limit: Option<u32>,
}

/// Parameters for searching relationships.
#[allow(dead_code)]
pub struct RelationshipSearchParams {
    pub query: String,
    pub limit: Option<u32>,
}

#[allow(dead_code)]
impl super::GraphClient {
    /// Search entities by vector similarity or fulltext.
    pub async fn search_entities(
        &self,
        params: &EntitySearchParams,
        query_embedding: Option<Vec<f32>>,
    ) -> Result<Vec<SearchResult<Entity>>, GraphError> {
        let start = std::time::Instant::now();
        let limit = params.limit.unwrap_or(20) as i64;

        let results = match params.mode {
            SearchMode::Semantic => {
                let embedding = query_embedding.ok_or_else(|| {
                    GraphError::Query("Semantic search requires a query embedding".into())
                })?;
                let emb_f64: Vec<f64> = embedding.iter().map(|&f| f as f64).collect();

                // Build WHERE filters.
                let mut where_parts = Vec::new();
                if params.kind_filter.is_some() {
                    where_parts.push("node.kind = $kind_filter".to_string());
                }
                if params.updated_after.is_some() {
                    where_parts.push("node.last_updated >= $updated_after".to_string());
                }
                if params.updated_before.is_some() {
                    where_parts.push("node.last_updated <= $updated_before".to_string());
                }

                let where_str = if where_parts.is_empty() {
                    String::new()
                } else {
                    format!(" WHERE {}", where_parts.join(" AND "))
                };

                let cypher = format!(
                    "CALL db.index.vector.queryNodes('entity_embedding', $limit, $embedding) \
                     YIELD node, score{} \
                     RETURN node, score ORDER BY score DESC",
                    where_str
                );

                let mut q = query(&cypher)
                    .param("limit", limit)
                    .param("embedding", emb_f64);

                if let Some(ref kind) = params.kind_filter {
                    q = q.param("kind_filter", kind.as_str());
                }
                if let Some(ref after) = params.updated_after {
                    q = q.param("updated_after", format_datetime(after));
                }
                if let Some(ref before) = params.updated_before {
                    q = q.param("updated_before", format_datetime(before));
                }

                self.execute_entity_search(q).await?
            }
            SearchMode::Keyword => {
                let mut where_parts = Vec::new();
                if params.kind_filter.is_some() {
                    where_parts.push("node.kind = $kind_filter".to_string());
                }
                if params.updated_after.is_some() {
                    where_parts.push("node.last_updated >= $updated_after".to_string());
                }
                if params.updated_before.is_some() {
                    where_parts.push("node.last_updated <= $updated_before".to_string());
                }

                let where_str = if where_parts.is_empty() {
                    String::new()
                } else {
                    format!(" WHERE {}", where_parts.join(" AND "))
                };

                let cypher = format!(
                    "CALL db.index.fulltext.queryNodes('entity_name_fulltext', $query) \
                     YIELD node, score{} \
                     RETURN node, score ORDER BY score DESC LIMIT $limit",
                    where_str
                );

                let escaped_query = escape_lucene_query(&params.query);
                let mut q = query(&cypher)
                    .param("query", escaped_query.as_str())
                    .param("limit", limit);

                if let Some(ref kind) = params.kind_filter {
                    q = q.param("kind_filter", kind.as_str());
                }
                if let Some(ref after) = params.updated_after {
                    q = q.param("updated_after", format_datetime(after));
                }
                if let Some(ref before) = params.updated_before {
                    q = q.param("updated_before", format_datetime(before));
                }

                self.execute_entity_search(q).await?
            }
        };

        metrics::histogram!("graph.search.latency", "mode" => match params.mode {
            SearchMode::Semantic => "semantic",
            SearchMode::Keyword => "keyword",
        }, "target" => "entity")
        .record(start.elapsed().as_secs_f64());

        metrics::histogram!("graph.search.results", "target" => "entity")
            .record(results.len() as f64);

        Ok(results)
    }

    async fn execute_entity_search(
        &self,
        q: neo4rs::Query,
    ) -> Result<Vec<SearchResult<Entity>>, GraphError> {
        let mut result = self
            .graph
            .execute(q)
            .await
            .map_err(|e| GraphError::Query(e.to_string()))?;

        let mut results = Vec::new();
        while let Some(row) = result
            .next()
            .await
            .map_err(|e| GraphError::Query(e.to_string()))?
        {
            let node: neo4rs::Node = row
                .get("node")
                .map_err(|e| GraphError::Query(format!("Missing 'node': {}", e)))?;
            let score: f64 = row
                .get("score")
                .map_err(|e| GraphError::Query(format!("Missing 'score': {}", e)))?;
            let entity = node_to_entity(&node)?;
            results.push(SearchResult {
                item: entity,
                score,
            });
        }
        Ok(results)
    }

    /// Search claims by content, entity references, or temporal filters.
    pub async fn search_claims(
        &self,
        params: &ClaimSearchParams,
        query_embedding: Option<Vec<f32>>,
    ) -> Result<Vec<SearchResult<Claim>>, GraphError> {
        let start = std::time::Instant::now();
        let limit = params.limit.unwrap_or(20) as i64;

        // Determine the search approach based on what parameters are provided.
        let results = if let Some(ref query_text) = params.query {
            let mode = params.mode.as_ref().unwrap_or(&SearchMode::Keyword);
            match mode {
                SearchMode::Semantic => {
                    let embedding = query_embedding.ok_or_else(|| {
                        GraphError::Query("Semantic claim search requires a query embedding".into())
                    })?;
                    let emb_f64: Vec<f64> = embedding.iter().map(|&f| f as f64).collect();

                    let mut where_parts = Vec::new();
                    self.add_claim_filters(params, &mut where_parts);

                    let where_str = if where_parts.is_empty() {
                        String::new()
                    } else {
                        format!(" WHERE {}", where_parts.join(" AND "))
                    };

                    let cypher = format!(
                        "CALL db.index.vector.queryNodes('claim_embedding', $limit, $embedding) \
                         YIELD node AS c, score{} \
                         OPTIONAL MATCH (source:Entity)-[:PUBLISHED]->(c) \
                         OPTIONAL MATCH (c)-[:REFERENCES]->(ref:Entity) \
                         RETURN c, source.id AS source_id, collect(ref.id) AS ref_ids, score \
                         ORDER BY score DESC",
                        where_str
                    );

                    let q = query(&cypher)
                        .param("limit", limit)
                        .param("embedding", emb_f64);

                    let q = self.bind_claim_filter_params(params, q);

                    self.execute_claim_search(q).await?
                }
                SearchMode::Keyword => {
                    let mut where_parts = Vec::new();
                    self.add_claim_filters(params, &mut where_parts);

                    let where_str = if where_parts.is_empty() {
                        String::new()
                    } else {
                        format!(" WHERE {}", where_parts.join(" AND "))
                    };

                    let cypher = format!(
                        "CALL db.index.fulltext.queryNodes('claim_content_fulltext', $query) \
                         YIELD node AS c, score{} \
                         OPTIONAL MATCH (source:Entity)-[:PUBLISHED]->(c) \
                         OPTIONAL MATCH (c)-[:REFERENCES]->(ref:Entity) \
                         RETURN c, source.id AS source_id, collect(ref.id) AS ref_ids, score \
                         ORDER BY score DESC LIMIT $limit",
                        where_str
                    );

                    let escaped_query = escape_lucene_query(query_text);
                    let q = query(&cypher)
                        .param("query", escaped_query.as_str())
                        .param("limit", limit);

                    let q = self.bind_claim_filter_params(params, q);

                    self.execute_claim_search(q).await?
                }
            }
        } else {
            // No query text — filter-only search (by entity, time range, etc.).
            let mut where_parts = Vec::new();
            let mut match_parts = vec!["MATCH (c:Claim)".to_string()];

            if params.source_entity_id.is_some() {
                match_parts.push(
                    "MATCH (source_filter:Entity {id: $source_entity_id})-[:PUBLISHED]->(c)"
                        .to_string(),
                );
            }
            if params.referenced_entity_id.is_some() {
                match_parts.push(
                    "MATCH (c)-[:REFERENCES]->(ref_filter:Entity {id: $referenced_entity_id})"
                        .to_string(),
                );
            }

            self.add_claim_temporal_filters(params, &mut where_parts);

            if let Some(ref depth) = params.attribution_depth {
                let depth_str = match depth {
                    AttributionDepth::Primary => "primary",
                    AttributionDepth::Secondhand => "secondhand",
                };
                where_parts.push(format!("c.attribution_depth = '{}'", depth_str));
            }

            let where_str = if where_parts.is_empty() {
                String::new()
            } else {
                format!(" WHERE {}", where_parts.join(" AND "))
            };

            let cypher = format!(
                "{}{} \
                 OPTIONAL MATCH (source:Entity)-[:PUBLISHED]->(c) \
                 OPTIONAL MATCH (c)-[:REFERENCES]->(ref:Entity) \
                 RETURN c, source.id AS source_id, collect(ref.id) AS ref_ids, 1.0 AS score \
                 ORDER BY c.ingested_timestamp DESC LIMIT $limit",
                match_parts.join(" "),
                where_str
            );

            let mut q = query(&cypher).param("limit", limit);

            if let Some(ref source_id) = params.source_entity_id {
                q = q.param("source_entity_id", source_id.to_string());
            }
            if let Some(ref ref_id) = params.referenced_entity_id {
                q = q.param("referenced_entity_id", ref_id.to_string());
            }
            if let Some(ref after) = params.published_after {
                q = q.param("published_after", format_datetime(after));
            }
            if let Some(ref before) = params.published_before {
                q = q.param("published_before", format_datetime(before));
            }

            self.execute_claim_search(q).await?
        };

        let mode_label = params
            .query
            .as_ref()
            .map(
                |_| match params.mode.as_ref().unwrap_or(&SearchMode::Keyword) {
                    SearchMode::Semantic => "semantic",
                    SearchMode::Keyword => "keyword",
                },
            )
            .unwrap_or("filter");

        metrics::histogram!("graph.search.latency", "mode" => mode_label, "target" => "claim")
            .record(start.elapsed().as_secs_f64());
        metrics::histogram!("graph.search.results", "target" => "claim")
            .record(results.len() as f64);

        Ok(results)
    }

    fn add_claim_filters(&self, params: &ClaimSearchParams, where_parts: &mut Vec<String>) {
        self.add_claim_temporal_filters(params, where_parts);

        if let Some(ref depth) = params.attribution_depth {
            let depth_str = match depth {
                AttributionDepth::Primary => "primary",
                AttributionDepth::Secondhand => "secondhand",
            };
            where_parts.push(format!("c.attribution_depth = '{}'", depth_str));
        }
    }

    fn add_claim_temporal_filters(
        &self,
        params: &ClaimSearchParams,
        where_parts: &mut Vec<String>,
    ) {
        if params.published_after.is_some() {
            where_parts.push("c.published_timestamp >= $published_after".to_string());
        }
        if params.published_before.is_some() {
            where_parts.push("c.published_timestamp <= $published_before".to_string());
        }
    }

    fn bind_claim_filter_params(
        &self,
        params: &ClaimSearchParams,
        q: neo4rs::Query,
    ) -> neo4rs::Query {
        let q = if let Some(ref after) = params.published_after {
            q.param("published_after", format_datetime(after))
        } else {
            q
        };
        if let Some(ref before) = params.published_before {
            q.param("published_before", format_datetime(before))
        } else {
            q
        }
    }

    async fn execute_claim_search(
        &self,
        q: neo4rs::Query,
    ) -> Result<Vec<SearchResult<Claim>>, GraphError> {
        let mut result = self
            .graph
            .execute(q)
            .await
            .map_err(|e| GraphError::Query(e.to_string()))?;

        let mut results = Vec::new();
        while let Some(row) = result
            .next()
            .await
            .map_err(|e| GraphError::Query(e.to_string()))?
        {
            let node: neo4rs::Node = row
                .get("c")
                .map_err(|e| GraphError::Query(format!("Missing 'c': {}", e)))?;
            let score: f64 = row
                .get("score")
                .map_err(|e| GraphError::Query(format!("Missing 'score': {}", e)))?;

            let source_id_str: String = row
                .get("source_id")
                .map_err(|e| GraphError::Query(format!("Missing 'source_id': {}", e)))?;
            let ref_id_strs: Vec<String> = row
                .get("ref_ids")
                .map_err(|e| GraphError::Query(format!("Missing 'ref_ids': {}", e)))?;

            let source_entity_id = parse_entity_id(&source_id_str)?;
            let referenced_entity_ids = ref_id_strs
                .iter()
                .map(|s| parse_entity_id(s))
                .collect::<Result<Vec<_>, _>>()?;

            let claim = node_to_claim(&node, source_entity_id, referenced_entity_ids)?;
            results.push(SearchResult { item: claim, score });
        }
        Ok(results)
    }

    /// Search relationships by semantic similarity.
    pub async fn search_relationships(
        &self,
        params: &RelationshipSearchParams,
        query_embedding: Option<Vec<f32>>,
    ) -> Result<Vec<SearchResult<Relationship>>, GraphError> {
        let start = std::time::Instant::now();
        let limit = params.limit.unwrap_or(20) as i64;

        let embedding = query_embedding.ok_or_else(|| {
            GraphError::Query("Relationship search requires a query embedding".into())
        })?;
        let emb_f64: Vec<f64> = embedding.iter().map(|&f| f as f64).collect();

        let cypher =
            "CALL db.index.vector.queryRelationships('relates_to_embedding', $limit, $embedding) \
             YIELD relationship AS r, score \
             MATCH (s:Entity)-[r]->(t:Entity) \
             RETURN r, s.id AS source_id, t.id AS target_id, score \
             ORDER BY score DESC";

        let q = query(cypher)
            .param("limit", limit)
            .param("embedding", emb_f64);

        let mut result = self
            .graph
            .execute(q)
            .await
            .map_err(|e| GraphError::Query(e.to_string()))?;

        let mut results = Vec::new();
        while let Some(row) = result
            .next()
            .await
            .map_err(|e| GraphError::Query(e.to_string()))?
        {
            let rel: neo4rs::Relation = row
                .get("r")
                .map_err(|e| GraphError::Query(format!("Missing 'r': {}", e)))?;
            let score: f64 = row
                .get("score")
                .map_err(|e| GraphError::Query(format!("Missing 'score': {}", e)))?;
            let source_id_str: String = row
                .get("source_id")
                .map_err(|e| GraphError::Query(format!("Missing 'source_id': {}", e)))?;
            let target_id_str: String = row
                .get("target_id")
                .map_err(|e| GraphError::Query(format!("Missing 'target_id': {}", e)))?;

            let relationship = relation_to_relationship(
                &rel,
                parse_entity_id(&source_id_str)?,
                parse_entity_id(&target_id_str)?,
            )?;
            results.push(SearchResult {
                item: relationship,
                score,
            });
        }

        metrics::histogram!("graph.search.latency", "mode" => "semantic", "target" => "relationship")
            .record(start.elapsed().as_secs_f64());
        metrics::histogram!("graph.search.results", "target" => "relationship")
            .record(results.len() as f64);

        Ok(results)
    }
}
