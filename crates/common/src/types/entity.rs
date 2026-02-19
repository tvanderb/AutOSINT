use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ids::EntityId;

/// An entity in the knowledge graph.
///
/// Minimal universal schema plus freeform properties the LLM attaches.
/// Entities hold current state only — history lives in claims.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entity {
    pub id: EntityId,
    pub canonical_name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    /// Loose descriptive label ("organization", "person", "country", etc.).
    /// NOT a rigid type — does not control what fields exist.
    pub kind: String,
    /// LLM-generated living summary. Quick-reference orientation, not analysis.
    #[serde(default)]
    pub summary: Option<String>,
    /// Whether this is a stub entity (referenced but not fully fleshed out).
    #[serde(default)]
    pub is_stub: bool,
    pub last_updated: DateTime<Utc>,
    /// Freeform key-value properties the LLM attaches.
    /// Includes external identifiers (wikidata_qid, stock_ticker, iso_code, etc.).
    #[serde(default)]
    pub properties: HashMap<String, Value>,
    /// Embedding vector (canonical_name + summary). None if embedding_pending.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
    /// True if embedding computation failed and needs backfill.
    #[serde(default)]
    pub embedding_pending: bool,
}

impl Entity {
    pub fn new(canonical_name: String, kind: String) -> Self {
        Self {
            id: EntityId::new(),
            canonical_name,
            aliases: Vec::new(),
            kind,
            summary: None,
            is_stub: false,
            last_updated: Utc::now(),
            properties: HashMap::new(),
            embedding: None,
            embedding_pending: false,
        }
    }

    pub fn new_stub(canonical_name: String, kind: String) -> Self {
        let mut entity = Self::new(canonical_name, kind);
        entity.is_stub = true;
        entity
    }
}
