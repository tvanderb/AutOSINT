use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU32};
use std::sync::Arc;

use serde_json::Value;

use autosint_common::config::{DedupConfig, ToolResultLimits};
use autosint_common::ids::InvestigationId;

use crate::embeddings::EmbeddingClient;
use crate::graph::GraphClient;
use crate::llm::session::{ToolExecutionResult, ToolExecutor};
use crate::llm::types::ToolDefinition;
use crate::queue::QueueClient;
use crate::store::StoreClient;

/// Shared context available to all tool handlers.
pub struct ToolHandlerContext {
    pub graph: Arc<GraphClient>,
    pub embedding_client: Option<Arc<EmbeddingClient>>,
    pub fetch_base_url: String,
    pub http: reqwest::Client,
    pub tool_result_limits: ToolResultLimits,
    pub dedup_config: DedupConfig,
    pub session_counters: SessionCounters,
    // Analyst-specific context (None for Processor sessions).
    pub store: Option<Arc<StoreClient>>,
    pub queue: Option<Arc<QueueClient>>,
    pub investigation_id: Option<InvestigationId>,
    pub investigation_cycle: Option<i32>,
    pub max_work_orders_per_cycle: Option<u32>,
}

/// Counters tracking write operations during a session.
pub struct SessionCounters {
    pub entities_created: AtomicU32,
    pub claims_created: AtomicU32,
    pub relationships_created: AtomicU32,
    pub work_orders_created: AtomicU32,
    pub assessment_produced: AtomicBool,
}

impl Default for SessionCounters {
    fn default() -> Self {
        Self {
            entities_created: AtomicU32::new(0),
            claims_created: AtomicU32::new(0),
            relationships_created: AtomicU32::new(0),
            work_orders_created: AtomicU32::new(0),
            assessment_produced: AtomicBool::new(false),
        }
    }
}

/// Handler function signature — takes args and context, returns JSON or error string.
pub type ToolHandler = Arc<
    dyn Fn(
            Value,
            Arc<ToolHandlerContext>,
        ) -> Pin<Box<dyn Future<Output = Result<Value, String>> + Send>>
        + Send
        + Sync,
>;

/// Registry of tool handlers with their schema definitions.
pub struct ToolRegistry {
    handlers: HashMap<String, ToolHandler>,
    definitions: Vec<ToolDefinition>,
    context: Arc<ToolHandlerContext>,
}

impl ToolRegistry {
    pub fn new(context: ToolHandlerContext) -> Self {
        Self {
            handlers: HashMap::new(),
            definitions: Vec::new(),
            context: Arc::new(context),
        }
    }

    /// Register a tool handler by name.
    pub fn register(&mut self, name: &str, handler: ToolHandler) {
        self.handlers.insert(name.to_string(), handler);
    }

    /// Load tool definitions from the config-loaded schemas.
    /// Filters to schemas matching the given role prefix (e.g. "processor").
    pub fn load_definitions(
        &mut self,
        tool_schemas: &HashMap<String, Value>,
        role: &str,
    ) -> Result<(), String> {
        let prefix = format!("{}/", role);

        for (key, schema) in tool_schemas {
            if !key.starts_with(&prefix) {
                continue;
            }

            let name = schema
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| format!("Tool schema '{}' missing 'name' field", key))?
                .to_string();

            let description = schema
                .get("description")
                .and_then(|v| v.as_str())
                .ok_or_else(|| format!("Tool schema '{}' missing 'description' field", key))?
                .to_string();

            let input_schema = schema
                .get("input_schema")
                .cloned()
                .ok_or_else(|| format!("Tool schema '{}' missing 'input_schema' field", key))?;

            self.definitions.push(ToolDefinition {
                name,
                description,
                input_schema,
            });
        }

        tracing::info!(
            role = role,
            tools = self.definitions.len(),
            "Loaded tool definitions"
        );

