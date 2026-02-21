use std::sync::Arc;

use serde::Deserialize;
use serde_json::{json, Value};

use autosint_common::api::fetch::{SearchRequest, SearchResponse};

use crate::tools::registry::{ToolHandler, ToolHandlerContext};

const MAX_RESULTS: usize = 10;

#[derive(Deserialize)]
struct Args {
    query: String,
    #[serde(default)]
    num_results: Option<usize>,
}

pub fn handler() -> ToolHandler {
    Arc::new(|args: Value, ctx: Arc<ToolHandlerContext>| {
        Box::pin(async move {
            let args: Args =
                serde_json::from_value(args).map_err(|e| format!("Invalid arguments: {}", e))?;

            let num_results = args.num_results.unwrap_or(MAX_RESULTS).min(MAX_RESULTS);

            let search_url = format!("{}/search", ctx.fetch_base_url.trim_end_matches('/'));

            let request = SearchRequest {
                query: args.query.clone(),
                num_results: Some(num_results),
            };

            let response = ctx
                .http
                .post(&search_url)
                .json(&request)
                .send()
                .await
                .map_err(|e| format!("Search request failed: {}", e))?;

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                return Err(format!("Search returned {}: {}", status, body));
            }

            let search_response: SearchResponse = response
                .json()
                .await
                .map_err(|e| format!("Failed to parse search response: {}", e))?;

            let results: Vec<Value> = search_response
                .results
                .into_iter()
                .map(|r| {
                    json!({
                        "url": r.url,
                        "title": r.title,
                        "snippet": r.snippet,
                    })
                })
                .collect();

            let count = results.len();
            Ok(json!({
                "query": args.query,
                "results": results,
                "count": count,
            }))
        })
    })
}
