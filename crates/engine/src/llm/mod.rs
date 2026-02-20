mod anthropic;
mod openai;
pub mod session;
pub mod types;

use std::future::Future;
use std::pin::Pin;

use autosint_common::config::{LlmRoleConfig, RetryConfig};

pub use types::{ContentBlock, LlmResponse, Message, Role, StopReason, TokenUsage, ToolDefinition};

/// LLM API client with provider dispatch and retry logic.
pub struct LlmClient {
    http: reqwest::Client,
    config: LlmRoleConfig,
    retry_config: RetryConfig,
    api_key: String,
}

/// Errors from LLM API calls.
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("LLM HTTP error: {0}")]
    Http(String),

    #[error("LLM auth error: {0}")]
    Auth(String),

    #[error("LLM rate limited (retry after {retry_after:?}s)")]
    RateLimited { retry_after: Option<u64> },

    #[error("LLM context window exceeded: {0}")]
    ContextWindowExceeded(String),

    #[error("LLM API error: {0}")]
    Api(String),

    #[error("LLM response parse error: {0}")]
    Parse(String),
}

impl LlmError {
    /// Whether this error should not be retried.
    fn is_non_retryable(&self) -> bool {
        matches!(self, LlmError::Auth(_) | LlmError::ContextWindowExceeded(_))
    }
}

impl From<LlmError> for autosint_common::AutOsintError {
    fn from(e: LlmError) -> Self {
        autosint_common::AutOsintError::LlmApi(e.to_string())
    }
}

impl LlmClient {
    /// Create a new LLM client.
    /// Reads the API key from the appropriate env var based on provider.
    /// Returns None if the key is not set.
    pub fn new(config: LlmRoleConfig, retry_config: RetryConfig) -> Option<Self> {
        let env_var = match config.provider.as_str() {
            "anthropic" => "ANTHROPIC_API_KEY",
            "openai" => "OPENAI_API_KEY",
            other => {
                tracing::warn!(provider = other, "Unknown LLM provider");
                return None;
            }
        };

        let api_key = match std::env::var(env_var) {
            Ok(key) if !key.is_empty() => key,
            _ => {
                tracing::warn!(
                    env_var = env_var,
                    provider = config.provider.as_str(),
                    "API key not set — LLM client disabled for this role"
                );
                return None;
            }
        };

        Some(Self {
            http: reqwest::Client::new(),
            config,
            retry_config,
            api_key,
        })
    }

    /// Send a chat request to the configured provider with retry logic.
    pub async fn chat(
        &self,
        system: &str,
        messages: &[Message],
        tools: &[ToolDefinition],
    ) -> Result<LlmResponse, LlmError> {
        let mut attempt = 0u32;
        let mut backoff_ms = self.retry_config.initial_backoff_ms;

        loop {
            attempt += 1;
            let result = self.send_once(system, messages, tools).await;

            match result {
                Ok(response) => return Ok(response),
                Err(ref e) if e.is_non_retryable() => {
                    metrics::counter!("llm.api.errors", "provider" => self.config.provider.clone())
                        .increment(1);
                    return result;
                }
                Err(LlmError::RateLimited { retry_after }) => {
                    if attempt >= self.retry_config.max_attempts {
                        metrics::counter!("llm.api.errors", "provider" => self.config.provider.clone())
                            .increment(1);
                        return Err(LlmError::RateLimited { retry_after });
                    }
                    let wait = retry_after.map(|s| s * 1000).unwrap_or(backoff_ms);
                    tracing::warn!(attempt, wait_ms = wait, "LLM rate limited, retrying");
                    tokio::time::sleep(std::time::Duration::from_millis(wait)).await;
                }
                Err(e) => {
                    if attempt >= self.retry_config.max_attempts {
                        metrics::counter!("llm.api.errors", "provider" => self.config.provider.clone())
                            .increment(1);
                        return Err(e);
                    }
                    let jitter = if self.retry_config.jitter {
                        compute_jitter(attempt, backoff_ms)
                    } else {
                        0
                    };
                    let wait = backoff_ms + jitter;
                    tracing::warn!(attempt, wait_ms = wait, error = %e, "LLM API error, retrying");
                    tokio::time::sleep(std::time::Duration::from_millis(wait)).await;
                    backoff_ms = (backoff_ms as f64 * self.retry_config.backoff_multiplier) as u64;
                    backoff_ms = backoff_ms.min(self.retry_config.max_backoff_ms);
                }
            }
        }
    }

    /// Single attempt — routes to provider-specific implementation.
    async fn send_once(
        &self,
        system: &str,
        messages: &[Message],
        tools: &[ToolDefinition],
    ) -> Result<LlmResponse, LlmError> {
        match self.config.provider.as_str() {
            "anthropic" => {
                anthropic::send_messages(
                    &self.http,
                    &self.api_key,
                    &self.config.model,
                    self.config.max_tokens,
                    self.config.temperature,
                    system,
                    messages,
                    tools,
                )
                .await
            }
            "openai" => {
                openai::send_chat_completion(
                    &self.http,
                    &self.api_key,
                    &self.config.model,
                    self.config.max_tokens,
                    self.config.temperature,
                    system,
                    messages,
                    tools,
                )
                .await
            }
            other => Err(LlmError::Api(format!("Unknown provider: {}", other))),
        }
    }
}

/// Compute jitter for retry backoff using simple hash-based approach.
fn compute_jitter(attempt: u32, backoff_ms: u64) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::hash::DefaultHasher::new();
    attempt.hash(&mut hasher);
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos()
        .hash(&mut hasher);
    hasher.finish() % (backoff_ms / 2 + 1)
}

/// Object-safe trait for testability (dyn dispatch).
/// Tests provide MockLlmCaller; production uses LlmClient.
pub trait LlmCaller: Send + Sync {
    fn chat<'a>(
        &'a self,
        system: &'a str,
        messages: &'a [Message],
        tools: &'a [ToolDefinition],
    ) -> Pin<Box<dyn Future<Output = Result<LlmResponse, LlmError>> + Send + 'a>>;
}

impl LlmCaller for LlmClient {
    fn chat<'a>(
        &'a self,
        system: &'a str,
        messages: &'a [Message],
        tools: &'a [ToolDefinition],
    ) -> Pin<Box<dyn Future<Output = Result<LlmResponse, LlmError>> + Send + 'a>> {
        Box::pin(self.chat(system, messages, tools))
    }
}
