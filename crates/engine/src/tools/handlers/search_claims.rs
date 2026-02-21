use std::sync::Arc;

use serde::Deserialize;
use serde_json::{json, Value};

use autosint_common::types::AttributionDepth;
use autosint_common::EntityId;

use crate::graph::{ClaimSearchParams, SearchMode};
use crate::tools::registry::{ToolHandler, ToolHandlerContext};
use crate::tools::truncation::{truncate_claim_previews, truncate_search_results};

#[derive(Deserialize)]
struct Args {
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    entity_id: Option<String>,
    #[serde(default)]
    source_entity_id: Option<String>,
    #[serde(default)]
    published_after: Option<String>,
    #[serde(default)]
    published_before: Option<String>,
    #[serde(default)]
    attribution_depth: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    sort_by: Option<String>,
    #[serde(default)]
    limit: Option<u32>,
}

pub fn handler() -> ToolHandler {
    Arc::new(|args: Value, ctx: Arc<ToolHandlerContext>| {
        Box::pin(async move {
            let args: Args =
                serde_json::from_value(args).map_err(|e| format!("Invalid arguments: {}", e))?;

            // Parse optional entity IDs.
            let referenced_entity_id = args
                .entity_id
                .as_deref()
                .map(|s| {
                    s.parse::<uuid::Uuid>()
                        .map(EntityId::from_uuid)
                        .map_err(|e| format!("Invalid entity_id: {}", e))
                })
                .transpose()?;

            let source_entity_id = args
                .source_entity_id
                .as_deref()
                .map(|s| {
                    s.parse::<uuid::Uuid>()
                        .map(EntityId::from_uuid)
                        .map_err(|e| format!("Invalid source_entity_id: {}", e))
                })
                .transpose()?;

            // Parse temporal filters.
            let published_after = args
                .published_after
                .as_deref()
                .map(|s| {
                    chrono::DateTime::parse_from_rfc3339(s)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .map_err(|e| format!("Invalid published_after: {}", e))
                })
                .transpose()?;

            let published_before = args
                .published_before
                .as_deref()
                .map(|s| {
                    chrono::DateTime::parse_from_rfc3339(s)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .map_err(|e| format!("Invalid published_before: {}", e))
                })
                .transpose()?;

            let attribution_depth = args
                .attribution_depth
                .as_deref()
                .map(|s| match s {
                    "primary" => Ok(AttributionDepth::Primary),
                    "secondhand" | "secondary" | "tertiary" => Ok(AttributionDepth::Secondhand),
                    other => Err(format!("Invalid attribution_depth: '{}'", other)),
                })
                .transpose()?;

            // Determine search mode and compute embedding if doing semantic search.
            let (mode, query_embedding) = if let Some(ref query) = args.query {
                if let Some(ref emb_client) = ctx.embedding_client {
                    match emb_client.embed_single(query).await {
                        Ok(emb) => (Some(SearchMode::Semantic), Some(emb)),
                        Err(e) => {
                            tracing::warn!(error = %e, "Failed to embed claim query, using keyword");
                            (Some(SearchMode::Keyword), None)
                        }
                    }
                } else {
                    (Some(SearchMode::Keyword), None)
                }
            } else {
                (None, None)
            };

            let params = ClaimSearchParams {
                query: args.query,
                mode,
                published_after,
                published_before,
                source_entity_id,
                referenced_entity_id,
                attribution_depth,
                limit: args.limit,
            };

            let results = ctx
                .graph
                .search_claims(&params, query_embedding)
                .await
                .map_err(|e| format!("Claim search failed: {}", e))?;

            let items: Vec<Value> = results
                .iter()
                .map(|r| {
                    json!({
                        "id": r.item.id.to_string(),
                        "content": r.item.content,
                        "source_entity_id": r.item.source_entity_id.to_string(),
                        "published_timestamp": r.item.published_timestamp.to_rfc3339(),
                        "attribution_depth": format!("{:?}", r.item.attribution_depth).to_lowercase(),
                        "raw_source_link": r.item.raw_source_link,
                        "score": r.score,
                    })
                })
                .collect();

            let mut result = json!({ "results": items });
            truncate_search_results(&mut result, &ctx.tool_result_limits);
            truncate_claim_previews(&mut result, &ctx.tool_result_limits);
            Ok(result)
        })
    })
}
