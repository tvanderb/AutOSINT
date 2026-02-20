use std::sync::Arc;
use std::time::Duration;

use neo4rs::query;
use tokio::task::JoinHandle;

use crate::graph::GraphClient;

use super::EmbeddingClient;

/// Spawn a background task that periodically finds nodes/relationships with
/// `embedding_pending = true`, computes their embeddings, and updates them.
pub fn spawn_backfill_task(
    graph: Arc<GraphClient>,
    embedding_client: Arc<EmbeddingClient>,
    interval_minutes: u32,
    batch_size: u32,
) -> JoinHandle<()> {
    let interval = Duration::from_secs(interval_minutes as u64 * 60);

    tokio::spawn(async move {
        tracing::info!(
            interval_minutes,
            batch_size,
            "Embedding backfill task started"
        );

        loop {
            tokio::time::sleep(interval).await;

            if let Err(e) = run_backfill_cycle(&graph, &embedding_client, batch_size).await {
                tracing::error!(error = %e, "Embedding backfill cycle failed");
            }
        }
    })
}

async fn run_backfill_cycle(
    graph: &GraphClient,
    embedding_client: &EmbeddingClient,
    batch_size: u32,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Backfill entities.
    backfill_entities(graph, embedding_client, batch_size).await?;

    // Backfill claims.
    backfill_claims(graph, embedding_client, batch_size).await?;

    // Backfill relationships.
    backfill_relationships(graph, embedding_client, batch_size).await?;

    Ok(())
}

async fn backfill_entities(
    graph: &GraphClient,
    embedding_client: &EmbeddingClient,
    batch_size: u32,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let q = query(
        "MATCH (e:Entity {embedding_pending: true}) \
         RETURN e.id AS id, e.canonical_name AS name, e.summary AS summary \
         LIMIT $limit",
    )
    .param("limit", batch_size as i64);

    let mut result = graph.inner().execute(q).await?;

    let mut ids = Vec::new();
    let mut texts = Vec::new();

    while let Ok(Some(row)) = result.next().await {
        let id: String = row.get("id")?;
        let name: String = row.get("name")?;
        let summary: Option<String> = row.get("summary").ok();
        let text = crate::graph::conversions::embedding_text_for_entity(&name, summary.as_deref());
        ids.push(id);
        texts.push(text);
    }

    if texts.is_empty() {
        return Ok(());
    }

    let count = texts.len();
    tracing::info!(count, "Backfilling entity embeddings");

    let embeddings = embedding_client.embed_batch(&texts).await?;

    for (id, embedding) in ids.iter().zip(embeddings.iter()) {
        let emb_f64: Vec<f64> = embedding.iter().map(|&f| f as f64).collect();
        let update = query(
            "MATCH (e:Entity {id: $id}) \
             SET e.embedding = $embedding, e.embedding_pending = false",
        )
        .param("id", id.as_str())
        .param("embedding", emb_f64);

        graph.inner().run(update).await?;
    }

    metrics::counter!("embedding.backfill.processed").increment(count as u64);
    Ok(())
}

async fn backfill_claims(
    graph: &GraphClient,
    embedding_client: &EmbeddingClient,
    batch_size: u32,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let q = query(
        "MATCH (c:Claim {embedding_pending: true}) \
         RETURN c.id AS id, c.content AS content \
         LIMIT $limit",
    )
    .param("limit", batch_size as i64);

    let mut result = graph.inner().execute(q).await?;

    let mut ids = Vec::new();
    let mut texts = Vec::new();

    while let Ok(Some(row)) = result.next().await {
        let id: String = row.get("id")?;
        let content: String = row.get("content")?;
        ids.push(id);
        texts.push(content);
    }

    if texts.is_empty() {
        return Ok(());
    }

    let count = texts.len();
    tracing::info!(count, "Backfilling claim embeddings");

    let embeddings = embedding_client.embed_batch(&texts).await?;

    for (id, embedding) in ids.iter().zip(embeddings.iter()) {
        let emb_f64: Vec<f64> = embedding.iter().map(|&f| f as f64).collect();
        let update = query(
            "MATCH (c:Claim {id: $id}) \
             SET c.embedding = $embedding, c.embedding_pending = false",
        )
        .param("id", id.as_str())
        .param("embedding", emb_f64);

        graph.inner().run(update).await?;
    }

    metrics::counter!("embedding.backfill.processed").increment(count as u64);
    Ok(())
}

async fn backfill_relationships(
    graph: &GraphClient,
    embedding_client: &EmbeddingClient,
    batch_size: u32,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let q = query(
        "MATCH ()-[r:RELATES_TO {embedding_pending: true}]-() \
         RETURN r.id AS id, r.description AS description \
         LIMIT $limit",
    )
    .param("limit", batch_size as i64);

    let mut result = graph.inner().execute(q).await?;

    let mut ids = Vec::new();
    let mut texts = Vec::new();

    while let Ok(Some(row)) = result.next().await {
        let id: String = row.get("id")?;
        let description: String = row.get("description")?;
        ids.push(id);
        texts.push(description);
    }

    if texts.is_empty() {
        return Ok(());
    }

    let count = texts.len();
    tracing::info!(count, "Backfilling relationship embeddings");

    let embeddings = embedding_client.embed_batch(&texts).await?;

    for (id, embedding) in ids.iter().zip(embeddings.iter()) {
        let emb_f64: Vec<f64> = embedding.iter().map(|&f| f as f64).collect();
        let update = query(
            "MATCH ()-[r:RELATES_TO {id: $id}]-() \
             SET r.embedding = $embedding, r.embedding_pending = false",
        )
        .param("id", id.as_str())
        .param("embedding", emb_f64);

        graph.inner().run(update).await?;
    }

    metrics::counter!("embedding.backfill.processed").increment(count as u64);
    Ok(())
}
