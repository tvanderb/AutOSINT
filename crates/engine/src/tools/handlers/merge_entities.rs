use std::sync::Arc;

use serde::Deserialize;
use serde_json::{json, Value};

use autosint_common::EntityId;

use crate::tools::registry::{ToolHandler, ToolHandlerContext};

#[derive(Deserialize)]
struct Args {
    source_entity_id: String,
    target_entity_id: String,
    #[serde(default)]
    reason: Option<String>,
}

pub fn handler() -> ToolHandler {
    Arc::new(|args: Value, ctx: Arc<ToolHandlerContext>| {
        Box::pin(async move {
            let args: Args =
                serde_json::from_value(args).map_err(|e| format!("Invalid arguments: {}", e))?;

            let source_id = args
                .source_entity_id
                .parse::<uuid::Uuid>()
                .map(EntityId::from_uuid)
                .map_err(|e| format!("Invalid source_entity_id: {}", e))?;

            let target_id = args
                .target_entity_id
                .parse::<uuid::Uuid>()
                .map(EntityId::from_uuid)
                .map_err(|e| format!("Invalid target_entity_id: {}", e))?;

            let merged = ctx
                .graph
                .merge_entities(source_id, target_id, args.reason.as_deref())
                .await
                .map_err(|e| format!("Failed to merge entities: {}", e))?;

            Ok(json!({
                "merged_entity_id": merged.id.to_string(),
                "canonical_name": merged.canonical_name,
                "aliases": merged.aliases,
                "kind": merged.kind,
                "message": format!(
                    "Entity {} merged into {}. All relationships and claims reassigned.",
                    source_id, target_id
                )
            }))
        })
    })
}
