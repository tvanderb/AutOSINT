use std::sync::Arc;

use serde_json::{json, Value};

use crate::tools::registry::{ToolHandler, ToolHandlerContext};

pub fn handler() -> ToolHandler {
    Arc::new(|_args: Value, _ctx: Arc<ToolHandlerContext>| {
        Box::pin(async move {
            Ok(json!({
                "available": false,
                "message": "AutOSINT Geo is not yet available. Geographic intelligence queries will be supported in a future release. For now, rely on knowledge graph entities and claims for geographic context."
            }))
        })
    })
}
