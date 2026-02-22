use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde::Deserialize;
use serde_json::{json, Value};

use autosint_common::types::{AttributionDepth, Claim, InformationType};
use autosint_common::EntityId;

use crate::graph::conversions::embedding_text_for_claim;
use crate::tools::registry::{ToolHandler, ToolHandlerContext};

#[derive(Deserialize)]
struct Args {
    content: String,
    source_entity_id: String,
    published_timestamp: String,
    #[serde(default = "default_attribution")]
    attribution_depth: String,
    #[serde(default = "default_information_type")]
    information_type: String,
    #[serde(default)]
    referenced_entity_ids: Vec<String>,
    #[serde(default)]
    raw_source_link: Option<String>,
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

            let source_entity_id: EntityId = args
                .source_entity_id
                .parse::<uuid::Uuid>()
                .map(EntityId::from_uuid)
                .map_err(|e| format!("Invalid source_entity_id: {}", e))?;

            let attribution_depth = match args.attribution_depth.as_str() {
                "primary" => AttributionDepth::Primary,
                "secondhand" | "secondary" => AttributionDepth::Secondhand,
                "indirect" | "tertiary" => AttributionDepth::Indirect,
                other => return Err(format!("Invalid attribution_depth: '{}'", other)),
            };

            let information_type = match args.information_type.as_str() {
                "assertion" => InformationType::Assertion,
                "analysis" => InformationType::Analysis,
                "discourse" => InformationType::Discourse,
                "testimony" => InformationType::Testimony,
                other => {
                    return Err(format!(
                        "Invalid information_type: '{}'. Use 'assertion', 'analysis', 'discourse', or 'testimony'.",
                        other
                    ))
                }
            };

            let published_timestamp =
                chrono::DateTime::parse_from_rfc3339(&args.published_timestamp)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .map_err(|e| {
                        format!("Invalid published_timestamp (expected RFC3339): {}", e)
                    })?;

            let referenced_entity_ids: Vec<EntityId> = args
                .referenced_entity_ids
                .iter()
                .map(|s| {
                    s.parse::<uuid::Uuid>()
                        .map(EntityId::from_uuid)
                        .map_err(|e| format!("Invalid referenced_entity_id '{}': {}", s, e))
                })
                .collect::<Result<_, _>>()?;

            // Compute embedding.
            let embed_text = embedding_text_for_claim(&args.content);
            let embedding = if let Some(ref emb_client) = ctx.embedding_client {
                match emb_client.embed_single(&embed_text).await {
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
                args.content,
                published_timestamp,
                attribution_depth,
                information_type,
                source_entity_id,
            );
            claim.referenced_entity_ids = referenced_entity_ids;
            claim.raw_source_link = args.raw_source_link;

            let created = ctx
                .graph
                .create_claim(&claim, embedding)
                .await
                .map_err(|e| format!("Failed to create claim: {}", e))?;

            ctx.session_counters
                .claims_created
                .fetch_add(1, Ordering::Relaxed);

            Ok(json!({
                "claim_id": created.id.to_string(),
                "content": created.content,
                "source_entity_id": created.source_entity_id.to_string(),
                "referenced_entity_ids": created.referenced_entity_ids.iter().map(|id| id.to_string()).collect::<Vec<_>>(),
                "message": "Claim created successfully."
            }))
        })
    })
}
