use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;
use tokio::sync::watch;
use tokio::task::JoinHandle;

use autosint_common::config::{
    DedupConfig, LlmRoleConfig, RetryConfig, SafetyLimits, ToolResultLimits,
};
use autosint_common::types::WorkOrderStatus;

use crate::embeddings::EmbeddingClient;
use crate::graph::GraphClient;
use crate::queue::QueueClient;
use crate::store::StoreClient;

use super::ProcessorSession;

/// Configuration for the Processor pool.
pub struct ProcessorPoolConfig {
    pub pool_size: u32,
    pub heartbeat_ttl_seconds: u64,
    /// Heartbeat refresh interval (typically ttl / 3).
    pub heartbeat_interval_seconds: u64,
}

/// Pool of Processor worker tasks that consume work orders from Redis.
pub struct ProcessorPool {
    workers: Vec<JoinHandle<()>>,
    shutdown_tx: watch::Sender<bool>,
}

impl ProcessorPool {
    /// Start the processor pool with the given number of workers.
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        config: ProcessorPoolConfig,
        llm_config: LlmRoleConfig,
        retry_config: RetryConfig,
        graph: Arc<GraphClient>,
        embedding_client: Option<Arc<EmbeddingClient>>,
        store: Arc<StoreClient>,
        queue: Arc<QueueClient>,
        fetch_base_url: String,
        system_prompt: String,
        tool_schemas: Arc<HashMap<String, Value>>,
        tool_result_limits: ToolResultLimits,
        dedup_config: DedupConfig,
        safety_limits: SafetyLimits,
    ) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        let llm_config = Arc::new(llm_config);
        let retry_config = Arc::new(retry_config);
        let safety_limits = Arc::new(safety_limits);

        let mut workers = Vec::with_capacity(config.pool_size as usize);

        for i in 0..config.pool_size {
            let consumer_name = format!("processor-{}", i);
            let worker = processor_worker_loop(
                consumer_name,
                shutdown_rx.clone(),
                Arc::clone(&llm_config),
                Arc::clone(&retry_config),
                Arc::clone(&graph),
                embedding_client.clone(),
                Arc::clone(&store),
                Arc::clone(&queue),
                fetch_base_url.clone(),
                system_prompt.clone(),
                Arc::clone(&tool_schemas),
                tool_result_limits.clone(),
                dedup_config.clone(),
                Arc::clone(&safety_limits),
                config.heartbeat_ttl_seconds,
                config.heartbeat_interval_seconds,
            );

            workers.push(tokio::spawn(worker));
        }

        tracing::info!(pool_size = config.pool_size, "Processor pool started");

        Self {
            workers,
            shutdown_tx,
        }
    }

    /// Signal all workers to shut down gracefully.
    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.send(true);
        tracing::info!("Processor pool shutdown signaled");
    }

    /// Wait for all workers to finish.
    pub async fn join(self) {
        for handle in self.workers {
            let _ = handle.await;
        }
    }
}

