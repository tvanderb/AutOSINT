use std::sync::Arc;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::tools::registry::{ToolHandler, ToolHandlerContext};
use crate::tools::truncation::truncate_search_results;

#[derive(Deserialize)]
struct Args {
    query: String,
    #[serde(default)]
    limit: Option<i64>,
}

pub fn handler() -> ToolHandler {
    Arc::new(|args: Value, ctx: Arc<ToolHandlerContext>| {
        Box::pin(async move {
            let args: Args =
                serde_json::from_value(args).map_err(|e| format!("Invalid arguments: {}", e))?;

            let store = ctx.store.as_ref().ok_or_else(|| {
                "Assessment search not available (store not configured)".to_string()
            })?;

            let emb_client = ctx.embedding_client.as_ref().ok_or_else(|| {
                "Assessment search not available (embedding client not configured)".to_string()
            })?;

            let query_embedding = emb_client
                .embed_single(&args.query)
                .await
                .map_err(|e| format!("Failed to embed query: {}", e))?;

            let limit = args.limit.unwrap_or(5);

            let results = store
                .search_assessments(query_embedding, limit)
                .await
                .map_err(|e| format!("Assessment search failed: {}", e))?;

            let items: Vec<Value> = results
                .iter()
                .map(|(assessment, score)| {
                    // Summarize content for search results (full content via get_assessment).
                    let summary = assessment
                        .content
                        .get("summary")
                        .and_then(|v| v.as_str())
                        .unwrap_or("[no summary]");

                    json!({
                        "id": assessment.id.to_string(),
                        "investigation_id": assessment.investigation_id.to_string(),
                        "confidence": assessment.confidence.as_db_str(),
                        "summary": summary,
                        "created_at": assessment.created_at.to_rfc3339(),
                        "score": score,
                    })
                })
                .collect();

            let mut result = json!({ "results": items });
            truncate_search_results(&mut result, &ctx.tool_result_limits);
            Ok(result)
        })
    })
}
