use chrono::Utc;
use pgvector::Vector;
use uuid::Uuid;

use autosint_common::ids::{AssessmentId, InvestigationId};
use autosint_common::types::{Assessment, Confidence};

use super::{StoreClient, StoreError};

impl StoreClient {
    /// Create a new assessment record with optional embedding for semantic search.
    pub async fn create_assessment(
        &self,
        assessment: &Assessment,
    ) -> Result<Assessment, StoreError> {
        let entity_refs_json = serde_json::to_value(&assessment.entity_refs).unwrap_or_default();
        let claim_refs_json = serde_json::to_value(&assessment.claim_refs).unwrap_or_default();
        let embedding = assessment
            .embedding
            .as_ref()
            .map(|v| Vector::from(v.clone()));

        sqlx::query(
            r#"
            INSERT INTO assessments (id, investigation_id, content, confidence,
                                     entity_refs, claim_refs, embedding, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(assessment.id.0)
        .bind(assessment.investigation_id.0)
        .bind(&assessment.content)
        .bind(assessment.confidence.as_db_str())
        .bind(&entity_refs_json)
        .bind(&claim_refs_json)
        .bind(embedding)
        .bind(assessment.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Query(e.to_string()))?;

        Ok(assessment.clone())
    }

    /// Retrieve an assessment by ID.
    pub async fn get_assessment(&self, id: AssessmentId) -> Result<Assessment, StoreError> {
        let row = sqlx::query_as::<_, AssessmentRow>(
            r#"
            SELECT id, investigation_id, content, confidence,
                   entity_refs, claim_refs, created_at
            FROM assessments
            WHERE id = $1
            "#,
        )
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StoreError::Query(e.to_string()))?
        .ok_or_else(|| StoreError::NotFound(format!("Assessment {}", id)))?;

        Ok(row.into())
    }

    /// Semantic search over assessments using pgvector cosine similarity.
    /// Returns assessments with similarity scores, ordered by relevance.
    pub async fn search_assessments(
        &self,
        query_embedding: Vec<f32>,
        limit: i64,
    ) -> Result<Vec<(Assessment, f64)>, StoreError> {
        let query_vec = Vector::from(query_embedding);

        let rows = sqlx::query_as::<_, AssessmentWithScoreRow>(
            r#"
            SELECT id, investigation_id, content, confidence,
                   entity_refs, claim_refs, created_at,
                   1 - (embedding <=> $1::vector) AS score
            FROM assessments
            WHERE embedding IS NOT NULL
            ORDER BY embedding <=> $1::vector
            LIMIT $2
            "#,
        )
        .bind(query_vec)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StoreError::Query(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let score = row.score;
                let assessment: Assessment = AssessmentRow {
                    id: row.id,
                    investigation_id: row.investigation_id,
                    content: row.content,
                    confidence: row.confidence,
                    entity_refs: row.entity_refs,
                    claim_refs: row.claim_refs,
                    created_at: row.created_at,
                }
                .into();
                (assessment, score)
            })
            .collect())
    }
}

/// Internal row type for sqlx deserialization.
#[derive(sqlx::FromRow)]
struct AssessmentRow {
    id: Uuid,
    investigation_id: Uuid,
    content: serde_json::Value,
    confidence: String,
    entity_refs: serde_json::Value,
    claim_refs: serde_json::Value,
    created_at: chrono::DateTime<Utc>,
}

/// Row type with similarity score for search results.
#[derive(sqlx::FromRow)]
struct AssessmentWithScoreRow {
    id: Uuid,
    investigation_id: Uuid,
    content: serde_json::Value,
    confidence: String,
    entity_refs: serde_json::Value,
    claim_refs: serde_json::Value,
    created_at: chrono::DateTime<Utc>,
    score: f64,
}

impl From<AssessmentRow> for Assessment {
    fn from(row: AssessmentRow) -> Self {
        let entity_refs = serde_json::from_value(row.entity_refs).unwrap_or_default();
        let claim_refs = serde_json::from_value(row.claim_refs).unwrap_or_default();

        Self {
            id: AssessmentId::from_uuid(row.id),
            investigation_id: InvestigationId::from_uuid(row.investigation_id),
            content: row.content,
            confidence: parse_confidence(&row.confidence),
            entity_refs,
            claim_refs,
            embedding: None, // Not retrieved in queries (large)
            created_at: row.created_at,
        }
    }
}

fn parse_confidence(s: &str) -> Confidence {
    match s {
        "high" => Confidence::High,
        "moderate" => Confidence::Moderate,
        "low" => Confidence::Low,
        other => {
            tracing::warn!(
                confidence = other,
                "Unknown confidence level, defaulting to Low"
            );
            Confidence::Low
        }
    }
}
