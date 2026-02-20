use std::sync::Arc;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::tools::registry::{ToolHandler, ToolHandlerContext};
use autosint_common::api::fetch::SourceQueryResponse;

#[derive(Deserialize)]
struct Args {
    source_id: String,
    #[serde(default)]
    query: Option<String>,
    /// Additional source-specific parameters passed through.
    #[serde(flatten)]
    params: serde_json::Map<String, Value>,
}

pub fn handler() -> ToolHandler {
    Arc::new(|args: Value, ctx: Arc<ToolHandlerContext>| {
        Box::pin(async move {
            let args: Args =
                serde_json::from_value(args).map_err(|e| format!("Invalid arguments: {}", e))?;

            let url = format!("{}/sources/{}/query", ctx.fetch_base_url, args.source_id);

            let mut body = serde_json::Map::new();
            if let Some(query) = args.query {
                body.insert("query".into(), Value::String(query));
            }
            for (k, v) in args.params {
                if k != "source_id" && k != "query" {
                    body.insert(k, v);
                }
            }

            let response = ctx
                .http
                .post(&url)
                .json(&body)
                .send()
                .await
                .map_err(|e| format!("Fetch service request failed: {}", e))?;

            let status = response.status();
            if !status.is_success() {
                let body = response.text().await.unwrap_or_default();
                return Err(format!("Fetch service returned {}: {}", status, body));
            }

            let query_response: SourceQueryResponse = response
                .json()
                .await
                .map_err(|e| format!("Failed to parse source query response: {}", e))?;

            Ok(json!({
                "source_id": query_response.metadata.source_id,
                "total_results": query_response.metadata.total_results,
                "returned_results": query_response.metadata.returned_results,
                "results": query_response.results,
            }))
        })
    })
}
