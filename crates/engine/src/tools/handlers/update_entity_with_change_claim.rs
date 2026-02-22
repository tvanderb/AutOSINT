use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde::Deserialize;
use serde_json::{json, Value};

use autosint_common::types::{AttributionDepth, Claim, InformationType};
use autosint_common::EntityId;

use crate::graph::conversions::{embedding_text_for_claim, embedding_text_for_entity};
use crate::graph::EntityUpdate;
use crate::tools::registry::{ToolHandler, ToolHandlerContext};

#[derive(Deserialize)]
struct Args {
    entity_id: String,
    /// Claim content describing what changed.
    claim_content: String,
    claim_source_entity_id: String,
    claim_published_timestamp: String,
    #[serde(default = "default_attribution")]
    claim_attribution_depth: String,
    #[serde(default = "default_information_type")]
    claim_information_type: String,
    #[serde(default)]
    claim_raw_source_link: Option<String>,
    /// Entity update fields (at least one required).
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

fn default_attribution() -> String {
    "secondhand".to_string()
}

fn default_information_type() -> String {
    "assertion".to_string()
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

            let source_entity_id: EntityId = args
                .claim_source_entity_id
                .parse::<uuid::Uuid>()
                .map(EntityId::from_uuid)
                .map_err(|e| format!("Invalid claim_source_entity_id: {}", e))?;

            let attribution_depth = match args.claim_attribution_depth.as_str() {
                "primary" => AttributionDepth::Primary,
                "secondhand" | "secondary" => AttributionDepth::Secondhand,
                "indirect" | "tertiary" => AttributionDepth::Indirect,
                other => {
                    return Err(format!(
                        "Invalid claim_attribution_depth: '{}'. Use 'primary', 'secondhand', or 'indirect'.",
                        other
                    ))
                }
            };

            let information_type = match args.claim_information_type.as_str() {
                "assertion" => InformationType::Assertion,
                "analysis" => InformationType::Analysis,
                "discourse" => InformationType::Discourse,
                "testimony" => InformationType::Testimony,
                other => {
                    return Err(format!(
                        "Invalid claim_information_type: '{}'. Use 'assertion', 'analysis', 'discourse', or 'testimony'.",
                        other
                    ))
                }
            };

            let published_timestamp =
                chrono::DateTime::parse_from_rfc3339(&args.claim_published_timestamp)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .map_err(|e| {
                        format!(
                            "Invalid claim_published_timestamp (expected RFC3339): {}",
                            e
                        )
                    })?;

            // 1. Update the entity.
            let needs_reembed = args.canonical_name.is_some() || args.summary.is_some();
            let entity_embedding = if needs_reembed {
                if let Some(ref emb_client) = ctx.embedding_client {
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

            let updated_entity = ctx
                .graph
                .update_entity(entity_id, &update, entity_embedding)
                .await
                .map_err(|e| format!("Failed to update entity: {}", e))?;

            // 2. Create the change claim.
            let claim_embedding = if let Some(ref emb_client) = ctx.embedding_client {
                let text = embedding_text_for_claim(&args.claim_content);
                match emb_client.embed_single(&text).await {
                    Ok(emb) => Some(emb),
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to compute claim embedding");
                        None
                    }
                }
            } else {
                None
            };

            let mut claim = Claim::new(
                args.claim_content,
                published_timestamp,
                attribution_depth,
                information_type,
                source_entity_id,
            );
            claim.referenced_entity_ids = vec![entity_id];
            claim.raw_source_link = args.claim_raw_source_link;

            let created_claim = ctx
                .graph
                .create_claim(&claim, claim_embedding)
                .await
                .map_err(|e| format!("Entity updated but claim creation failed: {}", e))?;

            ctx.session_counters
                .claims_created
                .fetch_add(1, Ordering::Relaxed);

            Ok(json!({
                "entity_id": updated_entity.id.to_string(),
                "canonical_name": updated_entity.canonical_name,
                "claim_id": created_claim.id.to_string(),
                "message": "Entity updated and change claim created successfully."
            }))
        })
    })
}
