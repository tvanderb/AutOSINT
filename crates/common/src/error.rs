use thiserror::Error;

/// Top-level error type for AutOSINT operations.
#[derive(Debug, Error)]
pub enum AutOsintError {
    // --- Hard dependency errors (system cannot function) ---
    #[error("Neo4j error: {0}")]
    Neo4j(String),

    #[error("PostgreSQL error: {0}")]
    Postgres(String),

    #[error("Redis error: {0}")]
    Redis(String),

    #[error("LLM API error: {0}")]
    LlmApi(String),

    // --- Soft dependency errors (system degrades) ---
    #[error("Fetch service error: {0}")]
    Fetch(String),

    #[error("Geo service error: {0}")]
    Geo(String),

    #[error("Scribe service error: {0}")]
    Scribe(String),

    // --- Operational errors ---
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Circuit breaker open for {0}")]
    CircuitOpen(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("{0}")]
    Internal(String),
}

impl AutOsintError {
    /// Whether this error is from a hard dependency (warrants investigation suspension).
    pub fn is_hard_dependency(&self) -> bool {
        matches!(
            self,
            Self::Neo4j(_) | Self::Postgres(_) | Self::Redis(_) | Self::LlmApi(_)
        )
    }

    /// Whether this error is from a soft dependency (LLM can adapt).
    pub fn is_soft_dependency(&self) -> bool {
        matches!(self, Self::Fetch(_) | Self::Geo(_) | Self::Scribe(_))
    }
}

/// Result type alias for AutOSINT operations.
pub type Result<T> = std::result::Result<T, AutOsintError>;
