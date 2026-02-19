use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::InvestigationId;

/// Investigation lifecycle states per the state machine in PLAN.md §4.7.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvestigationStatus {
    /// Record created, not yet started.
    Pending,
    /// Analyst agentic session in progress.
    AnalystRunning,
    /// Work orders dispatched, Processors working.
    Processing,
    /// Paused due to hard dependency failure. Retryable.
    Suspended,
    /// Assessment produced. Terminal.
    Completed,
    /// Unrecoverable error. Terminal (still produces partial assessment).
    Failed,
}

impl InvestigationStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed)
    }

    pub fn is_active(&self) -> bool {
        matches!(self, Self::AnalystRunning | Self::Processing)
    }

    /// Returns the string representation used in PostgreSQL.
    pub fn as_db_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::AnalystRunning => "analyst_running",
            Self::Processing => "processing",
            Self::Suspended => "suspended",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

/// An investigation record tracked in PostgreSQL.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Investigation {
    pub id: InvestigationId,
    pub prompt: String,
    pub status: InvestigationStatus,
    /// Parent investigation for multi-analyst decomposition (future).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_investigation_id: Option<InvestigationId>,
    pub cycle_count: i32,
    pub created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    /// Reason for suspension (e.g. "neo4j_unavailable", "llm_api_down").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suspended_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suspended_at: Option<DateTime<Utc>>,
    /// Where to resume from after suspension ("analyst" or "processing").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_from: Option<String>,
}

impl Investigation {
    pub fn new(prompt: String) -> Self {
        Self {
            id: InvestigationId::new(),
            prompt,
            status: InvestigationStatus::Pending,
            parent_investigation_id: None,
            cycle_count: 0,
            created_at: Utc::now(),
            completed_at: None,
            suspended_reason: None,
            suspended_at: None,
            resume_from: None,
        }
    }
}
