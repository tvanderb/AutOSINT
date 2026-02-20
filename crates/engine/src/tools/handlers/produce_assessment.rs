use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde::Deserialize;
use serde_json::{json, Value};

use autosint_common::ids::{ClaimId, EntityId};
use autosint_common::types::{Assessment, Confidence};

use crate::tools::registry::{ToolHandler, ToolHandlerContext};

#[derive(Deserialize)]
struct Args {
    content: Value,
    confidence: String,
    #[serde(default)]
    entity_refs: Vec<String>,
    #[serde(default)]
    claim_refs: Vec<String>,
}

pub fn handler() -> ToolHandler {
    Arc::new(|args: Value, ctx: Arc<ToolHandlerContext>| {
        Box::pin(async move {
            let args: Args =
                serde_json::from_value(args).map_err(|e| format!("Invalid arguments: {}", e))?;

            // Only one assessment per session.
            if ctx
                .session_counters
                .assessment_produced
                .load(Ordering::Relaxed)
            {
                return Err(
                    "Assessment already produced in this session. Only one assessment per cycle."
                        .to_string(),
                );
            }

            let store = ctx.store.as_ref().ok_or_else(|| {
                "Assessment production not available (store not configured)".to_string()
            })?;
            let investigation_id = ctx.investigation_id.ok_or_else(|| {
                "Assessment production not available (no investigation context)".to_string()
            })?;

            // Parse confidence.
            let confidence = match args.confidence.as_str() {
                "high" => Confidence::High,
                "moderate" => Confidence::Moderate,
                "low" => Confidence::Low,
                other => {
                    return Err(format!(
                        "Invalid confidence: '{}'. Use 'high', 'moderate', or 'low'.",
                        other
                    ))
                }
            };

            // Parse entity refs.
            let entity_refs: Vec<EntityId> = args
                .entity_refs
                .iter()
                .map(|s| {
                    s.parse::<uuid::Uuid>()
                        .map(EntityId::from_uuid)
                        .map_err(|e| format!("Invalid entity_ref '{}': {}", s, e))
                })
                .collect::<Result<_, _>>()?;

            // Parse claim refs.
            let claim_refs: Vec<ClaimId> = args
                .claim_refs
                .iter()
                .map(|s| {
                    s.parse::<uuid::Uuid>()
                        .map(ClaimId::from_uuid)
                        .map_err(|e| format!("Invalid claim_ref '{}': {}", s, e))
                })
                .collect::<Result<_, _>>()?;

            // Compute embedding for the assessment content.
            let embed_text = serde_json::to_string(&args.content).unwrap_or_default();
            let embedding = if let Some(ref emb_client) = ctx.embedding_client {
                match emb_client.embed_single(&embed_text).await {
                    Ok(emb) => Some(emb),
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to compute assessment embedding");
                        None
                    }
                }
            } else {
                None
            };

            let mut assessment = Assessment::new(investigation_id, args.content, confidence);
            assessment.entity_refs = entity_refs;
            assessment.claim_refs = claim_refs;
            assessment.embedding = embedding;

            let created = store
                .create_assessment(&assessment)
                .await
                .map_err(|e| format!("Failed to store assessment: {}", e))?;

            // Mark assessment as produced (only one per session).
            ctx.session_counters
                .assessment_produced
                .store(true, Ordering::Relaxed);

            tracing::info!(
                assessment_id = %created.id,
                investigation_id = %investigation_id,
                confidence = created.confidence.as_db_str(),
                "Assessment produced"
            );

            Ok(json!({
                "assessment_id": created.id.to_string(),
                "investigation_id": investigation_id.to_string(),
                "confidence": created.confidence.as_db_str(),
                "message": "Assessment stored successfully. Investigation will complete."
            }))
        })
    })
}
