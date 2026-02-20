use neo4rs::query;

use autosint_common::types::{Entity, Relationship};
use autosint_common::{EntityId, RelationshipId};

use super::conversions::{
    format_datetime, node_to_entity, parse_entity_id, relation_to_relationship,
};
use super::GraphError;

/// Relationship update with optional fields for partial updates.
#[allow(dead_code)]
pub struct RelationshipUpdate {
    pub description: Option<String>,
    pub weight: Option<f64>,
    pub confidence: Option<f64>,
    pub bidirectional: Option<bool>,
    pub timestamp: Option<chrono::DateTime<chrono::Utc>>,
}

/// Direction filter for relationship traversal.
#[allow(dead_code)]
pub enum TraversalDirection {
    Outgoing,
    Incoming,
    Both,
}

/// Parameters for traversing relationships from an entity.
#[allow(dead_code)]
pub struct TraversalParams {
    pub direction: Option<TraversalDirection>,
    pub min_weight: Option<f64>,
    pub limit: Option<u32>,
}

#[allow(dead_code)]
impl super::GraphClient {
    /// Create a new relationship (RELATES_TO edge) between two entities.
    pub async fn create_relationship(
        &self,
        relationship: &Relationship,
        embedding: Option<Vec<f32>>,
    ) -> Result<Relationship, GraphError> {
        let start = std::time::Instant::now();

        let has_embedding = embedding.is_some();
        let embedding_f64: Vec<f64> = embedding
            .as_ref()
            .map(|v| v.iter().map(|&f| f as f64).collect())
            .unwrap_or_default();
        let embedding_pending = !has_embedding;

        let mut set_parts = vec![
            "r.id = $id".to_string(),
            "r.description = $description".to_string(),
            "r.bidirectional = $bidirectional".to_string(),
            "r.embedding_pending = $embedding_pending".to_string(),
        ];

        if relationship.weight.is_some() {
            set_parts.push("r.weight = $weight".to_string());
        }
        if relationship.confidence.is_some() {
            set_parts.push("r.confidence = $confidence".to_string());
        }
        if relationship.timestamp.is_some() {
            set_parts.push("r.timestamp = $timestamp".to_string());
        }
        if has_embedding {
            set_parts.push("r.embedding = $embedding".to_string());
        }

        let cypher = format!(
            "MATCH (s:Entity {{id: $source_id}}), (t:Entity {{id: $target_id}}) \
             CREATE (s)-[r:RELATES_TO]->(t) \
             SET {} \
             RETURN r, s.id AS source_id, t.id AS target_id",
            set_parts.join(", ")
        );

        let mut q = query(&cypher)
            .param("source_id", relationship.source_entity_id.to_string())
            .param("target_id", relationship.target_entity_id.to_string())
            .param("id", relationship.id.to_string())
            .param("description", relationship.description.as_str())
            .param("bidirectional", relationship.bidirectional)
            .param("embedding_pending", embedding_pending);

        if let Some(weight) = relationship.weight {
            q = q.param("weight", weight);
        }
        if let Some(confidence) = relationship.confidence {
            q = q.param("confidence", confidence);
        }
        if let Some(ref ts) = relationship.timestamp {
            q = q.param("timestamp", format_datetime(ts));
        }
        if has_embedding {
            q = q.param("embedding", embedding_f64);
        }

        let mut result = self
            .graph
            .execute(q)
            .await
            .map_err(|e| GraphError::Query(e.to_string()))?;

        let row = result
            .next()
            .await
            .map_err(|e| GraphError::Query(e.to_string()))?
            .ok_or_else(|| {
                GraphError::NotFound(format!(
                    "Source entity {} or target entity {}",
                    relationship.source_entity_id, relationship.target_entity_id
                ))
            })?;

        let rel: neo4rs::Relation = row
            .get("r")
            .map_err(|e| GraphError::Query(format!("Missing 'r' column: {}", e)))?;
        let source_id_str: String = row
            .get("source_id")
            .map_err(|e| GraphError::Query(format!("Missing 'source_id' column: {}", e)))?;
        let target_id_str: String = row
            .get("target_id")
            .map_err(|e| GraphError::Query(format!("Missing 'target_id' column: {}", e)))?;

        let created = relation_to_relationship(
            &rel,
            parse_entity_id(&source_id_str)?,
            parse_entity_id(&target_id_str)?,
        )?;

        metrics::histogram!("graph.relationship.create.latency")
            .record(start.elapsed().as_secs_f64());

        Ok(created)
    }

