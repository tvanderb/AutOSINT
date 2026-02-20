use std::sync::Arc;

use serde::Deserialize;
use serde_json::{json, Value};

use autosint_common::RelationshipId;

use crate::graph::conversions::embedding_text_for_relationship;
use crate::graph::RelationshipUpdate;
use crate::tools::registry::{ToolHandler, ToolHandlerContext};

#[derive(Deserialize)]
struct Args {
    relationship_id: String,
    #[serde(default)]
    description: Option<String>,
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

            let rel_id: RelationshipId = args
                .relationship_id
                .parse::<uuid::Uuid>()
                .map(RelationshipId::from_uuid)
                .map_err(|e| format!("Invalid relationship_id: {}", e))?;

            let timestamp = args
                .timestamp
                .as_deref()
                .map(|ts| {
                    chrono::DateTime::parse_from_rfc3339(ts)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .map_err(|e| format!("Invalid timestamp: {}", e))
                })
                .transpose()?;

            // Recompute embedding if description changed.
            let embedding = if let Some(ref desc) = args.description {
                if let Some(ref emb_client) = ctx.embedding_client {
                    let text = embedding_text_for_relationship(desc);
                    match emb_client.embed_single(&text).await {
                        Ok(emb) => Some(emb),
                        Err(e) => {
                            tracing::warn!(error = %e, "Failed to recompute relationship embedding");
                            None
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };

            let update = RelationshipUpdate {
                description: args.description,
                weight: args.weight,
                confidence: args.confidence,
                bidirectional: args.bidirectional,
                timestamp,
            };

            let updated = ctx
                .graph
                .update_relationship(rel_id, &update, embedding)
                .await
                .map_err(|e| format!("Failed to update relationship: {}", e))?;

            Ok(json!({
                "relationship_id": updated.id.to_string(),
                "description": updated.description,
                "message": "Relationship updated successfully."
            }))
        })
    })
}
