use serde::{Deserialize, Serialize};

/// Top-level system configuration, deserialized from system.toml.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SystemConfig {
    pub safety: SafetyLimits,
    pub concurrency: ConcurrencyConfig,
    pub llm: LlmConfig,
    pub embeddings: EmbeddingConfig,
    pub dedup: DedupConfig,
    pub retry: RetryDefaults,
    pub cache: CacheConfig,
    pub tool_results: ToolResultLimits,
}

/// Safety limits per PLAN.md §4.7.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SafetyLimits {
    /// Max investigation cycles before forced final assessment.
    pub max_cycles_per_investigation: u32,
    /// Max tool calls per Analyst session.
    pub max_turns_per_analyst_session: u32,
    /// Max tool calls per Processor session.
    pub max_turns_per_processor_session: u32,
    /// Max work orders an Analyst can create in a single cycle.
    pub max_work_orders_per_cycle: u32,
    /// Heartbeat TTL in seconds. Expired = Processor dead.
    pub heartbeat_ttl_seconds: u64,
    /// Consecutive cycles where ALL work orders failed → FAILED state.
    pub consecutive_all_fail_limit: u32,
    /// Consecutive malformed tool calls before ending LLM session.
    pub max_consecutive_malformed_tool_calls: u32,
}

/// Concurrency parameters.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConcurrencyConfig {
    /// Number of Processor tokio tasks in the pool.
    pub processor_pool_size: u32,
    /// Max concurrent browser contexts in fetch-browser sidecar.
    pub browser_context_cap: u32,
}

/// LLM provider and model configuration per role.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LlmConfig {
    pub analyst: LlmRoleConfig,
    pub processor: LlmRoleConfig,
}

/// Configuration for a single LLM role.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LlmRoleConfig {
    /// Provider name ("anthropic" or "openai").
    /// Use "openai" for OpenAI-compatible providers (OpenRouter, Azure, etc.).
    pub provider: String,
    /// Model identifier (e.g. "claude-opus-4-20250514", "anthropic/claude-sonnet-4-20250514").
    pub model: String,
    /// Max tokens in the response.
    pub max_tokens: u32,
    /// Temperature (0.0–1.0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    /// Base URL for the API. Defaults to provider's standard URL.
    /// Override for OpenRouter, Azure, or other compatible endpoints.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// Environment variable name for the API key. Defaults to provider's standard env var.
    /// Override to use a different key source (e.g. `OPENROUTER_API_KEY`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key_env: Option<String>,
}

/// Embedding pipeline configuration per PLAN.md §11.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    /// Provider name ("openai").
    pub provider: String,
    /// Model identifier (e.g. "text-embedding-3-small").
    pub model: String,
    /// Embedding vector dimensions.
    pub dimensions: u32,
    /// Max texts per batch API call.
    pub batch_size: u32,
    /// Interval in minutes for background backfill of pending embeddings.
    pub backfill_interval_minutes: u32,
    /// Base URL for the embedding API. Defaults to `https://api.openai.com/v1`.
    /// Use `https://openrouter.ai/api/v1` for OpenRouter, etc.
    #[serde(default = "default_embedding_base_url")]
    pub base_url: String,
    /// Environment variable name for the API key. Defaults to "OPENAI_API_KEY".
    #[serde(default = "default_embedding_api_key_env")]
    pub api_key_env: String,
}

fn default_embedding_base_url() -> String {
    "https://api.openai.com/v1".to_string()
}

fn default_embedding_api_key_env() -> String {
    "OPENAI_API_KEY".to_string()
}

/// Entity deduplication thresholds.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DedupConfig {
    /// Jaro-Winkler similarity threshold for fuzzy string matching (0.0–1.0).
    pub fuzzy_threshold: f64,
    /// Cosine similarity threshold for embedding-based matching (0.0–1.0).
    pub embedding_threshold: f64,
}

/// Default retry parameters per PLAN.md §11.
/// Per-target overrides can be specified.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RetryDefaults {
    pub llm_api: RetryConfig,
    pub databases: RetryConfig,
    pub external_modules: RetryConfig,
}

/// Retry configuration for a specific target.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RetryConfig {
    pub max_attempts: u32,
    pub initial_backoff_ms: u64,
    pub max_backoff_ms: u64,
    pub backoff_multiplier: f64,
    pub jitter: bool,
}

/// Cache TTL configuration.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Fetch URL cache TTL in seconds.
    pub fetch_ttl_seconds: u64,
}

/// Tool result size limits per PLAN.md §4.9.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolResultLimits {
    /// Max items returned from search results.
    pub max_search_results: u32,
    /// Max characters for entity detail responses before truncation.
    pub max_entity_detail_chars: u32,
    /// Max characters for claim content previews.
    pub max_claim_preview_chars: u32,
}
