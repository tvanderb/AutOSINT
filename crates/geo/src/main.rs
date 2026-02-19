use std::sync::Arc;

use axum::{extract::State, http::StatusCode, response::IntoResponse, routing::get, Json, Router};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};

/// Shared application state.
struct AppState {
    metrics_handle: PrometheusHandle,
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

    tracing::info!("AutOSINT Geo starting");

    // Install Prometheus metrics recorder.
    let metrics_handle = PrometheusBuilder::new()
        .install_recorder()
        .expect("Failed to install Prometheus metrics recorder");

    let state = Arc::new(AppState { metrics_handle });

    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/metrics", get(metrics_handler))
        .with_state(state);

    let port: u16 = std::env::var("GEO_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8082);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port))
        .await
        .expect("Failed to bind TCP listener");

    tracing::info!(port = port, "AutOSINT Geo listening");

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
