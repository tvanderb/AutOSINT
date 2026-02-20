use chrono::Utc;
use uuid::Uuid;

use autosint_common::ids::{InvestigationId, WorkOrderId};
use autosint_common::types::{SourceGuidance, WorkOrder, WorkOrderPriority, WorkOrderStatus};

use super::{StoreClient, StoreError};

impl StoreClient {
    /// Create a new work order record.
    pub async fn create_work_order(&self, wo: &WorkOrder) -> Result<WorkOrder, StoreError> {
        let referenced_entities_json =
            serde_json::to_value(&wo.referenced_entities).unwrap_or_default();
        let source_guidance_json = wo
            .source_guidance
            .as_ref()
            .map(|sg| serde_json::to_value(sg).unwrap_or_default());

        sqlx::query(
            r#"
            INSERT INTO work_orders (id, investigation_id, objective, status, priority,
                                     referenced_entities, source_guidance, cycle, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
        )
        .bind(wo.id.0)
        .bind(wo.investigation_id.0)
        .bind(&wo.objective)
        .bind(wo.status.as_db_str())
        .bind(wo.priority.as_db_int())
        .bind(&referenced_entities_json)
        .bind(&source_guidance_json)
        .bind(wo.cycle)
        .bind(wo.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Query(e.to_string()))?;

        Ok(wo.clone())
    }

    /// Retrieve a work order by ID.
    pub async fn get_work_order(&self, id: WorkOrderId) -> Result<WorkOrder, StoreError> {
        let row = sqlx::query_as::<_, WorkOrderRow>(
            r#"
            SELECT id, investigation_id, objective, status, priority,
                   referenced_entities, source_guidance, processor_id,
                   cycle, claims_produced_count, created_at, completed_at
            FROM work_orders
            WHERE id = $1
            "#,
        )
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StoreError::Query(e.to_string()))?
        .ok_or_else(|| StoreError::NotFound(format!("WorkOrder {}", id)))?;

        Ok(row.into())
    }

    /// Update work order status. Optionally sets processor_id and claims_count.
    /// Sets completed_at when transitioning to a terminal state.
    pub async fn update_work_order_status(
        &self,
        id: WorkOrderId,
        status: &WorkOrderStatus,
        processor_id: Option<&str>,
        claims_count: Option<i32>,
    ) -> Result<(), StoreError> {
        let is_terminal = matches!(status, WorkOrderStatus::Completed | WorkOrderStatus::Failed);
        let completed_at = if is_terminal { Some(Utc::now()) } else { None };

        sqlx::query(
            r#"
            UPDATE work_orders
            SET status = $2,
                processor_id = COALESCE($3, processor_id),
                claims_produced_count = COALESCE($4, claims_produced_count),
                completed_at = COALESCE($5, completed_at)
            WHERE id = $1
            "#,
        )
        .bind(id.0)
        .bind(status.as_db_str())
        .bind(processor_id)
        .bind(claims_count)
        .bind(completed_at)
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Query(e.to_string()))?;

        Ok(())
    }

    /// Get all work orders for an investigation, ordered by cycle and creation time.
    pub async fn get_work_orders_by_investigation(
        &self,
        investigation_id: InvestigationId,
    ) -> Result<Vec<WorkOrder>, StoreError> {
        let rows = sqlx::query_as::<_, WorkOrderRow>(
            r#"
            SELECT id, investigation_id, objective, status, priority,
                   referenced_entities, source_guidance, processor_id,
                   cycle, claims_produced_count, created_at, completed_at
            FROM work_orders
            WHERE investigation_id = $1
            ORDER BY cycle, created_at
            "#,
        )
        .bind(investigation_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StoreError::Query(e.to_string()))?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// Count active (queued or processing) work orders for an investigation.
    pub async fn count_active_work_orders(
        &self,
        investigation_id: InvestigationId,
    ) -> Result<i64, StoreError> {
        let row: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*)
            FROM work_orders
            WHERE investigation_id = $1
              AND status IN ('queued', 'processing')
            "#,
        )
        .bind(investigation_id.0)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| StoreError::Query(e.to_string()))?;

        Ok(row.0)
    }
}

/// Internal row type for sqlx deserialization.
#[derive(sqlx::FromRow)]
struct WorkOrderRow {
    id: Uuid,
    investigation_id: Uuid,
    objective: String,
    status: String,
    priority: i32,
    referenced_entities: Option<serde_json::Value>,
    source_guidance: Option<serde_json::Value>,
    processor_id: Option<String>,
    cycle: i32,
    claims_produced_count: i32,
    created_at: chrono::DateTime<Utc>,
    completed_at: Option<chrono::DateTime<Utc>>,
}

impl From<WorkOrderRow> for WorkOrder {
    fn from(row: WorkOrderRow) -> Self {
        let referenced_entities = row
            .referenced_entities
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();

        let source_guidance: Option<SourceGuidance> = row
            .source_guidance
            .and_then(|v| serde_json::from_value(v).ok());

        Self {
            id: WorkOrderId::from_uuid(row.id),
            investigation_id: InvestigationId::from_uuid(row.investigation_id),
            objective: row.objective,
            status: parse_work_order_status(&row.status),
            priority: parse_priority(row.priority),
            referenced_entities,
            source_guidance,
            processor_id: row.processor_id,
            cycle: row.cycle,
            claims_produced_count: row.claims_produced_count,
            created_at: row.created_at,
            completed_at: row.completed_at,
        }
    }
}

fn parse_work_order_status(s: &str) -> WorkOrderStatus {
    match s {
        "queued" => WorkOrderStatus::Queued,
        "processing" => WorkOrderStatus::Processing,
        "completed" => WorkOrderStatus::Completed,
        "failed" => WorkOrderStatus::Failed,
        other => {
            tracing::warn!(
                status = other,
                "Unknown work order status, defaulting to Queued"
            );
            WorkOrderStatus::Queued
        }
    }
}

fn parse_priority(p: i32) -> WorkOrderPriority {
    match p {
        2 => WorkOrderPriority::High,
        1 => WorkOrderPriority::Normal,
        0 => WorkOrderPriority::Low,
        _ => WorkOrderPriority::Normal,
    }
}
