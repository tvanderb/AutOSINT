///! Integration tests for Processor sessions.
///! All tests are `#[ignore]` — run with `cargo test -- --ignored` against live services.
///!
///! Requirements: ANTHROPIC_API_KEY, running Neo4j, running Fetch service.
use std::sync::Arc;

use neo4rs::query;

use autosint_common::config::{DedupConfig, ToolResultLimits};
use autosint_engine::config;
use autosint_engine::graph::GraphClient;
use autosint_engine::processor::ProcessorSession;

async fn setup() -> (Arc<GraphClient>, config::EngineConfig) {
    let uri = std::env::var("NEO4J_URI").unwrap_or_else(|_| "bolt://localhost:7687".into());
    let user = std::env::var("NEO4J_USER").unwrap_or_else(|_| "neo4j".into());
    let password = std::env::var("NEO4J_PASSWORD").unwrap_or_else(|_| "autosint_dev".into());

    let client = GraphClient::connect(&uri, &user, &password)
        .await
        .expect("Failed to connect to Neo4j");

    // Clean all data.
    client
        .inner()
        .run(query("MATCH (n) DETACH DELETE n"))
        .await
        .expect("Failed to clean database");

    // Initialize schema.
    client
        .initialize_schema()
        .await
        .expect("Failed to initialize schema");

    let config_dir = std::env::var("AUTOSINT_CONFIG_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("../../config"));

    let engine_config = config::load_config(&config_dir).expect("Failed to load config");

    (Arc::new(client), engine_config)
}

#[tokio::test]
#[ignore]
async fn test_processor_session_basic() {
    let (graph, engine_config) = setup().await;

    let fetch_base_url =
        std::env::var("FETCH_BASE_URL").unwrap_or_else(|_| "http://localhost:8081".into());

    let system_prompt = engine_config
        .prompts
        .get("processor")
        .expect("processor prompt not found")
        .clone();

    let session = ProcessorSession::new(
        &engine_config.system.llm.processor,
        &engine_config.system.retry.llm_api,
        &engine_config.system.safety,
        Arc::clone(&graph),
        None, // No embedding client for basic test
        fetch_base_url,
        system_prompt,
        &engine_config.tool_schemas,
        engine_config.system.tool_results.clone(),
        engine_config.system.dedup.clone(),
    )
    .expect("Failed to create ProcessorSession");

    let result = session
        .run(
            "Fetch and extract information from https://example.com",
            &[],
            None,
        )
        .await;

    // Verify the session completed (not failed).
    match &result.outcome {
        autosint_engine::llm::session::SessionResult::Completed { final_text, stats } => {
            println!("Session completed in {} turns", stats.turns);
            println!("Tool calls: {}", stats.tool_calls);
            println!("Final text: {}", final_text);
        }
        autosint_engine::llm::session::SessionResult::MaxTurnsReached { stats } => {
            println!(
                "Session hit max turns ({}) — this may be expected for complex content",
                stats.turns
            );
        }
        autosint_engine::llm::session::SessionResult::Failed { error, .. } => {
            panic!("Session failed: {}", error);
        }
        autosint_engine::llm::session::SessionResult::MalformedToolCallLimit { stats } => {
            panic!(
                "Session hit malformed limit after {} tool calls",
                stats.malformed_tool_calls
            );
        }
    }

    println!(
        "Entities created: {}, Claims created: {}, Relationships created: {}",
        result.entities_created, result.claims_created, result.relationships_created
    );
}
