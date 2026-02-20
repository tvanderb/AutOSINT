use std::sync::atomic::Ordering;
use std::sync::Arc;

use autosint_common::config::{
    DedupConfig, LlmRoleConfig, RetryConfig, SafetyLimits, ToolResultLimits,
};
use autosint_common::ids::EntityId;
use autosint_common::types::SourceGuidance;
use serde_json::Value;

use crate::embeddings::EmbeddingClient;
use crate::graph::GraphClient;
use crate::llm::session::{run_session, SessionConfig, SessionResult};
use crate::llm::LlmClient;
use crate::tools::handlers::register_processor_tools;
use crate::tools::{SessionCounters, ToolHandlerContext, ToolRegistry};

/// Result of a Processor session, wrapping the generic session result
/// with domain-specific counters.
pub struct ProcessorSessionResult {
    pub outcome: SessionResult,
    pub entities_created: u32,
    pub claims_created: u32,
    pub relationships_created: u32,
}

/// A Processor session — fetches URLs, extracts entities/claims/relationships,
/// and writes them to the knowledge graph.
pub struct ProcessorSession {
    llm: Arc<LlmClient>,
    system_prompt: String,
    tool_registry: ToolRegistry,
    session_config: SessionConfig,
}

impl ProcessorSession {
    /// Create a new Processor session.
    ///
    /// `tool_schemas` should contain the loaded schemas from config (keyed as "processor/tool_name").
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        llm_config: &LlmRoleConfig,
        retry_config: &RetryConfig,
        safety_limits: &SafetyLimits,
        graph: Arc<GraphClient>,
        embedding_client: Option<Arc<EmbeddingClient>>,
        fetch_base_url: String,
        system_prompt: String,
        tool_schemas: &std::collections::HashMap<String, Value>,
        tool_result_limits: ToolResultLimits,
        dedup_config: DedupConfig,
    ) -> Result<Self, String> {
        let llm = LlmClient::new(llm_config.clone(), retry_config.clone())
            .ok_or_else(|| "Failed to create LLM client — API key not set".to_string())?;

        let context = ToolHandlerContext {
            graph,
            embedding_client,
            fetch_base_url,
            http: reqwest::Client::new(),
            tool_result_limits,
            dedup_config,
            session_counters: SessionCounters::default(),
        };

        let mut tool_registry = ToolRegistry::new(context);
        register_processor_tools(&mut tool_registry);
        tool_registry.load_definitions(tool_schemas, "processor")?;

        let session_config = SessionConfig {
            max_turns: safety_limits.max_turns_per_processor_session,
            max_consecutive_malformed: safety_limits.max_consecutive_malformed_tool_calls,
        };

        Ok(Self {
            llm: Arc::new(llm),
            system_prompt,
            tool_registry,
            session_config,
        })
    }

    /// Run the Processor session with the given work order details.
    pub async fn run(
        &self,
        objective: &str,
        referenced_entities: &[EntityId],
        source_guidance: Option<&SourceGuidance>,
    ) -> ProcessorSessionResult {
        let start = std::time::Instant::now();

        // Format the initial user message from the work order details.
        let initial_message =
            format_work_order_message(objective, referenced_entities, source_guidance);

        let executor = self.tool_registry.as_executor();

        let outcome = run_session(
            self.llm.as_ref(),
            &self.system_prompt,
            &initial_message,
            self.tool_registry.definitions(),
            &executor,
            &self.session_config,
        )
        .await;

        // Read counters from the tool handler context.
        let stats = outcome.stats();
        let entities_created = self
            .tool_registry
            .counters()
            .entities_created
            .load(Ordering::Relaxed);
        let claims_created = self
            .tool_registry
            .counters()
            .claims_created
            .load(Ordering::Relaxed);
        let relationships_created = self
            .tool_registry
            .counters()
            .relationships_created
            .load(Ordering::Relaxed);

        // Record metrics.
        let duration = start.elapsed().as_secs_f64();
        metrics::histogram!("processor.session.duration").record(duration);
        metrics::histogram!("processor.session.tool_calls").record(stats.tool_calls as f64);
        metrics::counter!("processor.session.entities_created").increment(entities_created as u64);
        metrics::counter!("processor.session.claims_created").increment(claims_created as u64);
        metrics::counter!("processor.session.relationships_created")
            .increment(relationships_created as u64);

        tracing::info!(
            duration_s = duration,
            turns = stats.turns,
            tool_calls = stats.tool_calls,
            entities = entities_created,
            claims = claims_created,
            relationships = relationships_created,
            "Processor session completed"
        );

        ProcessorSessionResult {
            outcome,
            entities_created,
            claims_created,
            relationships_created,
        }
    }
}

/// Format the work order into the initial user message for the LLM.
fn format_work_order_message(
    objective: &str,
    referenced_entities: &[EntityId],
    source_guidance: Option<&SourceGuidance>,
) -> String {
    let mut message = format!("## Work Order\n\n**Objective:** {}\n", objective);

    if !referenced_entities.is_empty() {
        message.push_str("\n**Referenced Entities (already in the knowledge graph):**\n");
        for entity_id in referenced_entities {
            message.push_str(&format!("- `{}`\n", entity_id));
        }
        message.push_str(
            "\nSearch for these entities to understand their current state before proceeding.\n",
        );
    }

    if let Some(guidance) = source_guidance {
        if !guidance.prefer.is_empty() {
            message.push_str("\n**Source Guidance:**\n");
            for source in &guidance.prefer {
                message.push_str(&format!("- Preferred source: {}\n", source));
            }
        }
        if !guidance.extra.is_empty() {
            for (key, value) in &guidance.extra {
                message.push_str(&format!("- {}: {}\n", key, value));
            }
        }
    }

    message.push_str(
        "\nProceed with fetching relevant sources and extracting all intelligence value.\n",
    );

    message
}
