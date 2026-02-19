use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ids::{AssessmentId, ClaimId, EntityId, InvestigationId};

/// Confidence level for an assessment.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    High,
    Moderate,
    Low,
}

impl Confidence {
    pub fn as_db_str(&self) -> &'static str {
        match self {
            Self::High => "high",
            Self::Moderate => "moderate",
            Self::Low => "low",
        }
    }
}

/// An assessment — the Analyst's analytical product.
///
/// Distinct from claims: assessments are the system's synthesis, not raw
/// information. Stored in PostgreSQL (assessment store), NOT in the
/// knowledge graph.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Assessment {
    pub id: AssessmentId,
    pub investigation_id: InvestigationId,
    /// Structured assessment content. Schema defined alongside prompt engineering.
    /// Contains conclusions, reasoning, competing hypotheses, gaps, indicators.
    pub content: Value,
    pub confidence: Confidence,
    /// Neo4j entity IDs referenced by this assessment (cross-database refs).
    #[serde(default)]
    pub entity_refs: Vec<EntityId>,
    /// Neo4j claim IDs referenced by this assessment (cross-database refs).
    #[serde(default)]
    pub claim_refs: Vec<ClaimId>,
    /// Embedding for semantic search over assessments via pgvector.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
    pub created_at: DateTime<Utc>,
}

impl Assessment {
    pub fn new(investigation_id: InvestigationId, content: Value, confidence: Confidence) -> Self {
        Self {
            id: AssessmentId::new(),
            investigation_id,
            content,
            confidence,
            entity_refs: Vec::new(),
            claim_refs: Vec::new(),
            embedding: None,
            created_at: Utc::now(),
        }
    }
}
