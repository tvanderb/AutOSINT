use std::collections::HashMap;

use neo4rs::query;
use serde_json::Value;

use autosint_common::types::Entity;
use autosint_common::EntityId;

use super::conversions::{build_aliases_text, flatten_properties, format_datetime, node_to_entity};
use super::GraphError;

/// Entity update with optional fields for partial updates.
#[allow(dead_code)]
pub struct EntityUpdate {
    pub canonical_name: Option<String>,
    pub aliases: Option<Vec<String>>,
    pub kind: Option<String>,
    pub summary: Option<String>,
    pub is_stub: Option<bool>,
    pub properties: Option<HashMap<String, Value>>,
}

#[allow(dead_code)]
impl super::GraphClient {
    /// Create a new entity in the knowledge graph.
    /// If embedding is provided, it's stored directly. Otherwise, embedding_pending is set.
    pub async fn create_entity(
        &self,
        entity: &Entity,
        embedding: Option<Vec<f32>>,
    ) -> Result<Entity, GraphError> {
        let start = std::time::Instant::now();

        let aliases_json = serde_json::to_string(&entity.aliases)
            .map_err(|e| GraphError::Query(format!("Failed to serialize aliases: {}", e)))?;
        let aliases_text = build_aliases_text(&entity.aliases);
        let last_updated = format_datetime(&entity.last_updated);

        let has_embedding = embedding.is_some();
        let embedding_f64: Vec<f64> = embedding
            .as_ref()
            .map(|v| v.iter().map(|&f| f as f64).collect())
            .unwrap_or_default();

        // Build the base CREATE query with all schema fields.
        let mut cypher = String::from(
            "CREATE (e:Entity { \
                id: $id, \
                canonical_name: $canonical_name, \
                aliases: $aliases, \
                aliases_text: $aliases_text, \
                kind: $kind, \
                is_stub: $is_stub, \
                last_updated: $last_updated, \
                embedding_pending: $embedding_pending \
            })",
        );

        // Add optional fields via SET.
        if entity.summary.is_some() {
            cypher.push_str(" SET e.summary = $summary");
        }
        if has_embedding {
            if entity.summary.is_some() {
                cypher.push_str(", e.embedding = $embedding");
            } else {
                cypher.push_str(" SET e.embedding = $embedding");
            }
        }

        // Add freeform properties.
        let flat_props = flatten_properties(&entity.properties);
        for (i, (prop_key, _)) in flat_props.iter().enumerate() {
            if i == 0 && entity.summary.is_none() && !has_embedding {
                cypher.push_str(&format!(" SET e.`{}` = $prop_{}", prop_key, i));
            } else {
                cypher.push_str(&format!(", e.`{}` = $prop_{}", prop_key, i));
            }
        }

        cypher.push_str(" RETURN e");

        let embedding_pending = !has_embedding;

        let mut q = query(&cypher)
            .param("id", entity.id.to_string())
            .param("canonical_name", entity.canonical_name.as_str())
            .param("aliases", aliases_json.as_str())
            .param("aliases_text", aliases_text.as_str())
            .param("kind", entity.kind.as_str())
            .param("is_stub", entity.is_stub)
            .param("last_updated", last_updated.as_str())
            .param("embedding_pending", embedding_pending);

        if let Some(ref summary) = entity.summary {
            q = q.param("summary", summary.as_str());
        }
        if has_embedding {
            q = q.param("embedding", embedding_f64);
        }

        for (i, (_, value)) in flat_props.iter().enumerate() {
            q = q.param(&format!("prop_{}", i), value.as_str());
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
            .ok_or_else(|| GraphError::Query("CREATE returned no rows".into()))?;

        let node: neo4rs::Node = row
            .get("e")
            .map_err(|e| GraphError::Query(format!("Missing 'e' column: {}", e)))?;

        let created = node_to_entity(&node)?;

        metrics::histogram!("graph.entity.create.latency").record(start.elapsed().as_secs_f64());

        Ok(created)
    }

    /// Get an entity by ID.
    pub async fn get_entity(&self, id: EntityId) -> Result<Entity, GraphError> {
        let start = std::time::Instant::now();

        let q = query("MATCH (e:Entity {id: $id}) RETURN e").param("id", id.to_string());

        let mut result = self
            .graph
            .execute(q)
            .await
            .map_err(|e| GraphError::Query(e.to_string()))?;

        let row = result
            .next()
            .await
            .map_err(|e| GraphError::Query(e.to_string()))?
            .ok_or_else(|| GraphError::NotFound(format!("Entity {}", id)))?;

        let node: neo4rs::Node = row
            .get("e")
            .map_err(|e| GraphError::Query(format!("Missing 'e' column: {}", e)))?;

        let entity = node_to_entity(&node)?;

        metrics::histogram!("graph.entity.get.latency").record(start.elapsed().as_secs_f64());

        Ok(entity)
    }

    /// Update an entity with partial fields.
    /// If name or summary changed and embedding is provided, updates the embedding.
    pub async fn update_entity(
        &self,
        id: EntityId,
        update: &EntityUpdate,
        embedding: Option<Vec<f32>>,
    ) -> Result<Entity, GraphError> {
        let start = std::time::Instant::now();

        let mut set_clauses = Vec::new();
        let mut params: Vec<(&str, Box<dyn std::any::Any + Send>)> = Vec::new();

        // We'll build the query dynamically based on which fields are present.
        // Use a simpler approach: always SET last_updated, conditionally SET other fields.
        let now = format_datetime(&chrono::Utc::now());

        let mut cypher = String::from("MATCH (e:Entity {id: $id})");
        set_clauses.push("e.last_updated = $last_updated".to_string());

        if let Some(ref name) = update.canonical_name {
            set_clauses.push("e.canonical_name = $canonical_name".to_string());
            let _ = (name, &mut params); // params used below with query builder
        }
        if let Some(ref aliases) = update.aliases {
            set_clauses.push("e.aliases = $aliases".to_string());
            set_clauses.push("e.aliases_text = $aliases_text".to_string());
            let _ = (aliases, &mut params);
        }
        if let Some(ref kind) = update.kind {
            set_clauses.push("e.kind = $kind".to_string());
            let _ = (kind, &mut params);
        }
        if update.summary.is_some() {
            set_clauses.push("e.summary = $summary".to_string());
        }
        if let Some(is_stub) = update.is_stub {
            set_clauses.push("e.is_stub = $is_stub".to_string());
            let _ = (is_stub, &mut params);
        }
        if embedding.is_some() {
            set_clauses.push("e.embedding = $embedding".to_string());
            set_clauses.push("e.embedding_pending = false".to_string());
        }

        // Handle freeform property updates.
        let flat_props = update
            .properties
            .as_ref()
            .map(flatten_properties)
            .unwrap_or_default();

        for (i, (prop_key, _)) in flat_props.iter().enumerate() {
            set_clauses.push(format!("e.`{}` = $prop_{}", prop_key, i));
        }

        cypher.push_str(&format!(" SET {} RETURN e", set_clauses.join(", ")));

        let mut q = query(&cypher)
            .param("id", id.to_string())
            .param("last_updated", now.as_str());

        if let Some(ref name) = update.canonical_name {
            q = q.param("canonical_name", name.as_str());
        }
        if let Some(ref aliases) = update.aliases {
            let aliases_json = serde_json::to_string(aliases)
                .map_err(|e| GraphError::Query(format!("Failed to serialize aliases: {}", e)))?;
            let aliases_text = build_aliases_text(aliases);
            q = q
                .param("aliases", aliases_json.as_str())
                .param("aliases_text", aliases_text.as_str());
        }
        if let Some(ref kind) = update.kind {
            q = q.param("kind", kind.as_str());
        }
        if let Some(ref summary) = update.summary {
            q = q.param("summary", summary.as_str());
        }
        if let Some(is_stub) = update.is_stub {
            q = q.param("is_stub", is_stub);
        }
        if let Some(ref emb) = embedding {
            let emb_f64: Vec<f64> = emb.iter().map(|&f| f as f64).collect();
            q = q.param("embedding", emb_f64);
        }
        for (i, (_, value)) in flat_props.iter().enumerate() {
            q = q.param(&format!("prop_{}", i), value.as_str());
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
            .ok_or_else(|| GraphError::NotFound(format!("Entity {}", id)))?;

        let node: neo4rs::Node = row
            .get("e")
            .map_err(|e| GraphError::Query(format!("Missing 'e' column: {}", e)))?;

        let entity = node_to_entity(&node)?;

        metrics::histogram!("graph.entity.update.latency").record(start.elapsed().as_secs_f64());

        Ok(entity)
    }

    /// Merge source entity into target, reassigning all edges.
    /// - PUBLISHED edges on claims pointing to source → target
    /// - REFERENCES edges on claims pointing to source → target
    /// - RELATES_TO edges (both directions) from source → target
    /// - Combine aliases
    /// - Delete source
    pub async fn merge_entities(
        &self,
        source_id: EntityId,
        target_id: EntityId,
        _reason: Option<&str>,
    ) -> Result<Entity, GraphError> {
        let start = std::time::Instant::now();

        if source_id == target_id {
            return Err(GraphError::Query(
                "Cannot merge an entity with itself".into(),
            ));
        }

        // Verify both entities exist.
        let _ = self.get_entity(source_id).await?;
        let target = self.get_entity(target_id).await?;

        let mut txn = self
            .graph
            .start_txn()
            .await
            .map_err(|e| GraphError::Query(e.to_string()))?;

        // 1. Reassign PUBLISHED edges (source entity → claim) to target.
        let q1 = query(
            "MATCH (source:Entity {id: $source_id})-[r:PUBLISHED]->(c:Claim) \
             MATCH (target:Entity {id: $target_id}) \
             DELETE r \
             CREATE (target)-[:PUBLISHED]->(c)",
        )
        .param("source_id", source_id.to_string())
        .param("target_id", target_id.to_string());
        txn.run(q1)
            .await
            .map_err(|e| GraphError::Query(e.to_string()))?;

        // 2. Reassign REFERENCES edges (claim → source entity) to target.
        let q2 = query(
            "MATCH (c:Claim)-[r:REFERENCES]->(source:Entity {id: $source_id}) \
             MATCH (target:Entity {id: $target_id}) \
             DELETE r \
             CREATE (c)-[:REFERENCES]->(target)",
        )
        .param("source_id", source_id.to_string())
        .param("target_id", target_id.to_string());
        txn.run(q2)
            .await
            .map_err(|e| GraphError::Query(e.to_string()))?;

        // 3. Reassign outgoing RELATES_TO edges (source → other) to (target → other).
        // Skip edges that would become self-referential (source → target becomes target → target).
        let q3 = query(
            "MATCH (source:Entity {id: $source_id})-[r:RELATES_TO]->(other:Entity) \
             WHERE other.id <> $target_id \
             MATCH (target:Entity {id: $target_id}) \
             CREATE (target)-[r2:RELATES_TO]->(other) \
             SET r2 = properties(r) \
             DELETE r",
        )
        .param("source_id", source_id.to_string())
        .param("target_id", target_id.to_string());
        txn.run(q3)
            .await
            .map_err(|e| GraphError::Query(e.to_string()))?;

        // 4. Reassign incoming RELATES_TO edges (other → source) to (other → target).
        let q4 = query(
            "MATCH (other:Entity)-[r:RELATES_TO]->(source:Entity {id: $source_id}) \
             WHERE other.id <> $target_id \
             MATCH (target:Entity {id: $target_id}) \
             CREATE (other)-[r2:RELATES_TO]->(target) \
             SET r2 = properties(r) \
             DELETE r",
        )
        .param("source_id", source_id.to_string())
        .param("target_id", target_id.to_string());
        txn.run(q4)
            .await
            .map_err(|e| GraphError::Query(e.to_string()))?;

        // 5. Delete any remaining edges on source (self-referential edges that were skipped).
        let q5 = query("MATCH (source:Entity {id: $source_id})-[r:RELATES_TO]-() DELETE r")
            .param("source_id", source_id.to_string());
        txn.run(q5)
            .await
            .map_err(|e| GraphError::Query(e.to_string()))?;

        // 6. Combine aliases and update target.
        let source_entity = self.get_entity(source_id).await?;
        let mut combined_aliases = target.aliases.clone();
        // Add source's canonical name as an alias.
        if !combined_aliases.contains(&source_entity.canonical_name) {
            combined_aliases.push(source_entity.canonical_name.clone());
        }
        // Add source's aliases.
        for alias in &source_entity.aliases {
            if !combined_aliases.contains(alias) {
                combined_aliases.push(alias.clone());
            }
        }

        let aliases_json = serde_json::to_string(&combined_aliases)
            .map_err(|e| GraphError::Query(format!("Failed to serialize aliases: {}", e)))?;
        let aliases_text = build_aliases_text(&combined_aliases);
        let now = format_datetime(&chrono::Utc::now());

        let q6 = query(
            "MATCH (target:Entity {id: $target_id}) \
             SET target.aliases = $aliases, \
                 target.aliases_text = $aliases_text, \
                 target.last_updated = $last_updated, \
                 target.embedding_pending = true \
             RETURN target",
        )
        .param("target_id", target_id.to_string())
        .param("aliases", aliases_json.as_str())
        .param("aliases_text", aliases_text.as_str())
        .param("last_updated", now.as_str());
        txn.run(q6)
            .await
            .map_err(|e| GraphError::Query(e.to_string()))?;

        // 7. Delete source entity.
        let q7 = query("MATCH (source:Entity {id: $source_id}) DETACH DELETE source")
            .param("source_id", source_id.to_string());
        txn.run(q7)
            .await
            .map_err(|e| GraphError::Query(e.to_string()))?;

        txn.commit()
            .await
            .map_err(|e| GraphError::Query(e.to_string()))?;

        metrics::histogram!("graph.entity.merge.latency").record(start.elapsed().as_secs_f64());
        metrics::counter!("graph.entity.merge_count").increment(1);

        // Return the updated target entity.
        self.get_entity(target_id).await
    }
}
