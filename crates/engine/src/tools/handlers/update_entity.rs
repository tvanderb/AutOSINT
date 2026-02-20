use std::sync::Arc;

use serde::Deserialize;
use serde_json::{json, Value};

use autosint_common::EntityId;

use crate::graph::conversions::embedding_text_for_entity;
use crate::graph::EntityUpdate;
use crate::tools::registry::{ToolHandler, ToolHandlerContext};

#[derive(Deserialize)]
struct Args {
    entity_id: String,
    #[serde(default)]
    canonical_name: Option<String>,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    aliases: Option<Vec<String>>,
    #[serde(default)]
    is_stub: Option<bool>,
    #[serde(default)]
    properties: Option<std::collections::HashMap<String, Value>>,
}

pub fn handler() -> ToolHandler {
    Arc::new(|args: Value, ctx: Arc<ToolHandlerContext>| {
        Box::pin(async move {
            let args: Args =
                serde_json::from_value(args).map_err(|e| format!("Invalid arguments: {}", e))?;

            let entity_id: EntityId = args
                .entity_id
                .parse::<uuid::Uuid>()
                .map(EntityId::from_uuid)
                .map_err(|e| format!("Invalid entity_id: {}", e))?;

            // Check if name or summary changed — recompute embedding if so.
            let needs_reembed = args.canonical_name.is_some() || args.summary.is_some();
            let embedding = if needs_reembed {
                if let Some(ref emb_client) = ctx.embedding_client {
                    // Get current entity to merge with updates for embedding text.
                    let current = ctx
                        .graph
                        .get_entity(entity_id)
                        .await
                        .map_err(|e| format!("Failed to get entity: {}", e))?;
                    let name = args
                        .canonical_name
                        .as_deref()
                        .unwrap_or(&current.canonical_name);
                    let summary = args.summary.as_deref().or(current.summary.as_deref());
                    let text = embedding_text_for_entity(name, summary);
                    match emb_client.embed_single(&text).await {
                        Ok(emb) => Some(emb),
                        Err(e) => {
                            tracing::warn!(error = %e, "Failed to recompute entity embedding");
                            None
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };

            let update = EntityUpdate {
                canonical_name: args.canonical_name,
                aliases: args.aliases,
                kind: args.kind,
                summary: args.summary,
                is_stub: args.is_stub,
                properties: args.properties,
            };

            let updated = ctx
                .graph
                .update_entity(entity_id, &update, embedding)
                .await
                .map_err(|e| format!("Failed to update entity: {}", e))?;

            Ok(json!({
                "entity_id": updated.id.to_string(),
                "canonical_name": updated.canonical_name,
                "kind": updated.kind,
                "summary": updated.summary,
                "message": "Entity updated successfully."
            }))
        })
    })
}
