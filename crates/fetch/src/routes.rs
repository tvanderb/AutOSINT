use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;

use autosint_common::api::fetch::{
    FetchMetadata, FetchRequest, FetchResponse, SearchRequest, SearchResponse, SearchResult,
    SourceInfo,
};

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
        .acquire(&domain, Duration::from_secs(120))
        .await
        .map_err(|e| (StatusCode::TOO_MANY_REQUESTS, e))?;

    // Fetch the URL.
    let timeout = request
        .options
        .as_ref()
        .and_then(|o| o.timeout_ms)
        .map(Duration::from_millis)
        .unwrap_or(Duration::from_secs(120));

    let (body, status_code, content_type) = fetch_url(&state.http, &request.url, Some(timeout))
        .await
        .map_err(|e| {
            metrics::counter!("fetch.request.errors", "domain" => domain.clone()).increment(1);
            (StatusCode::BAD_GATEWAY, e.to_string())
        })?;

    // Reject binary content types — only text-based responses are useful.
    if let Some(ref ct) = content_type {
        let ct_lower = ct.to_lowercase();
        let is_text = ct_lower.contains("text/")
            || ct_lower.contains("application/json")
            || ct_lower.contains("application/xml")
            || ct_lower.contains("application/xhtml");
        if !is_text {
            return Err((
                StatusCode::UNPROCESSABLE_ENTITY,
                format!(
                    "Unsupported content type: {}. Only text-based content is supported.",
                    ct
                ),
            ));
        }
    }

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

/// POST /search — web search via SearXNG backend.
pub async fn search_handler(
    State(state): State<Arc<AppState>>,
    Json(request): Json<SearchRequest>,
) -> Result<Json<SearchResponse>, (StatusCode, String)> {
    let start = std::time::Instant::now();
    let num_results = request.num_results.unwrap_or(10).min(20);

    let search_url = format!("{}/search", state.search_backend_url.trim_end_matches('/'));

    let response = state
        .http
        .get(&search_url)
        .query(&[
            ("q", request.query.as_str()),
            ("format", "json"),
            ("categories", "general"),
        ])
        .send()
        .await
        .map_err(|e| {
            metrics::counter!("fetch.search.errors").increment(1);
            (
                StatusCode::BAD_GATEWAY,
                format!("Search backend request failed: {}", e),
            )
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        metrics::counter!("fetch.search.errors").increment(1);
        return Err((
            StatusCode::BAD_GATEWAY,
            format!("Search backend returned {}: {}", status, body),
        ));
    }

    let searx: SearxResponse = response.json().await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            format!("Failed to parse search response: {}", e),
        )
    })?;

    let results: Vec<SearchResult> = searx
        .results
        .into_iter()
        .take(num_results)
        .map(|r| SearchResult {
            url: r.url,
            title: r.title,
            snippet: r.content,
        })
        .collect();

    let latency = start.elapsed().as_secs_f64();
    metrics::histogram!("fetch.search.latency").record(latency);
    metrics::counter!("fetch.search.count").increment(1);

    Ok(Json(SearchResponse {
        query: request.query,
        results,
    }))
}

/// SearXNG JSON response (internal).
#[derive(serde::Deserialize)]
struct SearxResponse {
    #[serde(default)]
    results: Vec<SearxResult>,
}

#[derive(serde::Deserialize)]
struct SearxResult {
    url: String,
    title: String,
    #[serde(default)]
    content: String,
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