    /// Update a relationship with partial fields.
    pub async fn update_relationship(
        &self,
        id: RelationshipId,
        update: &RelationshipUpdate,
        embedding: Option<Vec<f32>>,
    ) -> Result<Relationship, GraphError> {
        let start = std::time::Instant::now();

        let mut set_clauses = Vec::new();

        if update.description.is_some() {
            set_clauses.push("r.description = $description".to_string());
        }
        if update.weight.is_some() {
            set_clauses.push("r.weight = $weight".to_string());
        }
        if update.confidence.is_some() {
            set_clauses.push("r.confidence = $confidence".to_string());
        }
        if update.bidirectional.is_some() {
            set_clauses.push("r.bidirectional = $bidirectional".to_string());
        }
        if update.timestamp.is_some() {
            set_clauses.push("r.timestamp = $timestamp".to_string());
        }
        if embedding.is_some() {
            set_clauses.push("r.embedding = $embedding".to_string());
            set_clauses.push("r.embedding_pending = false".to_string());
        }

        if set_clauses.is_empty() {
            // Nothing to update — just return current state.
            return self.get_relationship(id).await;
        }

        let cypher = format!(
            "MATCH (s:Entity)-[r:RELATES_TO {{id: $id}}]->(t:Entity) \
             SET {} \
             RETURN r, s.id AS source_id, t.id AS target_id",
            set_clauses.join(", ")
        );

        let mut q = query(&cypher).param("id", id.to_string());

        if let Some(ref desc) = update.description {
            q = q.param("description", desc.as_str());
        }
        if let Some(weight) = update.weight {
            q = q.param("weight", weight);
        }
        if let Some(confidence) = update.confidence {
            q = q.param("confidence", confidence);
        }
        if let Some(bidirectional) = update.bidirectional {
            q = q.param("bidirectional", bidirectional);
        }
        if let Some(ref ts) = update.timestamp {
            q = q.param("timestamp", format_datetime(ts));
        }
        if let Some(ref emb) = embedding {
            let emb_f64: Vec<f64> = emb.iter().map(|&f| f as f64).collect();
            q = q.param("embedding", emb_f64);
        }

        let mut result = self
            .graph
            .execute(q)
            .await
            .map_err(|e| GraphError::Query(e.to_string()))?;

        let row = result
            .next()
            .await
            .map_err(|e| GraphError::Query(e.to_string()))?
            .ok_or_else(|| GraphError::NotFound(format!("Relationship {}", id)))?;

        let rel: neo4rs::Relation = row
            .get("r")
            .map_err(|e| GraphError::Query(format!("Missing 'r' column: {}", e)))?;
        let source_id_str: String = row
            .get("source_id")
            .map_err(|e| GraphError::Query(format!("Missing 'source_id' column: {}", e)))?;
        let target_id_str: String = row
            .get("target_id")
            .map_err(|e| GraphError::Query(format!("Missing 'target_id' column: {}", e)))?;

        let updated = relation_to_relationship(
            &rel,
            parse_entity_id(&source_id_str)?,
            parse_entity_id(&target_id_str)?,
        )?;

        metrics::histogram!("graph.relationship.update.latency")
            .record(start.elapsed().as_secs_f64());

        Ok(updated)
    }

