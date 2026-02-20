use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde::Deserialize;
use serde_json::{json, Value};

use autosint_common::types::Relationship;
use autosint_common::EntityId;

use crate::graph::conversions::embedding_text_for_relationship;
use crate::tools::registry::{ToolHandler, ToolHandlerContext};

#[derive(Deserialize)]
struct Args {
    source_entity_id: String,
    target_entity_id: String,
    description: String,
    #[serde(default)]
    weight: Option<f64>,
    #[serde(default)]
    confidence: Option<f64>,
    #[serde(default)]
    bidirectional: Option<bool>,
    #[serde(default)]
    timestamp: Option<String>,
}

pub fn handler() -> ToolHandler {
    Arc::new(|args: Value, ctx: Arc<ToolHandlerContext>| {
        Box::pin(async move {
            let args: Args =
                serde_json::from_value(args).map_err(|e| format!("Invalid arguments: {}", e))?;

            let source_id: EntityId = args
                .source_entity_id
                .parse::<uuid::Uuid>()
                .map(EntityId::from_uuid)
                .map_err(|e| format!("Invalid source_entity_id: {}", e))?;

            let target_id: EntityId = args
                .target_entity_id
                .parse::<uuid::Uuid>()
                .map(EntityId::from_uuid)
                .map_err(|e| format!("Invalid target_entity_id: {}", e))?;

            let timestamp = args
                .timestamp
                .as_deref()
                .map(|ts| {
                    chrono::DateTime::parse_from_rfc3339(ts)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .map_err(|e| format!("Invalid timestamp: {}", e))
                })
                .transpose()?;

            // Compute embedding.
            let embed_text = embedding_text_for_relationship(&args.description);
            let embedding = if let Some(ref emb_client) = ctx.embedding_client {
                match emb_client.embed_single(&embed_text).await {
                    Ok(emb) => Some(emb),
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to compute relationship embedding");
                        None
                    }
                }
            } else {
                None
            };

            let mut relationship = Relationship::new(source_id, target_id, args.description);
            relationship.weight = args.weight;
            relationship.confidence = args.confidence;
            relationship.bidirectional = args.bidirectional.unwrap_or(false);
            relationship.timestamp = timestamp;

            let created = ctx
                .graph
                .create_relationship(&relationship, embedding)
                .await
                .map_err(|e| format!("Failed to create relationship: {}", e))?;

            ctx.session_counters
                .relationships_created
                .fetch_add(1, Ordering::Relaxed);

            Ok(json!({
                "relationship_id": created.id.to_string(),
                "source_entity_id": created.source_entity_id.to_string(),
                "target_entity_id": created.target_entity_id.to_string(),
                "description": created.description,
                "message": "Relationship created successfully."
            }))
        })
    })
}
