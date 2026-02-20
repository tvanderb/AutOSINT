use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;

use autosint_common::api::fetch::{FetchMetadata, FetchRequest, FetchResponse, SourceInfo};

use crate::fetch::{extract_html_content, fetch_url};
use crate::AppState;

/// POST /fetch — fetch a URL, extract text, return content.
pub async fn fetch_handler(
    State(state): State<Arc<AppState>>,
    Json(request): Json<FetchRequest>,
) -> Result<Json<FetchResponse>, (StatusCode, String)> {
    let start = std::time::Instant::now();

    // Check cache first.
    {
        let cache = state.cache.read().await;
        if let Some((content, status_code, content_type)) = cache.get(&request.url) {
            return Ok(Json(FetchResponse {
                content,
                metadata: FetchMetadata {
                    status_code,
                    content_type,
                    url: request.url,
                    cached: true,
                },
            }));
        }
    }

    // Rate limit check.
    let domain = extract_domain(&request.url);
    state
        .rate_limiter
        .acquire(&domain, Duration::from_secs(30))
        .await
        .map_err(|e| (StatusCode::TOO_MANY_REQUESTS, e))?;

    // Fetch the URL.
    let timeout = request
        .options
        .as_ref()
        .and_then(|o| o.timeout_ms)
        .map(Duration::from_millis)
        .unwrap_or(Duration::from_secs(30));

    let (body, status_code, content_type) = fetch_url(&state.http, &request.url, Some(timeout))
        .await
        .map_err(|e| {
            metrics::counter!("fetch.request.errors", "domain" => domain.clone()).increment(1);
            (StatusCode::BAD_GATEWAY, e.to_string())
        })?;

    // Extract text from HTML content.
    let content = if content_type
        .as_deref()
        .is_some_and(|ct| ct.contains("text/html"))
    {
        extract_html_content(&body)
    } else {
        body.clone()
    };

    // Cache the result.
    {
        let mut cache = state.cache.write().await;
        cache.insert(
            request.url.clone(),
            content.clone(),
            status_code,
            content_type.clone(),
        );
    }

    let latency = start.elapsed().as_secs_f64();
    metrics::histogram!("fetch.request.total_latency", "domain" => domain).record(latency);

    Ok(Json(FetchResponse {
        content,
        metadata: FetchMetadata {
            status_code,
            content_type,
            url: request.url,
            cached: false,
        },
    }))
}

/// GET /sources — return the source catalog (empty until M5).
pub async fn sources_handler() -> Json<Vec<SourceInfo>> {
    Json(Vec::new())
}

fn extract_domain(url: &str) -> String {
    url.split("//")
        .nth(1)
        .unwrap_or(url)
        .split('/')
        .next()
        .unwrap_or("unknown")
        .to_string()
}