/// Main loop for a single Processor worker.
#[allow(clippy::too_many_arguments)]
async fn processor_worker_loop(
    consumer_name: String,
    shutdown_rx: watch::Receiver<bool>,
    llm_config: Arc<LlmRoleConfig>,
    retry_config: Arc<RetryConfig>,
    graph: Arc<GraphClient>,
    embedding_client: Option<Arc<EmbeddingClient>>,
    store: Arc<StoreClient>,
    queue: Arc<QueueClient>,
    fetch_base_url: String,
    system_prompt: String,
    tool_schemas: Arc<HashMap<String, Value>>,
    tool_result_limits: ToolResultLimits,
    dedup_config: DedupConfig,
    safety_limits: Arc<SafetyLimits>,
    heartbeat_ttl: u64,
    heartbeat_interval: u64,
) {
    tracing::info!(consumer = %consumer_name, "Processor worker started");

    // Reclaim stale messages from dead consumers periodically.
    // min_idle = 2× heartbeat TTL — if a consumer hasn't heartbeated in that long, it's dead.
    let reclaim_min_idle_ms = (heartbeat_ttl * 2) * 1000;
    let reclaim_interval = std::time::Duration::from_secs(heartbeat_ttl);
    let mut last_reclaim = std::time::Instant::now();

    loop {
        // Check shutdown.
        if *shutdown_rx.borrow() {
            tracing::info!(consumer = %consumer_name, "Processor worker shutting down");
            break;
        }

        // Periodically reclaim stale messages from dead consumers.
        // XCLAIM transfers ownership to this consumer; the next dequeue(ID=0) picks them up.
        if last_reclaim.elapsed() >= reclaim_interval {
            match queue.reclaim_pending(&consumer_name, reclaim_min_idle_ms).await {
                Ok(reclaimed) if !reclaimed.is_empty() => {
                    tracing::info!(
                        consumer = %consumer_name,
                        count = reclaimed.len(),
                        "Reclaimed stale messages from dead consumers"
                    );
                }
                Err(e) => {
                    tracing::warn!(consumer = %consumer_name, error = %e, "Reclaim check failed");
                }
                _ => {}
            }
            last_reclaim = std::time::Instant::now();
        }

        // Try to dequeue a work order (block for 5s to allow periodic shutdown checks).
        let dequeue_result = queue.dequeue(&consumer_name, Some(5000)).await;

        let (stream_name, entry_id, msg) = match dequeue_result {
            Ok(Some(item)) => item,
            Ok(None) => continue,
            Err(e) => {
                tracing::error!(consumer = %consumer_name, error = %e, "Failed to dequeue");
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                continue;
            }
        };

        let work_order_id = msg.work_order_id;
        tracing::info!(
            consumer = %consumer_name,
            work_order_id = %work_order_id,
            objective = %msg.objective,
            "Processing work order"
        );

        metrics::gauge!("processor.pool.active").increment(1.0);

        // Start heartbeat task.
        let (hb_cancel_tx, hb_cancel_rx) = tokio::sync::oneshot::channel::<()>();
        let hb_queue = Arc::clone(&queue);
        let hb_name = consumer_name.clone();
        let hb_handle = tokio::spawn(heartbeat_task(
            hb_queue,
            hb_name,
            heartbeat_ttl,
            heartbeat_interval,
            hb_cancel_rx,
        ));

        // Update work order status to Processing.
        if let Err(e) = store
            .update_work_order_status(
                work_order_id,
                &WorkOrderStatus::Processing,
                Some(&consumer_name),
                None,
            )
            .await
        {
            tracing::error!(error = %e, "Failed to update work order status to Processing");
        }

        // Create and run Processor session.
        let session_result = match ProcessorSession::new(
            &llm_config,
            &retry_config,
            &safety_limits,
            Arc::clone(&graph),
            embedding_client.clone(),
            fetch_base_url.clone(),
            system_prompt.clone(),
            &tool_schemas,
            tool_result_limits.clone(),
            dedup_config.clone(),
        ) {
            Ok(session) => {
                session
                    .run(
                        &msg.objective,
                        &msg.referenced_entities,
                        msg.source_guidance.as_ref(),
                    )
                    .await
            }
            Err(e) => {
                tracing::error!(error = %e, "Failed to create Processor session");
                if let Err(e2) = store
                    .update_work_order_status(work_order_id, &WorkOrderStatus::Failed, None, None)
                    .await
                {
                    tracing::error!(error = %e2, "Failed to update work order status to Failed");
                }

                let _ = hb_cancel_tx.send(());
                let _ = hb_handle.await;
                let _ = queue.ack(&stream_name, &entry_id).await;

                metrics::gauge!("processor.pool.active").decrement(1.0);
                continue;
            }
        };

        // Cancel heartbeat task.
        let _ = hb_cancel_tx.send(());
        let _ = hb_handle.await;

        // Determine final status and claims count.
        // MaxTurnsReached and MalformedToolCallLimit are treated as Completed — partial
        // progress (entities, claims written to the graph) is still valid and non-transactional.
        // Only actual errors (Failed) are terminal failures.
        let claims_count = session_result.claims_created as i32;
        let final_status = match &session_result.outcome {
            crate::llm::session::SessionResult::Completed { .. }
            | crate::llm::session::SessionResult::MaxTurnsReached { .. }
            | crate::llm::session::SessionResult::MalformedToolCallLimit { .. } => {
                WorkOrderStatus::Completed
            }
            _ => WorkOrderStatus::Failed,
        };

        // Update work order status in PG.
        if let Err(e) = store
            .update_work_order_status(work_order_id, &final_status, None, Some(claims_count))
            .await
        {
            tracing::error!(error = %e, "Failed to update work order final status");
        }

        // ACK the message in Redis.
        if let Err(e) = queue.ack(&stream_name, &entry_id).await {
            tracing::error!(error = %e, "Failed to ACK work order message");
        }

        tracing::info!(
            consumer = %consumer_name,
            work_order_id = %work_order_id,
            status = ?final_status,
            claims = claims_count,
            "Work order processing complete"
        );

        metrics::gauge!("processor.pool.active").decrement(1.0);
    }
}

/// Independent heartbeat task — runs until cancelled.
async fn heartbeat_task(
    queue: Arc<QueueClient>,
    processor_id: String,
    ttl_seconds: u64,
    interval_seconds: u64,
    cancel: tokio::sync::oneshot::Receiver<()>,
) {
    let mut cancel = cancel;
    let interval = std::time::Duration::from_secs(interval_seconds);

    // Initial heartbeat.
    let _ = queue.heartbeat(&processor_id, ttl_seconds).await;

    loop {
        tokio::select! {
            _ = tokio::time::sleep(interval) => {
                if let Err(e) = queue.heartbeat(&processor_id, ttl_seconds).await {
                    tracing::warn!(
                        processor_id = %processor_id,
                        error = %e,
                        "Failed to refresh heartbeat"
                    );
                }
            }
            _ = &mut cancel => {
                break;
            }
        }
    }
}
