mod claims;
pub(crate) mod conversions;
pub mod dedup;
mod entities;
mod relationships;
mod search;

// Re-exports for use by other engine modules.
#[allow(unused_imports)]
pub use dedup::{DedupResult, DedupStage};
#[allow(unused_imports)]
pub use entities::EntityUpdate;
#[allow(unused_imports)]
pub use relationships::{RelationshipUpdate, TraversalDirection, TraversalParams};
#[allow(unused_imports)]
pub use search::{
    ClaimSearchParams, EntitySearchParams, RelationshipSearchParams, SearchMode, SearchResult,
};

use neo4rs::{query, Graph};

/// Neo4j client wrapping a connection pool.
pub struct GraphClient {
    graph: Graph,
}

impl GraphClient {
    /// Connect to Neo4j and return a client with a connection pool.
    pub async fn connect(uri: &str, user: &str, password: &str) -> Result<Self, GraphError> {
        tracing::info!(uri = uri, "Connecting to Neo4j");

        let graph = Graph::new(uri, user, password)
            .await
            .map_err(|e| GraphError::Connection(e.to_string()))?;

        let client = Self { graph };
        client.health_check().await?;
        tracing::info!("Neo4j connection established");

        Ok(client)
    }

    /// Verify the connection is alive.
    pub async fn health_check(&self) -> Result<(), GraphError> {
        self.graph
            .run(query("RETURN 1"))
            .await
            .map_err(|e| GraphError::Query(e.to_string()))?;
        Ok(())
    }

    /// Initialize schema: create indexes and constraints.
    /// Safe to run on every startup (CREATE ... IF NOT EXISTS).
    pub async fn initialize_schema(&self) -> Result<(), GraphError> {
        tracing::info!("Initializing Neo4j schema");

        let schema_statements = [
            // Entity constraints
            "CREATE CONSTRAINT entity_id_unique IF NOT EXISTS FOR (e:Entity) REQUIRE e.id IS UNIQUE",
            // Claim constraints
            "CREATE CONSTRAINT claim_id_unique IF NOT EXISTS FOR (c:Claim) REQUIRE c.id IS UNIQUE",
            // Entity indexes
            "CREATE INDEX entity_kind_idx IF NOT EXISTS FOR (e:Entity) ON (e.kind)",
            "CREATE INDEX entity_last_updated_idx IF NOT EXISTS FOR (e:Entity) ON (e.last_updated)",
            // Claim indexes
            "CREATE INDEX claim_published_idx IF NOT EXISTS FOR (c:Claim) ON (c.published_timestamp)",
            "CREATE INDEX claim_ingested_idx IF NOT EXISTS FOR (c:Claim) ON (c.ingested_timestamp)",
            // Full-text indexes (composite syntax)
            "CREATE FULLTEXT INDEX entity_name_fulltext IF NOT EXISTS FOR (e:Entity) ON EACH [e.canonical_name, e.aliases_text]",
            "CREATE FULLTEXT INDEX claim_content_fulltext IF NOT EXISTS FOR (c:Claim) ON EACH [c.content]",
            "CREATE FULLTEXT INDEX relationship_desc_fulltext IF NOT EXISTS FOR ()-[r:RELATES_TO]-() ON EACH [r.description]",
        ];

        for stmt in &schema_statements {
            if let Err(e) = self.graph.run(query(stmt)).await {
                tracing::warn!(
                    statement = *stmt,
                    error = %e,
                    "Failed to create schema element (may already exist in different form)"
                );
            }
        }

        // Vector indexes — Neo4j 5.x uses CREATE VECTOR INDEX syntax.
        let vector_indexes = [
            "CREATE VECTOR INDEX entity_embedding IF NOT EXISTS FOR (e:Entity) ON (e.embedding) OPTIONS {indexConfig: {`vector.dimensions`: 1536, `vector.similarity_function`: 'cosine'}}",
            "CREATE VECTOR INDEX claim_embedding IF NOT EXISTS FOR (c:Claim) ON (c.embedding) OPTIONS {indexConfig: {`vector.dimensions`: 1536, `vector.similarity_function`: 'cosine'}}",
            "CREATE VECTOR INDEX relates_to_embedding IF NOT EXISTS FOR ()-[r:RELATES_TO]-() ON (r.embedding) OPTIONS {indexConfig: {`vector.dimensions`: 1536, `vector.similarity_function`: 'cosine'}}",
        ];

        for stmt in &vector_indexes {
            if let Err(e) = self.graph.run(query(stmt)).await {
                let err_str = e.to_string();
                if err_str.contains("already exists") || err_str.contains("EquivalentSchema") {
                    tracing::debug!(statement = *stmt, "Vector index already exists, skipping");
                } else {
                    tracing::warn!(
                        statement = *stmt,
                        error = %e,
                        "Failed to create vector index"
                    );
                }
            }
        }

        tracing::info!("Neo4j schema initialization complete");
        Ok(())
    }

    /// Get a reference to the underlying neo4rs Graph for direct queries.
    #[allow(dead_code)]
    pub fn inner(&self) -> &Graph {
        &self.graph
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GraphError {
    #[error("Neo4j connection error: {0}")]
    Connection(String),

    #[error("Neo4j query error: {0}")]
    Query(String),

    #[error("Not found: {0}")]
    NotFound(String),
}

impl From<GraphError> for autosint_common::AutOsintError {
    fn from(e: GraphError) -> Self {
        match &e {
            GraphError::NotFound(msg) => autosint_common::AutOsintError::NotFound(msg.clone()),
            _ => autosint_common::AutOsintError::Neo4j(e.to_string()),
        }
    }
}
