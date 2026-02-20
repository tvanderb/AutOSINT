use std::path::PathBuf;
use std::sync::Arc;

use axum::{extract::State, http::StatusCode, response::IntoResponse, routing::get, Json, Router};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};

use autosint_engine::config;
use autosint_engine::embeddings;
use autosint_engine::graph;
use autosint_engine::queue;
use autosint_engine::store;

/// Shared application state accessible from axum handlers.
struct AppState {
    graph: Arc<graph::GraphClient>,
    store: store::StoreClient,
    queue: queue::QueueClient,
    #[allow(dead_code)]
    embedding_client: Option<Arc<embeddings::EmbeddingClient>>,
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

    tracing::info!("AutOSINT Engine starting");

    // Load configuration — fail loudly on misconfiguration.
    let config_dir = std::env::var("AUTOSINT_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("config"));

    let engine_config = match config::load_config(&config_dir) {
        Ok(config) => {
            tracing::info!("Configuration loaded successfully");
            config
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to load configuration — refusing to start");
            std::process::exit(1);
        }
    };

    // Install Prometheus metrics recorder.
    let metrics_handle = PrometheusBuilder::new()
        .install_recorder()
        .expect("Failed to install Prometheus metrics recorder");

    // Connect to databases.
    let neo4j_uri = std::env::var("NEO4J_URI").unwrap_or_else(|_| "bolt://localhost:7687".into());
    let neo4j_user = std::env::var("NEO4J_USER").unwrap_or_else(|_| "neo4j".into());
    let neo4j_password = std::env::var("NEO4J_PASSWORD").unwrap_or_else(|_| "autosint_dev".into());
    let postgres_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://autosint:autosint_dev@localhost:5432/autosint".into());
    let redis_url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".into());

    // Neo4j
    let graph_client =
        match graph::GraphClient::connect(&neo4j_uri, &neo4j_user, &neo4j_password).await {
            Ok(client) => client,
            Err(e) => {
                tracing::error!(error = %e, "Failed to connect to Neo4j");
                std::process::exit(1);
            }
        };

    if let Err(e) = graph_client.initialize_schema().await {
        tracing::error!(error = %e, "Failed to initialize Neo4j schema");
        std::process::exit(1);
    }

    let graph_client = Arc::new(graph_client);

    // PostgreSQL
    let store_client = match store::StoreClient::connect(&postgres_url, 10).await {
        Ok(client) => client,
        Err(e) => {
            tracing::error!(error = %e, "Failed to connect to PostgreSQL");
            std::process::exit(1);
        }
    };

    if let Err(e) = store_client.migrate().await {
        tracing::error!(error = %e, "Failed to run PostgreSQL migrations");
        std::process::exit(1);
    }

    // Redis
    let queue_client = match queue::QueueClient::connect(&redis_url).await {
        Ok(client) => client,
        Err(e) => {
            tracing::error!(error = %e, "Failed to connect to Redis");
            std::process::exit(1);
        }
    };

    if let Err(e) = queue_client.initialize_streams().await {
        tracing::error!(error = %e, "Failed to initialize Redis streams");
        std::process::exit(1);
    }

    tracing::info!("All databases connected and initialized");

    // Embedding client (optional — gracefully handle missing API key).
    let embedding_client = embeddings::EmbeddingClient::new(
        engine_config.system.embeddings.clone(),
        engine_config.system.retry.llm_api.clone(),
    )
    .map(Arc::new);

    // Spawn embedding backfill task if client is available.
    if let Some(ref client) = embedding_client {
        let _backfill_handle = embeddings::spawn_backfill_task(
            Arc::clone(&graph_client),
            Arc::clone(client),
            engine_config.system.embeddings.backfill_interval_minutes,
            engine_config.system.embeddings.batch_size,
        );
    }

    // Build shared state.
    let state = Arc::new(AppState {
        graph: graph_client,
        store: store_client,
        queue: queue_client,
        embedding_client,
        metrics_handle,
    });

    // Build HTTP server.
    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/metrics", get(metrics_handler))
        .with_state(state);

    let port: u16 = std::env::var("ENGINE_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port))
        .await
        .expect("Failed to bind TCP listener");

    tracing::info!(port = port, "AutOSINT Engine listening");

    axum::serve(listener, app).await.expect("HTTP server error");
}

/// Health check endpoint. Checks all three database connections.
async fn health_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let neo4j_ok = state.graph.health_check().await.is_ok();
    let postgres_ok = state.store.health_check().await.is_ok();
    let redis_ok = state.queue.health_check().await.is_ok();

    let all_healthy = neo4j_ok && postgres_ok && redis_ok;

    let status = if all_healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    let body = serde_json::json!({
        "status": if all_healthy { "healthy" } else { "unhealthy" },
        "services": {
            "neo4j": if neo4j_ok { "healthy" } else { "unhealthy" },
            "postgres": if postgres_ok { "healthy" } else { "unhealthy" },
            "redis": if redis_ok { "healthy" } else { "unhealthy" },
        }
    });

    (status, Json(body))
}

/// Prometheus metrics endpoint.
async fn metrics_handler(State(state): State<Arc<AppState>>) -> String {
    state.metrics_handle.render()
}