        Ok(())
    }

    /// Get the tool definitions for sending to the LLM.
    pub fn definitions(&self) -> &[ToolDefinition] {
        &self.definitions
    }

    /// Get a reference to the session counters.
    pub fn counters(&self) -> &SessionCounters {
        &self.context.session_counters
    }

    /// Execute a tool call by name.
    pub async fn execute(&self, tool_name: &str, args: Value) -> ToolExecutionResult {
        let start = std::time::Instant::now();

        let handler = match self.handlers.get(tool_name) {
            Some(h) => h,
            None => {
                metrics::counter!("tools.execution.errors", "tool" => tool_name.to_string())
                    .increment(1);
                return ToolExecutionResult {
                    content: format!(
                        "Unknown tool: '{}'. Available tools: {:?}",
                        tool_name,
                        self.handlers.keys().collect::<Vec<_>>()
                    ),
                    is_error: true,
                    is_malformed: true,
                };
            }
        };

        let result = handler(args, Arc::clone(&self.context)).await;

        let latency = start.elapsed().as_secs_f64();
        metrics::histogram!("tools.execution.latency", "tool" => tool_name.to_string())
            .record(latency);
        metrics::counter!("tools.execution.count", "tool" => tool_name.to_string()).increment(1);

        match result {
            Ok(value) => {
                let content = serde_json::to_string(&value).unwrap_or_else(|e| {
                    format!("{{\"error\": \"Failed to serialize result: {}\"}}", e)
                });
                ToolExecutionResult {
                    content,
                    is_error: false,
                    is_malformed: false,
                }
            }
            Err(msg) => {
                metrics::counter!("tools.execution.errors", "tool" => tool_name.to_string())
                    .increment(1);
                // Check if this was a serde parse error (malformed args from LLM).
                let is_malformed = msg.contains("invalid type")
                    || msg.contains("missing field")
                    || msg.contains("expected");
                ToolExecutionResult {
                    content: msg,
                    is_error: true,
                    is_malformed,
                }
            }
        }
    }

    /// Create a ToolExecutor closure for use with `run_session`.
    pub fn as_executor(&self) -> ToolExecutor {
        let handlers = self.handlers.clone();
        let context = Arc::clone(&self.context);

        Box::new(move |name: String, args: Value| {
            let handlers = handlers.clone();
            let context = Arc::clone(&context);

            Box::pin(async move {
                let start = std::time::Instant::now();

                tracing::info!(tool = %name, "Tool call started");

                let handler = match handlers.get(&name) {
                    Some(h) => h,
                    None => {
                        tracing::warn!(tool = %name, "Unknown tool called");
                        metrics::counter!("tools.execution.errors", "tool" => name.clone())
                            .increment(1);
                        return ToolExecutionResult {
                            content: format!(
                                "Unknown tool: '{}'. Check tool name and try again.",
                                name
                            ),
                            is_error: true,
                            is_malformed: true,
                        };
                    }
                };

                let result = handler(args, Arc::clone(&context)).await;

                let latency = start.elapsed().as_secs_f64();
                metrics::histogram!("tools.execution.latency", "tool" => name.clone())
                    .record(latency);
                metrics::counter!("tools.execution.count", "tool" => name.clone()).increment(1);

                match result {
                    Ok(value) => {
                        let content = serde_json::to_string(&value).unwrap_or_else(|e| {
                            format!("{{\"error\": \"Failed to serialize result: {}\"}}", e)
                        });
                        let content_len = content.len();
                        tracing::info!(
                            tool = %name,
                            latency_s = latency,
                            result_len = content_len,
                            "Tool call succeeded"
                        );
                        ToolExecutionResult {
                            content,
                            is_error: false,
                            is_malformed: false,
                        }
                    }
                    Err(ref msg) => {
                        tracing::warn!(
                            tool = %name,
                            latency_s = latency,
                            error = %msg,
                            "Tool call failed"
                        );
                        metrics::counter!("tools.execution.errors", "tool" => name).increment(1);
                        let is_malformed = msg.contains("invalid type")
                            || msg.contains("missing field")
                            || msg.contains("expected");
                        ToolExecutionResult {
                            content: msg.clone(),
                            is_error: true,
                            is_malformed,
                        }
                    }
                }
            })
        })
    }
}
