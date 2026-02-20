use std::sync::Arc;

use serde::Deserialize;
use serde_json::{json, Value};

use autosint_common::ids::AssessmentId;

use crate::tools::registry::{ToolHandler, ToolHandlerContext};

#[derive(Deserialize)]
struct Args {
    assessment_id: String,
}

pub fn handler() -> ToolHandler {
    Arc::new(|args: Value, ctx: Arc<ToolHandlerContext>| {
        Box::pin(async move {
            let args: Args =
                serde_json::from_value(args).map_err(|e| format!("Invalid arguments: {}", e))?;

            let assessment_id = args
                .assessment_id
                .parse::<uuid::Uuid>()
                .map(AssessmentId::from_uuid)
                .map_err(|e| format!("Invalid assessment_id: {}", e))?;

            let store = ctx.store.as_ref().ok_or_else(|| {
                "Assessment retrieval not available (store not configured)".to_string()
            })?;

            let assessment = store
                .get_assessment(assessment_id)
                .await
                .map_err(|e| format!("Failed to get assessment: {}", e))?;

            Ok(json!({
                "id": assessment.id.to_string(),
                "investigation_id": assessment.investigation_id.to_string(),
                "content": assessment.content,
                "confidence": assessment.confidence.as_db_str(),
                "entity_refs": assessment.entity_refs.iter().map(|id| id.to_string()).collect::<Vec<_>>(),
                "claim_refs": assessment.claim_refs.iter().map(|id| id.to_string()).collect::<Vec<_>>(),
                "created_at": assessment.created_at.to_rfc3339(),
            }))
        })
    })
}
