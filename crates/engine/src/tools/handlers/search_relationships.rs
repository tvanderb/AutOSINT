use std::sync::Arc;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::graph::RelationshipSearchParams;
use crate::tools::registry::{ToolHandler, ToolHandlerContext};
use crate::tools::truncation::truncate_search_results;

#[derive(Deserialize)]
struct Args {
    query: String,
    #[serde(default)]
    limit: Option<u32>,
}

pub fn handler() -> ToolHandler {
    Arc::new(|args: Value, ctx: Arc<ToolHandlerContext>| {
        Box::pin(async move {
            let args: Args =
                serde_json::from_value(args).map_err(|e| format!("Invalid arguments: {}", e))?;

            // Compute embedding for semantic search.
            let query_embedding = if let Some(ref emb_client) = ctx.embedding_client {
                match emb_client.embed_single(&args.query).await {
                    Ok(emb) => Some(emb),
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to embed relationship query");
                        None
                    }
                }
            } else {
                None
            };

            let params = RelationshipSearchParams {
                query: args.query,
                limit: args.limit,
            };

            let results = ctx
                .graph
                .search_relationships(&params, query_embedding)
                .await
                .map_err(|e| format!("Relationship search failed: {}", e))?;

            let items: Vec<Value> = results
                .iter()
                .map(|r| {
                    json!({
                        "id": r.item.id.to_string(),
                        "description": r.item.description,
                        "weight": r.item.weight,
                        "confidence": r.item.confidence,
                        "bidirectional": r.item.bidirectional,
                        "source_entity_id": r.item.source_entity_id.to_string(),
                        "target_entity_id": r.item.target_entity_id.to_string(),
                        "score": r.score,
                    })
                })
                .collect();

            let mut result = json!({ "results": items });
            truncate_search_results(&mut result, &ctx.tool_result_limits);
            Ok(result)
        })
    })
}
