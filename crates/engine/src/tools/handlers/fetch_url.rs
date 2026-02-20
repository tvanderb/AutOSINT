use std::sync::Arc;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::tools::registry::{ToolHandler, ToolHandlerContext};
use autosint_common::api::fetch::{FetchRequest, FetchResponse};

const MAX_CONTENT_CHARS: usize = 50_000;

#[derive(Deserialize)]
struct Args {
    url: String,
}

pub fn handler() -> ToolHandler {
    Arc::new(|args: Value, ctx: Arc<ToolHandlerContext>| {
        Box::pin(async move {
            let args: Args =
                serde_json::from_value(args).map_err(|e| format!("Invalid arguments: {}", e))?;

            let fetch_url = format!("{}/fetch", ctx.fetch_base_url);

            let request = FetchRequest {
                url: args.url.clone(),
                options: None,
            };

            let response = ctx
                .http
                .post(&fetch_url)
                .json(&request)
                .send()
                .await
                .map_err(|e| format!("Fetch service request failed: {}", e))?;

            let status = response.status();
            if !status.is_success() {
                let body = response.text().await.unwrap_or_default();
                return Err(format!("Fetch service returned {}: {}", status, body));
            }

            let fetch_response: FetchResponse = response
                .json()
                .await
                .map_err(|e| format!("Failed to parse fetch response: {}", e))?;

            // Truncate content to keep within LLM context limits.
            let content = if fetch_response.content.len() > MAX_CONTENT_CHARS {
                let truncated = &fetch_response.content[..MAX_CONTENT_CHARS];
                format!(
                    "{}...\n[Content truncated: {} chars total, showing first {}]",
                    truncated,
                    fetch_response.content.len(),
                    MAX_CONTENT_CHARS
                )
            } else {
                fetch_response.content
            };

            Ok(json!({
                "url": fetch_response.metadata.url,
                "status_code": fetch_response.metadata.status_code,
                "content_type": fetch_response.metadata.content_type,
                "cached": fetch_response.metadata.cached,
                "content": content,
            }))
        })
    })
}
