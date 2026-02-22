use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use tokio::sync::RwLock;

mod cache;
mod fetch;
mod rate_limit;
mod routes;

use cache::UrlCache;
use rate_limit::DomainRateLimiter;

/// Shared application state.
pub struct AppState {
    pub http: reqwest::Client,
    pub cache: Arc<RwLock<UrlCache>>,
    pub rate_limiter: Arc<DomainRateLimiter>,
    pub metrics_handle: PrometheusHandle,
    /// SearXNG backend URL for web search.
    pub search_backend_url: String,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    tracing::info!("AutOSINT Fetch starting");

    // Install Prometheus metrics recorder.
    let metrics_handle = PrometheusBuilder::new()
        .install_recorder()
        .expect("Failed to install Prometheus metrics recorder");

    // Configure cache TTL from env (default 3600 seconds).
    let cache_ttl_secs: u64 = std::env::var("FETCH_CACHE_TTL")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3600);

    // Configure rate limit from env (default 2.0 requests per second per domain).
    let rate_limit: f64 = std::env::var("FETCH_RATE_LIMIT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2.0);

    let http = reqwest::Client::builder()
        .user_agent("AutOSINT-Fetch/0.1")
        .build()
        .expect("Failed to build HTTP client");

    let search_backend_url =
        std::env::var("SEARCH_BACKEND_URL").unwrap_or_else(|_| "http://localhost:8888".into());

    let state = Arc::new(AppState {
        http,
        cache: Arc::new(RwLock::new(UrlCache::new(Duration::from_secs(
            cache_ttl_secs,
        )))),
        rate_limiter: Arc::new(DomainRateLimiter::new(rate_limit)),
        metrics_handle,
        search_backend_url,
    });

    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/metrics", get(metrics_handler))
        .route("/fetch", post(routes::fetch_handler))
        .route("/search", post(routes::search_handler))
        .route("/sources", get(routes::sources_handler))
        .with_state(state);

    let port: u16 = std::env::var("FETCH_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8081);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port))
        .await
        .expect("Failed to bind TCP listener");

    tracing::info!(port = port, "AutOSINT Fetch listening");

    axum::serve(listener, app).await.expect("HTTP server error");
}

async fn health_handler() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(serde_json::json!({ "status": "healthy" })),
    )
}

async fn metrics_handler(State(state): State<Arc<AppState>>) -> String {
    state.metrics_handle.render()
}
