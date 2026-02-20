use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    extract::State, http::StatusCode, response::IntoResponse, routing::get, routing::post, Json,
    Router,
};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use serde::Deserialize;

use autosint_engine::circuit_breaker::CircuitBreakerRegistry;
use autosint_engine::config;
use autosint_engine::embeddings;
use autosint_engine::graph;
use autosint_engine::orchestrator::Orchestrator;
use autosint_engine::processor::{ProcessorPool, ProcessorPoolConfig};
use autosint_engine::queue;
use autosint_engine::store;

/// Shared application state accessible from axum handlers.
struct AppState {
    graph: Arc<graph::GraphClient>,
    store: Arc<store::StoreClient>,
    queue: Arc<queue::QueueClient>,
    #[allow(dead_code)]
    embedding_client: Option<Arc<embeddings::EmbeddingClient>>,
    #[allow(dead_code)]
    engine_config: Arc<config::EngineConfig>,
    orchestrator: Arc<Orchestrator>,
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

    let store_client = Arc::new(store_client);

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

    let queue_client = Arc::new(queue_client);

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

    let fetch_base_url =
        std::env::var("FETCH_BASE_URL").unwrap_or_else(|_| "http://localhost:8081".into());

    let engine_config = Arc::new(engine_config);
    let tool_schemas = Arc::new(engine_config.tool_schemas.clone());

    // Start Processor pool (if LLM key available).
    let processor_prompt = engine_config
        .prompts
        .get("processor")
        .cloned()
        .unwrap_or_default();

    let _processor_pool = if autosint_engine::llm::LlmClient::new(
        engine_config.system.llm.processor.clone(),
        engine_config.system.retry.llm_api.clone(),
    )
    .is_some()
    {
        let pool_config = ProcessorPoolConfig {
            pool_size: engine_config.system.concurrency.processor_pool_size,
            heartbeat_ttl_seconds: engine_config.system.safety.heartbeat_ttl_seconds,
            heartbeat_interval_seconds: engine_config.system.safety.heartbeat_ttl_seconds / 3,
        };

        let pool = ProcessorPool::start(
            pool_config,
            engine_config.system.llm.processor.clone(),
            engine_config.system.retry.llm_api.clone(),
            Arc::clone(&graph_client),
            embedding_client.clone(),
            Arc::clone(&store_client),
            Arc::clone(&queue_client),
            fetch_base_url.clone(),
            processor_prompt,
            Arc::clone(&tool_schemas),
            engine_config.system.tool_results.clone(),
            engine_config.system.dedup.clone(),
            engine_config.system.safety.clone(),
        );

        tracing::info!("Processor pool started");
        Some(pool)
    } else {
        tracing::warn!("Processor LLM not available — Processor pool not started");
        None
    };

    // Circuit breakers for external dependency health tracking.
    let circuit_breakers = Arc::new(CircuitBreakerRegistry::new());

    // Create Orchestrator.
    let analyst_prompt = engine_config
        .prompts
        .get("analyst")
        .cloned()
        .unwrap_or_default();

    let orchestrator = Arc::new(Orchestrator::new(
        Arc::clone(&graph_client),
        Arc::clone(&store_client),
        Arc::clone(&queue_client),
        embedding_client.clone(),
        Arc::clone(&engine_config),
        fetch_base_url,
        Arc::clone(&tool_schemas),
        analyst_prompt,
        Arc::clone(&circuit_breakers),
    ));

    // Recover any non-terminal investigations from before restart.
    if let Err(e) = orchestrator.recover_on_startup().await {
        tracing::error!(error = %e, "Failed to recover investigations on startup");
    }

    // Spawn circuit breaker metrics reporter.
    {
        let cbs = Arc::clone(&circuit_breakers);
        tokio::spawn(async move {
            let interval = std::time::Duration::from_secs(30);
            loop {
                tokio::time::sleep(interval).await;
                cbs.report_metrics();
            }
        });
    }

    // Build shared state.
    let state = Arc::new(AppState {
        graph: graph_client,
        store: store_client,
        queue: queue_client,
        embedding_client,
        engine_config,
        orchestrator,
        metrics_handle,
    });

    // Build HTTP server.
    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/metrics", get(metrics_handler))
        .route("/investigate", post(investigate_handler))
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

/// Request body for starting an investigation.
#[derive(Deserialize)]
struct InvestigateRequest {
    prompt: String,
}

/// POST /investigate — start a new investigation.
async fn investigate_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<InvestigateRequest>,
) -> impl IntoResponse {
    let orchestrator = Arc::clone(&state.orchestrator);

    match orchestrator.start_investigation(&req.prompt).await {
        Ok(investigation_id) => {
            // Spawn investigation lifecycle in background.
            let orch = Arc::clone(&state.orchestrator);
            let inv_id = investigation_id;
            tokio::spawn(async move {
                if let Err(e) = orch.run_investigation(inv_id).await {
                    tracing::error!(
                        investigation_id = %inv_id,
                        error = %e,
                        "Investigation failed"
                    );
                }
            });

            let body = serde_json::json!({
                "investigation_id": investigation_id.to_string(),
                "status": "pending",
                "message": "Investigation started."
            });

            (StatusCode::ACCEPTED, Json(body))
        }
        Err(e) => {
            let body = serde_json::json!({
                "error": e,
            });
            (StatusCode::INTERNAL_SERVER_ERROR, Json(body))
        }
    }
}
