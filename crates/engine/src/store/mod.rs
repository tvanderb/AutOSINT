mod assessments;
mod investigations;
mod work_orders;

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

/// PostgreSQL client for the Assessment Store and Orchestrator State.
pub struct StoreClient {
    pool: PgPool,
}

impl StoreClient {
    /// Connect to PostgreSQL and return a client with a connection pool.
    pub async fn connect(database_url: &str, max_connections: u32) -> Result<Self, StoreError> {
        tracing::info!("Connecting to PostgreSQL");

        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .connect(database_url)
            .await
            .map_err(|e| StoreError::Connection(e.to_string()))?;

        let client = Self { pool };
        client.health_check().await?;
        tracing::info!("PostgreSQL connection established");

        Ok(client)
    }

    /// Verify the connection is alive.
    pub async fn health_check(&self) -> Result<(), StoreError> {
        sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .map_err(|e| StoreError::Query(e.to_string()))?;
        Ok(())
    }

    /// Run database migrations.
    pub async fn migrate(&self) -> Result<(), StoreError> {
        tracing::info!("Running PostgreSQL migrations");

        sqlx::migrate!("src/store/migrations")
            .run(&self.pool)
            .await
            .map_err(|e| StoreError::Migration(e.to_string()))?;

        tracing::info!("PostgreSQL migrations complete");
        Ok(())
    }

    /// Get a reference to the underlying connection pool.
    #[allow(dead_code)]
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("PostgreSQL connection error: {0}")]
    Connection(String),

    #[error("PostgreSQL query error: {0}")]
    Query(String),

    #[error("PostgreSQL migration error: {0}")]
    Migration(String),

    #[error("Not found: {0}")]
    NotFound(String),
}

impl From<StoreError> for autosint_common::AutOsintError {
    fn from(e: StoreError) -> Self {
        autosint_common::AutOsintError::Postgres(e.to_string())
    }
}
