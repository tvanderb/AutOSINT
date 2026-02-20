use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde::Deserialize;
use serde_json::{json, Value};

use autosint_common::types::Entity;

use crate::graph::conversions::embedding_text_for_entity;
use crate::graph::dedup::{DedupResult, EntityDedup};
use crate::tools::registry::{ToolHandler, ToolHandlerContext};

#[derive(Deserialize)]
struct Args {
    canonical_name: String,
    kind: String,
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

            // Compute embedding for dedup + storage.
            let embed_text =
                embedding_text_for_entity(&args.canonical_name, args.summary.as_deref());
            let embedding = if let Some(ref emb_client) = ctx.embedding_client {
                match emb_client.embed_single(&embed_text).await {
                    Ok(emb) => Some(emb),
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to compute entity embedding");
                        None
                    }
                }
            } else {
                None
            };

            // Run dedup pipeline (stages 1-3; LLM judge = None for now).
            let dedup = EntityDedup::new(&ctx.graph, &ctx.dedup_config, None);
            let dedup_result = dedup
                .find_duplicate(&args.canonical_name, &args.kind, embedding.as_deref())
                .await
                .map_err(|e| format!("Dedup check failed: {}", e))?;

            match dedup_result {
                DedupResult::ExactMatch(entity_id) => {
                    let existing = ctx
                        .graph
                        .get_entity(entity_id)
                        .await
                        .map_err(|e| format!("Failed to get existing entity: {}", e))?;
                    Ok(json!({
                        "deduplicated": true,
                        "entity_id": existing.id.to_string(),
                        "canonical_name": existing.canonical_name,
                        "kind": existing.kind,
                        "summary": existing.summary,
                        "message": "Entity already exists (exact match). Use update_entity to modify."
                    }))
                }
                DedupResult::ProbableMatch {
                    entity_id,
                    confidence,
                    ..
                } => {
                    let existing = ctx
                        .graph
                        .get_entity(entity_id)
                        .await
                        .map_err(|e| format!("Failed to get existing entity: {}", e))?;
                    Ok(json!({
                        "deduplicated": true,
                        "entity_id": existing.id.to_string(),
                        "canonical_name": existing.canonical_name,
                        "kind": existing.kind,
                        "summary": existing.summary,
                        "confidence": confidence,
                        "message": "Probable duplicate found. Use update_entity to modify if this is the same entity."
                    }))
                }
                DedupResult::NoMatch => {
                    // Create the new entity.
                    let mut entity = Entity::new(args.canonical_name, args.kind);
                    entity.summary = args.summary;
                    if let Some(aliases) = args.aliases {
                        entity.aliases = aliases;
                    }
                    if let Some(is_stub) = args.is_stub {
                        entity.is_stub = is_stub;
                    }
                    if let Some(properties) = args.properties {
                        entity.properties = properties;
                    }

                    let created = ctx
                        .graph
                        .create_entity(&entity, embedding)
                        .await
                        .map_err(|e| format!("Failed to create entity: {}", e))?;

                    ctx.session_counters
                        .entities_created
                        .fetch_add(1, Ordering::Relaxed);

                    Ok(json!({
                        "deduplicated": false,
                        "entity_id": created.id.to_string(),
                        "canonical_name": created.canonical_name,
                        "kind": created.kind,
                        "summary": created.summary,
                        "message": "Entity created successfully."
                    }))
                }
            }
        })
    })
}
