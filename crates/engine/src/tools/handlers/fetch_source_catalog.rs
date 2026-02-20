use std::sync::Arc;

use serde_json::{json, Value};

use crate::tools::registry::{ToolHandler, ToolHandlerContext};
use autosint_common::api::fetch::SourceInfo;

pub fn handler() -> ToolHandler {
    Arc::new(|_args: Value, ctx: Arc<ToolHandlerContext>| {
        Box::pin(async move {
            let url = format!("{}/sources", ctx.fetch_base_url);

            let response = ctx
                .http
                .get(&url)
                .send()
                .await
                .map_err(|e| format!("Fetch service request failed: {}", e))?;

            let status = response.status();
            if !status.is_success() {
                let body = response.text().await.unwrap_or_default();
                return Err(format!("Fetch service returned {}: {}", status, body));
            }

            let sources: Vec<SourceInfo> = response
                .json()
                .await
                .map_err(|e| format!("Failed to parse sources response: {}", e))?;

            Ok(json!({
                "sources": sources,
                "count": sources.len(),
            }))
        })
    })
}
