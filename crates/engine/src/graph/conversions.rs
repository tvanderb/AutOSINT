#![allow(dead_code)]

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use neo4rs::{Node, Relation, Row};
use serde_json::Value;
use uuid::Uuid;

use autosint_common::types::{AttributionDepth, Claim, Entity, InformationType, Relationship};
use autosint_common::{ClaimId, EntityId, RelationshipId};

use super::GraphError;

// ---------------------------------------------------------------------------
// UUID parsing
// ---------------------------------------------------------------------------

pub fn parse_uuid(s: &str) -> Result<Uuid, GraphError> {
    Uuid::parse_str(s).map_err(|e| GraphError::Query(format!("Invalid UUID '{}': {}", s, e)))
}

pub fn parse_entity_id(s: &str) -> Result<EntityId, GraphError> {
    Ok(EntityId::from_uuid(parse_uuid(s)?))
}

pub fn parse_claim_id(s: &str) -> Result<ClaimId, GraphError> {
    Ok(ClaimId::from_uuid(parse_uuid(s)?))
}

pub fn parse_relationship_id(s: &str) -> Result<RelationshipId, GraphError> {
    Ok(RelationshipId::from_uuid(parse_uuid(s)?))
}

// ---------------------------------------------------------------------------
// DateTime helpers
// ---------------------------------------------------------------------------

pub fn parse_datetime(s: &str) -> Result<DateTime<Utc>, GraphError> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| GraphError::Query(format!("Invalid datetime '{}': {}", s, e)))
}

pub fn format_datetime(dt: &DateTime<Utc>) -> String {
    dt.to_rfc3339()
}

// ---------------------------------------------------------------------------
// Aliases
// ---------------------------------------------------------------------------

/// Build a space-separated string of aliases for fulltext indexing.
pub fn build_aliases_text(aliases: &[String]) -> String {
    aliases.join(" ")
}