    /// Get a single relationship by ID.
    pub async fn get_relationship(&self, id: RelationshipId) -> Result<Relationship, GraphError> {
        let q = query(
            "MATCH (s:Entity)-[r:RELATES_TO {id: $id}]->(t:Entity) \
             RETURN r, s.id AS source_id, t.id AS target_id",
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
            .ok_or_else(|| GraphError::NotFound(format!("Relationship {}", id)))?;

        let rel: neo4rs::Relation = row
            .get("r")
            .map_err(|e| GraphError::Query(format!("Missing 'r' column: {}", e)))?;
        let source_id_str: String = row
            .get("source_id")
            .map_err(|e| GraphError::Query(format!("Missing 'source_id' column: {}", e)))?;
        let target_id_str: String = row
            .get("target_id")
            .map_err(|e| GraphError::Query(format!("Missing 'target_id' column: {}", e)))?;

        relation_to_relationship(
            &rel,
            parse_entity_id(&source_id_str)?,
            parse_entity_id(&target_id_str)?,
        )
    }

    /// Traverse relationships from an entity with direction/weight/limit filters.
    /// Returns pairs of (Relationship, connected Entity).
    pub async fn traverse_relationships(
        &self,
        entity_id: EntityId,
        params: &TraversalParams,
    ) -> Result<Vec<(Relationship, Entity)>, GraphError> {
        let start = std::time::Instant::now();

        // Build direction-specific MATCH patterns.
        let direction = params
            .direction
            .as_ref()
            .unwrap_or(&TraversalDirection::Both);

        let mut where_clauses = Vec::new();
        if let Some(min_weight) = params.min_weight {
            where_clauses.push(format!("r.weight >= {}", min_weight));
        }

        let where_str = if where_clauses.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", where_clauses.join(" AND "))
        };

        let limit = params.limit.unwrap_or(100);

        let cypher = match direction {
            TraversalDirection::Outgoing => format!(
                "MATCH (start:Entity {{id: $entity_id}})-[r:RELATES_TO]->(other:Entity){} \
                 RETURN r, start.id AS source_id, other.id AS target_id, other AS connected \
                 LIMIT $limit",
                where_str
            ),
            TraversalDirection::Incoming => format!(
                "MATCH (other:Entity)-[r:RELATES_TO]->(start:Entity {{id: $entity_id}}){} \
                 RETURN r, other.id AS source_id, start.id AS target_id, other AS connected \
                 LIMIT $limit",
                where_str
            ),
            TraversalDirection::Both => format!(
                "MATCH (start:Entity {{id: $entity_id}})-[r:RELATES_TO]-(other:Entity){} \
                 WITH r, \
                      CASE WHEN startNode(r) = start THEN start.id ELSE other.id END AS source_id, \
                      CASE WHEN endNode(r) = start THEN start.id ELSE other.id END AS target_id, \
                      other AS connected \
                 RETURN r, source_id, target_id, connected \
                 LIMIT $limit",
                where_str
            ),
        };

        let q = query(&cypher)
            .param("entity_id", entity_id.to_string())
            .param("limit", limit as i64);

        let mut result = self
            .graph
            .execute(q)
            .await
            .map_err(|e| GraphError::Query(e.to_string()))?;

        let mut pairs = Vec::new();

        while let Some(row) = result
            .next()
            .await
            .map_err(|e| GraphError::Query(e.to_string()))?
        {
            let rel: neo4rs::Relation = row
                .get("r")
                .map_err(|e| GraphError::Query(format!("Missing 'r': {}", e)))?;
            let source_id_str: String = row
                .get("source_id")
                .map_err(|e| GraphError::Query(format!("Missing 'source_id': {}", e)))?;
            let target_id_str: String = row
                .get("target_id")
                .map_err(|e| GraphError::Query(format!("Missing 'target_id': {}", e)))?;
            let connected_node: neo4rs::Node = row
                .get("connected")
                .map_err(|e| GraphError::Query(format!("Missing 'connected': {}", e)))?;

            let relationship = relation_to_relationship(
                &rel,
                parse_entity_id(&source_id_str)?,
                parse_entity_id(&target_id_str)?,
            )?;
            let entity = node_to_entity(&connected_node)?;

            // For bidirectional edges, also include when traversing from the "wrong" direction.
            pairs.push((relationship, entity));
        }

        metrics::histogram!("graph.relationship.traverse.latency")
            .record(start.elapsed().as_secs_f64());

        Ok(pairs)
    }
}
