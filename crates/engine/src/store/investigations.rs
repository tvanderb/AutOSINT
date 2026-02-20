use chrono::Utc;
use uuid::Uuid;

use autosint_common::ids::InvestigationId;
use autosint_common::types::{Investigation, InvestigationStatus};

use super::{StoreClient, StoreError};

impl StoreClient {
    /// Create a new investigation record.
    pub async fn create_investigation(
        &self,
        investigation: &Investigation,
    ) -> Result<Investigation, StoreError> {
        sqlx::query(
            r#"
            INSERT INTO investigations (id, prompt, status, parent_investigation_id, cycle_count, created_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(investigation.id.0)
        .bind(&investigation.prompt)
        .bind(investigation.status.as_db_str())
        .bind(investigation.parent_investigation_id.map(|id| id.0))
        .bind(investigation.cycle_count)
        .bind(investigation.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Query(e.to_string()))?;

        Ok(investigation.clone())
    }

    /// Retrieve an investigation by ID.
    pub async fn get_investigation(
        &self,
        id: InvestigationId,
    ) -> Result<Investigation, StoreError> {
        let row = sqlx::query_as::<_, InvestigationRow>(
            r#"
            SELECT id, prompt, status, parent_investigation_id, cycle_count,
                   created_at, completed_at, suspended_reason, suspended_at, resume_from
            FROM investigations
            WHERE id = $1
            "#,
        )
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StoreError::Query(e.to_string()))?
        .ok_or_else(|| StoreError::NotFound(format!("Investigation {}", id)))?;

        Ok(row.into())
    }

    /// Update investigation status. Optionally increments cycle_count.
    /// Sets completed_at when transitioning to a terminal state.
    pub async fn update_investigation_status(
        &self,
        id: InvestigationId,
        status: &InvestigationStatus,
        increment_cycle: bool,
    ) -> Result<(), StoreError> {
        let completed_at = if status.is_terminal() {
            Some(Utc::now())
        } else {
            None
        };

        let cycle_increment = if increment_cycle { 1 } else { 0 };

        sqlx::query(
            r#"
            UPDATE investigations
            SET status = $2,
                cycle_count = cycle_count + $3,
                completed_at = COALESCE($4, completed_at)
            WHERE id = $1
            "#,
        )
        .bind(id.0)
        .bind(status.as_db_str())
        .bind(cycle_increment)
        .bind(completed_at)
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Query(e.to_string()))?;

        Ok(())
    }

    /// Suspend an investigation with reason and resume point.
    pub async fn suspend_investigation(
        &self,
        id: InvestigationId,
        reason: &str,
        resume_from: &str,
    ) -> Result<(), StoreError> {
        sqlx::query(
            r#"
            UPDATE investigations
            SET status = 'suspended',
                suspended_reason = $2,
                suspended_at = $3,
                resume_from = $4
            WHERE id = $1
            "#,
        )
        .bind(id.0)
        .bind(reason)
        .bind(Utc::now())
        .bind(resume_from)
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Query(e.to_string()))?;

        Ok(())
    }

    /// Clear suspension state (when resuming).
    pub async fn clear_suspension(&self, id: InvestigationId) -> Result<(), StoreError> {
        sqlx::query(
            r#"
            UPDATE investigations
            SET suspended_reason = NULL,
                suspended_at = NULL,
                resume_from = NULL
            WHERE id = $1
            "#,
        )
        .bind(id.0)
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Query(e.to_string()))?;

        Ok(())
    }

    /// Get all non-terminal investigations (for startup recovery).
    pub async fn get_non_terminal_investigations(&self) -> Result<Vec<Investigation>, StoreError> {
        let rows = sqlx::query_as::<_, InvestigationRow>(
            r#"
            SELECT id, prompt, status, parent_investigation_id, cycle_count,
                   created_at, completed_at, suspended_reason, suspended_at, resume_from
            FROM investigations
            WHERE status NOT IN ('completed', 'failed')
            ORDER BY created_at
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StoreError::Query(e.to_string()))?;

        Ok(rows.into_iter().map(Into::into).collect())
    }
}

/// Internal row type for sqlx deserialization.
#[derive(sqlx::FromRow)]
struct InvestigationRow {
    id: Uuid,
    prompt: String,
    status: String,
    parent_investigation_id: Option<Uuid>,
    cycle_count: i32,
    created_at: chrono::DateTime<Utc>,
    completed_at: Option<chrono::DateTime<Utc>>,
    suspended_reason: Option<String>,
    suspended_at: Option<chrono::DateTime<Utc>>,
    resume_from: Option<String>,
}

impl From<InvestigationRow> for Investigation {
    fn from(row: InvestigationRow) -> Self {
        Self {
            id: InvestigationId::from_uuid(row.id),
            prompt: row.prompt,
            status: parse_investigation_status(&row.status),
            parent_investigation_id: row.parent_investigation_id.map(InvestigationId::from_uuid),
            cycle_count: row.cycle_count,
            created_at: row.created_at,
            completed_at: row.completed_at,
            suspended_reason: row.suspended_reason,
            suspended_at: row.suspended_at,
            resume_from: row.resume_from,
        }
    }
}

fn parse_investigation_status(s: &str) -> InvestigationStatus {
    match s {
        "pending" => InvestigationStatus::Pending,
        "analyst_running" => InvestigationStatus::AnalystRunning,
        "processing" => InvestigationStatus::Processing,
        "suspended" => InvestigationStatus::Suspended,
        "completed" => InvestigationStatus::Completed,
        "failed" => InvestigationStatus::Failed,
        other => {
            tracing::warn!(
                status = other,
                "Unknown investigation status, defaulting to Pending"
            );
            InvestigationStatus::Pending
        }
    }
}
