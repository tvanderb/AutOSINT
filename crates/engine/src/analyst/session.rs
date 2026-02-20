use std::sync::atomic::Ordering;
use std::sync::Arc;

use autosint_common::config::{
    DedupConfig, LlmRoleConfig, RetryConfig, SafetyLimits, ToolResultLimits,
};
use autosint_common::ids::InvestigationId;
use serde_json::Value;

use crate::embeddings::EmbeddingClient;
use crate::graph::GraphClient;
use crate::llm::session::{run_session, SessionConfig, SessionResult};
use crate::llm::LlmClient;
use crate::queue::QueueClient;
use crate::store::StoreClient;
use crate::tools::handlers::register_analyst_tools;
use crate::tools::{SessionCounters, ToolHandlerContext, ToolRegistry};

/// Outcome of an Analyst session, determined by which tools were called.
pub enum AnalystOutcome {
    /// Analyst created work orders — needs more data.
    WorkOrdersCreated { count: u32 },
    /// Analyst produced an assessment — investigation complete.
    AssessmentProduced,
    /// No work orders and no assessment — empty session.
    EmptySession,
    /// LLM error or max turns reached.
    Failed { error: String },
}

/// Result of an Analyst session with raw session stats.
pub struct AnalystSessionResult {
    pub outcome: AnalystOutcome,
    pub session_result: SessionResult,
}

/// An Analyst session — queries the knowledge graph, identifies gaps,
/// creates work orders or produces an assessment.
pub struct AnalystSession {
    llm: Arc<LlmClient>,
    system_prompt: String,
    tool_registry: ToolRegistry,
    session_config: SessionConfig,
}

impl AnalystSession {
    /// Create a new Analyst session.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        llm_config: &LlmRoleConfig,
        retry_config: &RetryConfig,
        safety_limits: &SafetyLimits,
        graph: Arc<GraphClient>,
        embedding_client: Option<Arc<EmbeddingClient>>,
        store: Arc<StoreClient>,
        queue: Arc<QueueClient>,
        fetch_base_url: String,
        system_prompt: String,
        tool_schemas: &std::collections::HashMap<String, Value>,
        tool_result_limits: ToolResultLimits,
        dedup_config: DedupConfig,
        investigation_id: InvestigationId,
        investigation_cycle: i32,
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
            store: Some(store),
            queue: Some(queue),
            investigation_id: Some(investigation_id),
            investigation_cycle: Some(investigation_cycle),
            max_work_orders_per_cycle: Some(safety_limits.max_work_orders_per_cycle),
        };

        let mut tool_registry = ToolRegistry::new(context);
        register_analyst_tools(&mut tool_registry);
        tool_registry.load_definitions(tool_schemas, "analyst")?;

        let session_config = SessionConfig {
            max_turns: safety_limits.max_turns_per_analyst_session,
            max_consecutive_malformed: safety_limits.max_consecutive_malformed_tool_calls,
        };

        Ok(Self {
            llm: Arc::new(llm),
            system_prompt,
            tool_registry,
            session_config,
        })
    }

    /// Run the Analyst session with the investigation prompt.
    pub async fn run(&self, investigation_prompt: &str) -> AnalystSessionResult {
        let start = std::time::Instant::now();

        let executor = self.tool_registry.as_executor();

        let session_result = run_session(
            self.llm.as_ref(),
            &self.system_prompt,
            investigation_prompt,
            self.tool_registry.definitions(),
            &executor,
            &self.session_config,
        )
        .await;

        // Read counters to determine outcome.
        let work_orders_created = self
            .tool_registry
            .counters()
            .work_orders_created
            .load(Ordering::Relaxed);
        let assessment_produced = self
            .tool_registry
            .counters()
            .assessment_produced
            .load(Ordering::Relaxed);

        let outcome = match &session_result {
            SessionResult::Failed { error, .. } => AnalystOutcome::Failed {
                error: error.clone(),
            },
            SessionResult::MaxTurnsReached { .. } => AnalystOutcome::Failed {
                error: "Max turns reached".to_string(),
            },
            SessionResult::MalformedToolCallLimit { .. } => AnalystOutcome::Failed {
                error: "Malformed tool call limit reached".to_string(),
            },
            SessionResult::Completed { .. } => {
                if assessment_produced {
                    AnalystOutcome::AssessmentProduced
                } else if work_orders_created > 0 {
                    AnalystOutcome::WorkOrdersCreated {
                        count: work_orders_created,
                    }
                } else {
                    AnalystOutcome::EmptySession
                }
            }
        };

        // Record metrics.
        let duration = start.elapsed().as_secs_f64();
        let stats = session_result.stats();
        let outcome_label = match &outcome {
            AnalystOutcome::WorkOrdersCreated { .. } => "work_orders",
            AnalystOutcome::AssessmentProduced => "assessment",
            AnalystOutcome::EmptySession => "empty",
            AnalystOutcome::Failed { .. } => "failed",
        };

        metrics::histogram!("analyst.session.duration").record(duration);
        metrics::histogram!("analyst.session.tool_calls").record(stats.tool_calls as f64);
        metrics::counter!("analyst.session.outcome", "outcome" => outcome_label.to_string())
            .increment(1);

        tracing::info!(
            duration_s = duration,
            turns = stats.turns,
            tool_calls = stats.tool_calls,
            work_orders = work_orders_created,
            assessment = assessment_produced,
            outcome = outcome_label,
            "Analyst session completed"
        );

        AnalystSessionResult {
            outcome,
            session_result,
        }
    }
}

/// Append a force-final directive to the system prompt for the last cycle.
pub fn force_final_prompt(base_prompt: &str) -> String {
    format!(
        "{}\n\n---\n\n\
        **CRITICAL: This is your FINAL cycle.** You MUST produce an assessment now using the \
        `produce_assessment` tool. You cannot create more work orders. Synthesize everything you \
        know, clearly state what remains unknown, and produce the best assessment possible with \
        the available information. An honest assessment noting gaps and uncertainties is far more \
        valuable than no assessment at all.",
        base_prompt
    )
}
