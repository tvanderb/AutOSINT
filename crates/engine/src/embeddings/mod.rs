mod backfill;
mod openai;

use autosint_common::config::{EmbeddingConfig, RetryConfig};

pub use backfill::spawn_backfill_task;

/// Client for computing text embeddings via an external API.
pub struct EmbeddingClient {
    http: reqwest::Client,
    config: EmbeddingConfig,
    retry_config: RetryConfig,
    api_key: String,
}

#[derive(Debug, thiserror::Error)]
pub enum EmbeddingError {
    #[error("Embedding API HTTP error: {0}")]
    Http(String),

    #[error("Embedding API auth error: {0}")]
    Auth(String),

    #[error("Embedding API rate limited (retry after {retry_after:?}s)")]
    RateLimited { retry_after: Option<u64> },

    #[error("Embedding dimension mismatch: expected {expected}, got {got}")]
    DimensionMismatch { expected: u32, got: usize },

    #[error("Embedding API error: {0}")]
    Api(String),
}

#[allow(dead_code)]
impl EmbeddingClient {
    /// Create a new embedding client.
    /// Reads the API key from `OPENAI_API_KEY` env var.
    /// Returns None if the key is not set (graceful degradation).
    pub fn new(config: EmbeddingConfig, retry_config: RetryConfig) -> Option<Self> {
        let api_key = match std::env::var("OPENAI_API_KEY") {
            Ok(key) if !key.is_empty() => key,
            _ => {
                tracing::warn!(
                    "OPENAI_API_KEY not set — embedding client disabled. \
                     Entities will be created with embedding_pending = true."
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

    /// Embed a single text string.
    pub async fn embed_single(&self, text: &str) -> Result<Vec<f32>, EmbeddingError> {
        let results = self.embed_batch(&[text.to_string()]).await?;
        results
            .into_iter()
            .next()
            .ok_or_else(|| EmbeddingError::Api("Empty response from embedding API".into()))
    }

    /// Embed a batch of texts. Splits into sub-batches per config.batch_size.
    pub async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let batch_size = self.config.batch_size as usize;
        let mut all_embeddings = Vec::with_capacity(texts.len());

        for chunk in texts.chunks(batch_size) {
            let embeddings = self.call_api(chunk).await?;
            all_embeddings.extend(embeddings);
        }

        Ok(all_embeddings)
    }

    /// Get the configured embedding dimensions.
    pub fn dimensions(&self) -> u32 {
        self.config.dimensions
    }

    /// Call the OpenAI-compatible embedding API with retry logic.
    async fn call_api(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        let mut attempt = 0u32;
        let mut backoff_ms = self.retry_config.initial_backoff_ms;

        loop {
            attempt += 1;
            match openai::call_openai_embeddings(
                &self.http,
                &self.api_key,
                &self.config.model,
                self.config.dimensions,
                texts,
            )
            .await
            {
                Ok(embeddings) => {
                    metrics::counter!("embedding.api.tokens").increment(
                        texts.iter().map(|t| t.len() as u64 / 4).sum::<u64>(), // rough token estimate
                    );
                    return Ok(embeddings);
                }
                Err(EmbeddingError::Auth(_)) | Err(EmbeddingError::DimensionMismatch { .. }) => {
                    // Non-retryable errors.
                    metrics::counter!("embedding.api.errors").increment(1);
                    return Err(EmbeddingError::Api(format!(
                        "Non-retryable error on attempt {}",
                        attempt
                    )));
                }
                Err(EmbeddingError::RateLimited { retry_after }) => {
                    if attempt >= self.retry_config.max_attempts {
                        metrics::counter!("embedding.api.errors").increment(1);
                        return Err(EmbeddingError::RateLimited { retry_after });
                    }
                    let wait = retry_after.map(|s| s * 1000).unwrap_or(backoff_ms);
                    tracing::warn!(attempt, wait_ms = wait, "Rate limited, retrying");
                    tokio::time::sleep(std::time::Duration::from_millis(wait)).await;
                }
                Err(e) => {
                    if attempt >= self.retry_config.max_attempts {
                        metrics::counter!("embedding.api.errors").increment(1);
                        return Err(e);
                    }
                    let jitter = if self.retry_config.jitter {
                        use std::hash::{Hash, Hasher};
                        let mut hasher = std::hash::DefaultHasher::new();
                        attempt.hash(&mut hasher);
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .subsec_nanos()
                            .hash(&mut hasher);
                        hasher.finish() % (backoff_ms / 2 + 1)
                    } else {
                        0
                    };
                    let wait = backoff_ms + jitter;
                    tracing::warn!(attempt, wait_ms = wait, error = %e, "Embedding API error, retrying");
                    tokio::time::sleep(std::time::Duration::from_millis(wait)).await;
                    backoff_ms = (backoff_ms as f64 * self.retry_config.backoff_multiplier) as u64;
                    backoff_ms = backoff_ms.min(self.retry_config.max_backoff_ms);
                }
            }
        }
    }
}
