use std::sync::Arc;

use serde::Deserialize;
use serde_json::{json, Value};

use autosint_common::EntityId;

use crate::graph::{TraversalDirection, TraversalParams};
use crate::tools::registry::{ToolHandler, ToolHandlerContext};
use crate::tools::truncation::truncate_search_results;

#[derive(Deserialize)]
struct Args {
    entity_id: String,
    #[serde(default)]
    direction: Option<String>,
    #[serde(default)]
    description_query: Option<String>,
    #[serde(default)]
    min_weight: Option<f64>,
    #[serde(default)]
    limit: Option<u32>,
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

            let direction = match args.direction.as_deref() {
                Some("outgoing") => Some(TraversalDirection::Outgoing),
                Some("incoming") => Some(TraversalDirection::Incoming),
                Some("both") | None => Some(TraversalDirection::Both),
                Some(other) => {
                    return Err(format!(
                        "Invalid direction: '{}'. Use 'outgoing', 'incoming', or 'both'.",
                        other
                    ))
                }
            };

            // If description_query provided, compute embedding for semantic filtering.
            // Note: traverse_relationships on the graph client doesn't directly support
            // semantic filtering — we filter client-side if needed.
            let _ = args.description_query; // Reserved for future semantic filtering.

            let params = TraversalParams {
                direction,
                min_weight: args.min_weight,
                limit: args.limit,
            };

            let results = ctx
                .graph
                .traverse_relationships(entity_id, &params)
                .await
                .map_err(|e| format!("Traversal failed: {}", e))?;

            let items: Vec<Value> = results
                .iter()
                .map(|(rel, target)| {
                    json!({
                        "relationship": {
                            "id": rel.id.to_string(),
                            "description": rel.description,
                            "weight": rel.weight,
                            "confidence": rel.confidence,
                            "bidirectional": rel.bidirectional,
                            "source_entity_id": rel.source_entity_id.to_string(),
                            "target_entity_id": rel.target_entity_id.to_string(),
                        },
                        "connected_entity": {
                            "id": target.id.to_string(),
                            "canonical_name": target.canonical_name,
                            "kind": target.kind,
                            "summary": target.summary,
                        }
                    })
                })
                .collect();

            let mut result = json!({ "results": items });
            truncate_search_results(&mut result, &ctx.tool_result_limits);
            Ok(result)
        })
    })
}
