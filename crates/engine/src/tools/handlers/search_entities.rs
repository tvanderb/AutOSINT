use std::sync::Arc;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::graph::{EntitySearchParams, SearchMode};
use crate::tools::registry::{ToolHandler, ToolHandlerContext};
use crate::tools::truncation::truncate_search_results;

#[derive(Deserialize)]
struct Args {
    query: String,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    limit: Option<u32>,
}

pub fn handler() -> ToolHandler {
    Arc::new(|args: Value, ctx: Arc<ToolHandlerContext>| {
        Box::pin(async move {
            let args: Args =
                serde_json::from_value(args).map_err(|e| format!("Invalid arguments: {}", e))?;

            let mode = match args.mode.as_deref() {
                Some("semantic") | None => SearchMode::Semantic,
                Some("keyword") => SearchMode::Keyword,
                Some(other) => {
                    return Err(format!(
                        "Unknown search mode: '{}'. Use 'semantic' or 'keyword'.",
                        other
                    ))
                }
            };

            // Compute embedding for semantic search.
            let query_embedding = if matches!(mode, SearchMode::Semantic) {
                if let Some(ref emb_client) = ctx.embedding_client {
                    match emb_client.embed_single(&args.query).await {
                        Ok(emb) => Some(emb),
                        Err(e) => {
                            tracing::warn!(error = %e, "Failed to embed search query, falling back to keyword");
                            // Fall back: we'll do keyword search instead
                            return do_keyword_search(&args, &ctx).await;
                        }
                    }
                } else {
                    return do_keyword_search(&args, &ctx).await;
                }
            } else {
                None
            };

            let params = EntitySearchParams {
                query: args.query,
                mode,
                kind_filter: args.kind,
                updated_after: None,
                updated_before: None,
                limit: args.limit,
            };

            let results = ctx
                .graph
                .search_entities(&params, query_embedding)
                .await
                .map_err(|e| format!("Search failed: {}", e))?;

            let items: Vec<Value> = results
                .iter()
                .map(|r| {
                    json!({
                        "id": r.item.id.to_string(),
                        "canonical_name": r.item.canonical_name,
                        "kind": r.item.kind,
                        "summary": r.item.summary,
                        "aliases": r.item.aliases,
                        "is_stub": r.item.is_stub,
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

async fn do_keyword_search(args: &Args, ctx: &ToolHandlerContext) -> Result<Value, String> {
    let params = EntitySearchParams {
        query: args.query.clone(),
        mode: SearchMode::Keyword,
        kind_filter: args.kind.clone(),
        updated_after: None,
        updated_before: None,
        limit: args.limit,
    };

    let results = ctx
        .graph
        .search_entities(&params, None)
        .await
        .map_err(|e| format!("Search failed: {}", e))?;

    let items: Vec<Value> = results
        .iter()
        .map(|r| {
            json!({
                "id": r.item.id.to_string(),
                "canonical_name": r.item.canonical_name,
                "kind": r.item.kind,
                "summary": r.item.summary,
                "aliases": r.item.aliases,
                "is_stub": r.item.is_stub,
                "score": r.score,
            })
        })
        .collect();

    let mut result = json!({ "results": items });
    truncate_search_results(&mut result, &ctx.tool_result_limits);
    Ok(result)
}