/// Parse a JSON-serialized aliases array from a node property.
pub fn parse_aliases(json_str: &str) -> Vec<String> {
    serde_json::from_str(json_str).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Freeform property flattening
// ---------------------------------------------------------------------------

/// Flatten a HashMap<String, Value> into (prop_key, neo4j-safe-string) pairs.
/// Nested JSON values are serialized as strings.
pub fn flatten_properties(properties: &HashMap<String, Value>) -> Vec<(String, String)> {
    properties
        .iter()
        .map(|(key, value)| {
            let flat_value = match value {
                Value::String(s) => s.clone(),
                Value::Null => String::new(),
                other => other.to_string(),
            };
            (format!("prop_{}", key), flat_value)
        })
        .collect()
}

/// Collect all prop_* properties from a Node back into a HashMap.
/// Uses Node::get which returns Result — errors are treated as missing values.
pub fn unflatten_properties_from_node(node: &Node) -> HashMap<String, Value> {
    let mut props = HashMap::new();
    for key in node.keys() {
        if let Some(stripped) = key.strip_prefix("prop_") {
            if let Ok(val) = node.get::<String>(key) {
                // Try to parse as JSON first; fall back to plain string.
                let json_val = serde_json::from_str(&val).unwrap_or(Value::String(val));
                props.insert(stripped.to_string(), json_val);
            }
        }
    }
    props
}

// ---------------------------------------------------------------------------
// Embedding text builders
// ---------------------------------------------------------------------------

/// Build the text to embed for an entity (canonical_name + summary).
pub fn embedding_text_for_entity(name: &str, summary: Option<&str>) -> String {
    match summary {
        Some(s) => format!("{}\n{}", name, s),
        None => name.to_string(),
    }
}

/// Build the text to embed for a claim (content).
pub fn embedding_text_for_claim(content: &str) -> String {
    content.to_string()
}

/// Build the text to embed for a relationship (description).
pub fn embedding_text_for_relationship(description: &str) -> String {
    description.to_string()
}

// ---------------------------------------------------------------------------
// Helper: extract required/optional fields from neo4rs Node/Relation
// neo4rs 0.8 Node::get and Relation::get return Result<T, DeError>.
// ---------------------------------------------------------------------------

fn node_get_required<T: serde::de::DeserializeOwned>(
    node: &Node,
    key: &str,
    type_name: &str,
) -> Result<T, GraphError> {
    node.get::<T>(key)
        .map_err(|_| GraphError::Query(format!("{} node missing or invalid '{}'", type_name, key)))
}

fn node_get_optional<T: serde::de::DeserializeOwned>(node: &Node, key: &str) -> Option<T> {
    node.get::<T>(key).ok()
}

fn rel_get_required<T: serde::de::DeserializeOwned>(
    rel: &Relation,
    key: &str,
) -> Result<T, GraphError> {
    rel.get::<T>(key)
        .map_err(|_| GraphError::Query(format!("Relation missing or invalid '{}'", key)))
}

fn rel_get_optional<T: serde::de::DeserializeOwned>(rel: &Relation, key: &str) -> Option<T> {
    rel.get::<T>(key).ok()
}

// ---------------------------------------------------------------------------
// Node/Relation → domain type conversions
// ---------------------------------------------------------------------------

/// Extract an Entity from a Neo4j Node.
pub fn node_to_entity(node: &Node) -> Result<Entity, GraphError> {
    let id_str: String = node_get_required(node, "id", "Entity")?;
    let canonical_name: String = node_get_required(node, "canonical_name", "Entity")?;
    let kind: String = node_get_required(node, "kind", "Entity")?;
    let last_updated_str: String = node_get_required(node, "last_updated", "Entity")?;

    let summary: Option<String> = node_get_optional(node, "summary");
    let is_stub: bool = node_get_optional(node, "is_stub").unwrap_or(false);
    let embedding_pending: bool = node_get_optional(node, "embedding_pending").unwrap_or(false);

    let aliases = match node_get_optional::<String>(node, "aliases") {
        Some(json_str) => parse_aliases(&json_str),
        None => Vec::new(),
    };

    let embedding: Option<Vec<f32>> = node_get_optional::<Vec<f64>>(node, "embedding")
        .map(|v| v.into_iter().map(|f| f as f32).collect());

    let properties = unflatten_properties_from_node(node);

    Ok(Entity {
        id: parse_entity_id(&id_str)?,
        canonical_name,
        aliases,
        kind,
        summary,
        is_stub,
        last_updated: parse_datetime(&last_updated_str)?,
        properties,
        embedding,
        embedding_pending,
    })
}

/// Extract a Claim from a Neo4j Node plus edge data.
pub fn node_to_claim(
    node: &Node,
    source_entity_id: EntityId,
    referenced_entity_ids: Vec<EntityId>,
) -> Result<Claim, GraphError> {
    let id_str: String = node_get_required(node, "id", "Claim")?;
    let content: String = node_get_required(node, "content", "Claim")?;
    let published_str: String = node_get_required(node, "published_timestamp", "Claim")?;
    let ingested_str: String = node_get_required(node, "ingested_timestamp", "Claim")?;
    let attribution_depth_str: String = node_get_required(node, "attribution_depth", "Claim")?;

    let raw_source_link: Option<String> = node_get_optional(node, "raw_source_link");
    let embedding_pending: bool = node_get_optional(node, "embedding_pending").unwrap_or(false);

    let embedding: Option<Vec<f32>> = node_get_optional::<Vec<f64>>(node, "embedding")
        .map(|v| v.into_iter().map(|f| f as f32).collect());

    let attribution_depth = match attribution_depth_str.as_str() {
        "primary" => AttributionDepth::Primary,
        "secondhand" => AttributionDepth::Secondhand,
        "indirect" => AttributionDepth::Indirect,
        other => {
            return Err(GraphError::Query(format!(
                "Unknown attribution_depth: '{}'",
                other
            )))
        }
    };

    // Backward compat: existing claims without information_type default to Assertion.
    let information_type = match node_get_optional::<String>(node, "information_type").as_deref() {
        Some("assertion") | None => InformationType::Assertion,
        Some("analysis") => InformationType::Analysis,
        Some("discourse") => InformationType::Discourse,
        Some("testimony") => InformationType::Testimony,
        Some(other) => {
            return Err(GraphError::Query(format!(
                "Unknown information_type: '{}'",
                other
            )))
        }
    };

    Ok(Claim {
        id: parse_claim_id(&id_str)?,
        content,
        published_timestamp: parse_datetime(&published_str)?,
        ingested_timestamp: parse_datetime(&ingested_str)?,
        raw_source_link,
        attribution_depth,
        information_type,
        source_entity_id,
        referenced_entity_ids,
        embedding,
        embedding_pending,
    })
}

/// Extract a Relationship from a Neo4j Relation plus endpoint entity IDs.
pub fn relation_to_relationship(
    rel: &Relation,
    source_entity_id: EntityId,
    target_entity_id: EntityId,
) -> Result<Relationship, GraphError> {
    let id_str: String = rel_get_required(rel, "id")?;
    let description: String = rel_get_required(rel, "description")?;

    let weight: Option<f64> = rel_get_optional(rel, "weight");
    let confidence: Option<f64> = rel_get_optional(rel, "confidence");
    let bidirectional: bool = rel_get_optional(rel, "bidirectional").unwrap_or(false);
    let embedding_pending: bool = rel_get_optional(rel, "embedding_pending").unwrap_or(false);

    let timestamp: Option<DateTime<Utc>> = rel_get_optional::<String>(rel, "timestamp")
        .map(|s| parse_datetime(&s))
        .transpose()?;

    let embedding: Option<Vec<f32>> = rel_get_optional::<Vec<f64>>(rel, "embedding")
        .map(|v| v.into_iter().map(|f| f as f32).collect());

    Ok(Relationship {
        id: parse_relationship_id(&id_str)?,
        source_entity_id,
        target_entity_id,
        description,
        weight,
        confidence,
        bidirectional,
        timestamp,
        embedding,
        embedding_pending,
    })
}

/// Extract a Claim from a query row.
/// Expected columns: "c" (Node), "source_id" (String), "ref_ids" (Vec<String>).
pub fn row_to_claim(row: Row) -> Result<Claim, GraphError> {
    let node: Node = row
        .get("c")
        .map_err(|e| GraphError::Query(format!("Missing 'c' column: {}", e)))?;

    let source_id_str: String = row
        .get("source_id")
        .map_err(|e| GraphError::Query(format!("Missing 'source_id' column: {}", e)))?;
    let source_entity_id = parse_entity_id(&source_id_str)?;

    let ref_id_strs: Vec<String> = row
        .get("ref_ids")
        .map_err(|e| GraphError::Query(format!("Missing 'ref_ids' column: {}", e)))?;
    let referenced_entity_ids: Vec<EntityId> = ref_id_strs
        .iter()
        .map(|s| parse_entity_id(s))
        .collect::<Result<Vec<_>, _>>()?;

    node_to_claim(&node, source_entity_id, referenced_entity_ids)
}

/// Extract a Relationship from a query row.
/// Expected columns: "r" (Relation), "source_id" (String), "target_id" (String).
pub fn row_to_relationship(row: Row) -> Result<Relationship, GraphError> {
    let rel: Relation = row
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
