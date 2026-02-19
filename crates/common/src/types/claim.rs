use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{ClaimId, EntityId};

/// Attribution depth: primary source vs secondhand reporting.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AttributionDepth {
    /// Direct source (the actual statement, filing, data).
    Primary,
    /// Reported by an intermediary without direct quotes.
    Secondhand,
}

/// A claim in the knowledge graph.
///
/// Claims are units of information, not text. They scale with information
/// density, not word count. A 50-page SEC filing produces many claims;
/// a 1,500-word article might produce 3.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Claim {
    pub id: ClaimId,
    /// The information content (LLM-decided format and depth).
    pub content: String,
    /// When the information was published/occurred in the real world.
    pub published_timestamp: DateTime<Utc>,
    /// When the system added this claim to the graph.
    pub ingested_timestamp: DateTime<Utc>,
    /// URL/reference back to the original document.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_source_link: Option<String>,
    /// Primary source vs secondhand reporting.
    pub attribution_depth: AttributionDepth,
    /// Source entity (publication/outlet) that produced this claim.
    /// Linked via PUBLISHED edge in Neo4j.
    pub source_entity_id: EntityId,
    /// Entities this claim is about. Linked via REFERENCES edges in Neo4j.
    #[serde(default)]
    pub referenced_entity_ids: Vec<EntityId>,
    /// Embedding vector (content). None if embedding_pending.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
    #[serde(default)]
    pub embedding_pending: bool,
}

impl Claim {
    pub fn new(
        content: String,
        published_timestamp: DateTime<Utc>,
        attribution_depth: AttributionDepth,
        source_entity_id: EntityId,
    ) -> Self {
        Self {
            id: ClaimId::new(),
            content,
            published_timestamp,
            ingested_timestamp: Utc::now(),
            raw_source_link: None,
            attribution_depth,
            source_entity_id,
            referenced_entity_ids: Vec::new(),
            embedding: None,
            embedding_pending: false,
        }
    }
}
