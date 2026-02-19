use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ids::{EntityId, InvestigationId, WorkOrderId};

/// Work order lifecycle states.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkOrderStatus {
    Queued,
    Processing,
    Completed,
    Failed,
}

impl WorkOrderStatus {
    pub fn as_db_str(&self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Processing => "processing",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

/// Priority levels for work order queue routing.
/// Maps to Redis streams: workorders:high, workorders:normal, workorders:low.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkOrderPriority {
    High,
    #[default]
    Normal,
    Low,
}

impl WorkOrderPriority {
    pub fn as_redis_stream(&self) -> &'static str {
        match self {
            Self::High => "workorders:high",
            Self::Normal => "workorders:normal",
            Self::Low => "workorders:low",
        }
    }

    pub fn as_db_int(&self) -> i32 {
        match self {
            Self::High => 2,
            Self::Normal => 1,
            Self::Low => 0,
        }
    }
}

/// Directional hints about where to look for information.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SourceGuidance {
    /// Preferred source adapter IDs or types.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prefer: Vec<String>,
    /// Additional hints as freeform key-value.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

/// A work order — a discovery directive from the Analyst to Processors.
///
/// Directs WHERE to look and WHAT to look for, NOT what to extract.
/// Extraction is always comprehensive.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkOrder {
    pub id: WorkOrderId,
    pub investigation_id: InvestigationId,
    /// What to find. A search directive, not an analytical question.
    pub objective: String,
    pub status: WorkOrderStatus,
    pub priority: WorkOrderPriority,
    /// Existing graph entities this relates to, for linking and dedup.
    #[serde(default)]
    pub referenced_entities: Vec<EntityId>,
    /// Where to look (specific source adapters, web search, etc.).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_guidance: Option<SourceGuidance>,
    /// Which processor handled this work order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub processor_id: Option<String>,
    pub created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
}

impl WorkOrder {
    pub fn new(
        investigation_id: InvestigationId,
        objective: String,
        priority: WorkOrderPriority,
    ) -> Self {
        Self {
            id: WorkOrderId::new(),
            investigation_id,
            objective,
            status: WorkOrderStatus::Queued,
            priority,
            referenced_entities: Vec::new(),
            source_guidance: None,
            processor_id: None,
            created_at: Utc::now(),
            completed_at: None,
        }
    }
}

/// Redis stream message payload for work order dispatch.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkOrderMessage {
    pub work_order_id: WorkOrderId,
    pub investigation_id: InvestigationId,
    pub objective: String,
    pub referenced_entities: Vec<EntityId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_guidance: Option<SourceGuidance>,
}

impl From<&WorkOrder> for WorkOrderMessage {
    fn from(wo: &WorkOrder) -> Self {
        Self {
            work_order_id: wo.id,
            investigation_id: wo.investigation_id,
            objective: wo.objective.clone(),
            referenced_entities: wo.referenced_entities.clone(),
            source_guidance: wo.source_guidance.clone(),
        }
    }
}
