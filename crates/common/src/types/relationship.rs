use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{EntityId, RelationshipId};

/// A relationship (RELATES_TO edge) between two entities in the knowledge graph.
///
/// Descriptions are freeform natural language, searched semantically.
/// There are NO enumerated relationship types.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Relationship {
    pub id: RelationshipId,
    pub source_entity_id: EntityId,
    pub target_entity_id: EntityId,
    /// Freeform natural language description.
    /// e.g. "TSMC supplies approximately 40% of Apple's A-series chip production."
    pub description: String,
    /// Numeric significance signal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<f64>,
    /// How certain the system is about this relationship.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    /// Whether this relationship applies in both directions.
    #[serde(default)]
    pub bidirectional: bool,
    /// When this relationship was established/last confirmed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<DateTime<Utc>>,
    /// Embedding vector (description). None if embedding_pending.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
    #[serde(default)]
    pub embedding_pending: bool,
}

impl Relationship {
    pub fn new(
        source_entity_id: EntityId,
        target_entity_id: EntityId,
        description: String,
    ) -> Self {
        Self {
            id: RelationshipId::new(),
            source_entity_id,
            target_entity_id,
            description,
            weight: None,
            confidence: None,
            bidirectional: false,
            timestamp: None,
            embedding: None,
            embedding_pending: false,
        }
    }
}
