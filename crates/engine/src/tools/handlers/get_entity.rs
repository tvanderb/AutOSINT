use std::sync::Arc;

use serde::Deserialize;
use serde_json::{json, Value};

use autosint_common::EntityId;

use crate::tools::registry::{ToolHandler, ToolHandlerContext};
use crate::tools::truncation::truncate_entity_detail;

#[derive(Deserialize)]
struct Args {
    entity_id: String,
}

pub fn handler() -> ToolHandler {
    Arc::new(|args: Value, ctx: Arc<ToolHandlerContext>| {
        Box::pin(async move {
            let args: Args =
                serde_json::from_value(args).map_err(|e| format!("Invalid arguments: {}", e))?;

            let entity_id = args
                .entity_id
                .parse::<uuid::Uuid>()
                .map(EntityId::from_uuid)
                .map_err(|e| format!("Invalid entity_id: {}", e))?;

            let entity = ctx
                .graph
                .get_entity(entity_id)
                .await
                .map_err(|e| format!("Failed to get entity: {}", e))?;

            let properties: Value = serde_json::to_value(&entity.properties).unwrap_or_default();

            let mut result = json!({
                "id": entity.id.to_string(),
                "canonical_name": entity.canonical_name,
                "kind": entity.kind,
                "summary": entity.summary,
                "aliases": entity.aliases,
                "is_stub": entity.is_stub,
                "last_updated": entity.last_updated.to_rfc3339(),
                "properties": properties,
            });

            truncate_entity_detail(&mut result, &ctx.tool_result_limits);
            Ok(result)
        })
    })
}
