use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

use crate::config::EngineConfig;
use autosint_common::ids::InvestigationId;
use autosint_common::types::{Investigation, InvestigationStatus};

use crate::analyst::{force_final_prompt, AnalystOutcome, AnalystSession};
use crate::circuit_breaker::CircuitBreakerRegistry;
use crate::embeddings::EmbeddingClient;
use crate::graph::GraphClient;
use crate::queue::QueueClient;
use crate::store::StoreClient;

/// The Orchestrator drives investigation lifecycles as a deterministic state machine.
pub struct Orchestrator {
    graph: Arc<GraphClient>,
    store: Arc<StoreClient>,
    queue: Arc<QueueClient>,
    embedding_client: Option<Arc<EmbeddingClient>>,
    config: Arc<EngineConfig>,
    fetch_base_url: String,
    tool_schemas: Arc<HashMap<String, Value>>,
    analyst_prompt: String,
    circuit_breakers: Arc<CircuitBreakerRegistry>,
}

impl Orchestrator {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        graph: Arc<GraphClient>,
        store: Arc<StoreClient>,
        queue: Arc<QueueClient>,
        embedding_client: Option<Arc<EmbeddingClient>>,
        config: Arc<EngineConfig>,
        fetch_base_url: String,
        tool_schemas: Arc<HashMap<String, Value>>,
        analyst_prompt: String,
        circuit_breakers: Arc<CircuitBreakerRegistry>,
    ) -> Self {
        Self {
            graph,
            store,
            queue,
            embedding_client,
            config,
            fetch_base_url,
            tool_schemas,
            analyst_prompt,
            circuit_breakers,
        }
    }

    /// Start a new investigation from a prompt. Returns the investigation ID.
    pub async fn start_investigation(&self, prompt: &str) -> Result<InvestigationId, String> {
        let investigation = Investigation::new(prompt.to_string());
        let id = investigation.id;

        self.store
            .create_investigation(&investigation)
            .await
            .map_err(|e| format!("Failed to create investigation: {}", e))?;

        tracing::info!(
            investigation_id = %id,
            prompt = %prompt,
            "Investigation created"
        );

        metrics::counter!("investigations.created").increment(1);

        Ok(id)
    }

    /// Run the full investigation lifecycle. Call from a spawned task.
    pub async fn run_investigation(&self, id: InvestigationId) -> Result<(), String> {
        let span = tracing::info_span!("investigation", investigation_id = %id);
        let _enter = span.enter();

        let safety = &self.config.system.safety;
        let mut empty_session_count: u32 = 0;
        let mut consecutive_all_fail_cycles: u32 = 0;

        loop {
            // Reload investigation state from DB.
            let investigation = self
                .store
                .get_investigation(id)
                .await
                .map_err(|e| format!("Failed to get investigation: {}", e))?;

            if investigation.status.is_terminal() {
                tracing::info!(
                    status = investigation.status.as_db_str(),
                    "Investigation terminal"
                );
                return Ok(());
            }

            // Circuit breaker check — suspend if a hard dependency is down.
            if let Some(circuit_name) = self.circuit_breakers.any_hard_open() {
                tracing::warn!(
                    circuit = circuit_name,
                    "Hard dependency circuit open, suspending investigation"
                );
                let resume_from = match investigation.status {
                    InvestigationStatus::Processing => "processing",
                    _ => "analyst",
                };
                self.store
                    .suspend_investigation(
                        id,
                        &format!("circuit_breaker:{}", circuit_name),
                        resume_from,
                    )
                    .await
                    .map_err(|e| format!("Failed to suspend investigation: {}", e))?;

                return Ok(());
            }

            match investigation.status {
                InvestigationStatus::Pending | InvestigationStatus::AnalystRunning => {
                    // Check max cycles.
                    let force_final =
                        investigation.cycle_count as u32 >= safety.max_cycles_per_investigation;

                    if force_final {
                        tracing::warn!(
                            cycles = investigation.cycle_count,
                            max = safety.max_cycles_per_investigation,
                            "Max cycles reached, forcing final assessment"
                        );
                    }

                    // Transition to ANALYST_RUNNING.
                    self.store
                        .update_investigation_status(
                            id,
                            &InvestigationStatus::AnalystRunning,
                            false,
                        )
                        .await
                        .map_err(|e| format!("Failed to update status: {}", e))?;

                    // Run Analyst cycle.
                    let outcome = self
                        .run_analyst_cycle(id, &investigation, force_final)
                        .await?;

                    match outcome {
                        AnalystOutcome::AssessmentProduced => {
                            self.store
                                .update_investigation_status(
                                    id,
                                    &InvestigationStatus::Completed,
                                    false,
                                )
                                .await
                                .map_err(|e| format!("Failed to mark completed: {}", e))?;

                            tracing::info!("Investigation completed with assessment");
                            metrics::counter!("investigations.completed").increment(1);
                            return Ok(());
                        }
                        AnalystOutcome::WorkOrdersCreated { count } => {
                            // Transition to PROCESSING, increment cycle.
                            self.store
                                .update_investigation_status(
                                    id,
                                    &InvestigationStatus::Processing,
                                    true,
                                )
                                .await
                                .map_err(|e| format!("Failed to mark processing: {}", e))?;

                            tracing::info!(
                                work_orders = count,
                                "Analyst created work orders, processing"
                            );
                            empty_session_count = 0;

                            // Wait for all work orders to complete.
                            self.wait_for_work_orders(id).await?;

                            // Check for all-fail cycle.
                            if self.check_all_failed_cycle(id).await? {
                                consecutive_all_fail_cycles += 1;
                                tracing::warn!(
                                    consecutive = consecutive_all_fail_cycles,
                                    "All work orders failed in this cycle"
                                );

                                if consecutive_all_fail_cycles >= safety.consecutive_all_fail_limit
                                {
                                    tracing::error!("Consecutive all-fail limit reached");
                                    self.transition_to_failed(id, &investigation).await?;
                                    return Ok(());
                                }
                            } else {
                                consecutive_all_fail_cycles = 0;
                            }

                            // Transition back to ANALYST_RUNNING for next cycle.
                            self.store
                                .update_investigation_status(
                                    id,
                                    &InvestigationStatus::AnalystRunning,
                                    false,
                                )
                                .await
                                .map_err(|e| format!("Failed to transition back: {}", e))?;
                        }
                        AnalystOutcome::EmptySession => {
                            empty_session_count += 1;
                            tracing::warn!(
                                count = empty_session_count,
                                "Empty Analyst session (no WOs, no assessment)"
                            );

                            if empty_session_count >= 2 {
                                // Force final assessment on second empty.
                                tracing::warn!("Second empty session, forcing final assessment");
                                let forced_outcome =
                                    self.run_analyst_cycle(id, &investigation, true).await?;

                                match forced_outcome {
                                    AnalystOutcome::AssessmentProduced => {
                                        self.store
                                            .update_investigation_status(
                                                id,
                                                &InvestigationStatus::Completed,
                                                false,
                                            )
                                            .await
                                            .map_err(|e| {
                                                format!("Failed to mark completed: {}", e)
                                            })?;
                                        return Ok(());
                                    }
                                    _ => {
                                        // Even forced final failed — mark as failed.
                                        self.transition_to_failed(id, &investigation).await?;
                                        return Ok(());
                                    }
                                }
                            }
                            // First empty → retry (loop continues).
                        }
                        AnalystOutcome::Failed { error } => {
                            tracing::error!(error = %error, "Analyst session failed");
                            self.transition_to_failed(id, &investigation).await?;
                            return Ok(());
                        }
                    }
                }
                InvestigationStatus::Processing => {
                    // Resuming from processing state (e.g., after restart).
                    self.wait_for_work_orders(id).await?;

                    self.store
                        .update_investigation_status(
                            id,
                            &InvestigationStatus::AnalystRunning,
                            false,
                        )
                        .await
                        .map_err(|e| format!("Failed to transition from processing: {}", e))?;
                }
                InvestigationStatus::Suspended => {
                    // Clear suspension and resume.
                    self.store
                        .clear_suspension(id)
                        .await
                        .map_err(|e| format!("Failed to clear suspension: {}", e))?;

                    let resume_status = match investigation.resume_from.as_deref() {
                        Some("processing") => InvestigationStatus::Processing,
                        _ => InvestigationStatus::AnalystRunning,
                    };

                    self.store
                        .update_investigation_status(id, &resume_status, false)
                        .await
                        .map_err(|e| format!("Failed to resume: {}", e))?;

                    tracing::info!(
                        resume_from = investigation.resume_from.as_deref().unwrap_or("analyst"),
                        "Investigation resumed from suspension"
                    );
                }
                InvestigationStatus::Completed | InvestigationStatus::Failed => {
                    return Ok(());
                }
            }
        }
    }

    /// Run a single Analyst cycle.
    async fn run_analyst_cycle(
        &self,
        id: InvestigationId,
        investigation: &Investigation,
        force_final: bool,
    ) -> Result<AnalystOutcome, String> {
        let prompt = if force_final {
            force_final_prompt(&self.analyst_prompt)
        } else {
            self.analyst_prompt.clone()
        };

        let session = AnalystSession::new(
            &self.config.system.llm.analyst,
            &self.config.system.retry.llm_api,
            &self.config.system.safety,
            Arc::clone(&self.graph),
            self.embedding_client.clone(),
            Arc::clone(&self.store),
            Arc::clone(&self.queue),
            self.fetch_base_url.clone(),
            prompt,
            &self.tool_schemas,
            self.config.system.tool_results.clone(),
            self.config.system.dedup.clone(),
            id,
            investigation.cycle_count,
        )?;

        let user_prompt = format!(
            "## Investigation\n\n{}\n\n---\nCycle: {} | Max cycles: {}",
            investigation.prompt,
            investigation.cycle_count,
            self.config.system.safety.max_cycles_per_investigation,
        );

        let result = session.run(&user_prompt).await;
        Ok(result.outcome)
    }

    /// Poll until all active work orders for an investigation are resolved.
    async fn wait_for_work_orders(&self, id: InvestigationId) -> Result<(), String> {
        let poll_interval = std::time::Duration::from_secs(5);
        let max_wait = std::time::Duration::from_secs(3600); // 1 hour max.
        let start = std::time::Instant::now();

        loop {
            let active = self
                .store
                .count_active_work_orders(id)
                .await
                .map_err(|e| format!("Failed to count active work orders: {}", e))?;

            if active == 0 {
                tracing::info!("All work orders resolved");
                return Ok(());
            }

            if start.elapsed() > max_wait {
                tracing::error!(remaining = active, "Timed out waiting for work orders");
                return Err(format!(
                    "Timed out waiting for {} work orders to complete",
                    active
                ));
            }

            tracing::debug!(active = active, "Waiting for work orders");
            metrics::gauge!("work_orders.queue_depth").set(active as f64);
            tokio::time::sleep(poll_interval).await;
        }
    }

    /// Check if all work orders in the most recent cycle failed.
    async fn check_all_failed_cycle(&self, id: InvestigationId) -> Result<bool, String> {
        let work_orders = self
            .store
            .get_work_orders_by_investigation(id)
            .await
            .map_err(|e| format!("Failed to get work orders: {}", e))?;

        if work_orders.is_empty() {
            return Ok(false);
        }

        // Find the most recent cycle.
        let max_cycle = work_orders.iter().map(|wo| wo.cycle).max().unwrap_or(0);

        // Check if all work orders in that cycle failed.
        let cycle_orders: Vec<_> = work_orders
            .iter()
            .filter(|wo| wo.cycle == max_cycle)
            .collect();

        if cycle_orders.is_empty() {
            return Ok(false);
        }

        let all_failed = cycle_orders
            .iter()
            .all(|wo| wo.status == autosint_common::types::WorkOrderStatus::Failed);

        Ok(all_failed)
    }

    /// Transition to FAILED state — run one final Analyst session with failure prompt,
    /// then mark the investigation as failed.
    async fn transition_to_failed(
        &self,
        id: InvestigationId,
        investigation: &Investigation,
    ) -> Result<(), String> {
        tracing::warn!(investigation_id = %id, "Investigation transitioning to FAILED");

        // Attempt one final assessment with failure context.
        let failure_prompt = format!(
            "{}\n\n---\n\n\
            **CRITICAL: This investigation has FAILED.** Produce the best assessment you can with \
            the available information. Clearly note all gaps, limitations, and failures encountered. \
            A partial assessment documenting what is known and what is not is valuable.",
            self.analyst_prompt
        );

        let final_session = AnalystSession::new(
            &self.config.system.llm.analyst,
            &self.config.system.retry.llm_api,
            &self.config.system.safety,
            Arc::clone(&self.graph),
            self.embedding_client.clone(),
            Arc::clone(&self.store),
            Arc::clone(&self.queue),
            self.fetch_base_url.clone(),
            failure_prompt,
            &self.tool_schemas,
            self.config.system.tool_results.clone(),
            self.config.system.dedup.clone(),
            id,
            investigation.cycle_count,
        );

        if let Ok(session) = final_session {
            let user_prompt = format!(
                "## Investigation (FAILURE MODE)\n\n{}\n\n\
                This investigation has failed. Produce a partial assessment.",
                investigation.prompt
            );
            let _ = session.run(&user_prompt).await;
        }

        self.store
            .update_investigation_status(id, &InvestigationStatus::Failed, false)
            .await
            .map_err(|e| format!("Failed to mark investigation as failed: {}", e))?;

        metrics::counter!("investigations.failed").increment(1);
        Ok(())
    }

    /// On startup, recover non-terminal investigations.
    pub async fn recover_on_startup(&self) -> Result<(), String> {
        let investigations = self
            .store
            .get_non_terminal_investigations()
            .await
            .map_err(|e| format!("Failed to query non-terminal investigations: {}", e))?;

        if investigations.is_empty() {
            tracing::info!("No investigations to recover on startup");
            return Ok(());
        }

        tracing::info!(
            count = investigations.len(),
            "Found non-terminal investigations on startup"
        );

        for investigation in investigations {
            match investigation.status {
                InvestigationStatus::Suspended => {
                    tracing::info!(
                        id = %investigation.id,
                        reason = investigation.suspended_reason.as_deref().unwrap_or("unknown"),
                        "Resuming suspended investigation"
                    );
                }
                InvestigationStatus::AnalystRunning | InvestigationStatus::Processing => {
                    // Crashed mid-operation — treat as suspended.
                    tracing::warn!(
                        id = %investigation.id,
                        status = investigation.status.as_db_str(),
                        "Investigation was active at shutdown, treating as suspended"
                    );
                    let resume_from = if investigation.status == InvestigationStatus::Processing {
                        "processing"
                    } else {
                        "analyst"
                    };
                    if let Err(e) = self
                        .store
                        .suspend_investigation(investigation.id, "engine_restart", resume_from)
                        .await
                    {
                        tracing::error!(error = %e, "Failed to suspend investigation for recovery");
                        continue;
                    }
                }
                InvestigationStatus::Pending => {
                    tracing::info!(
                        id = %investigation.id,
                        "Resuming pending investigation"
                    );
                }
                _ => continue,
            }

            // Spawn investigation lifecycle as a background task.
            let orchestrator_graph = Arc::clone(&self.graph);
            let orchestrator_store = Arc::clone(&self.store);
            let orchestrator_queue = Arc::clone(&self.queue);
            let orchestrator_emb = self.embedding_client.clone();
            let orchestrator_config = Arc::clone(&self.config);
            let orchestrator_fetch = self.fetch_base_url.clone();
            let orchestrator_schemas = Arc::clone(&self.tool_schemas);
            let orchestrator_prompt = self.analyst_prompt.clone();
            let orchestrator_cbs = Arc::clone(&self.circuit_breakers);
            let inv_id = investigation.id;

            tokio::spawn(async move {
                let orch = Orchestrator::new(
                    orchestrator_graph,
                    orchestrator_store,
                    orchestrator_queue,
                    orchestrator_emb,
                    orchestrator_config,
                    orchestrator_fetch,
                    orchestrator_schemas,
                    orchestrator_prompt,
                    orchestrator_cbs,
                );
                if let Err(e) = orch.run_investigation(inv_id).await {
                    tracing::error!(
                        investigation_id = %inv_id,
                        error = %e,
                        "Recovery investigation failed"
                    );
                }
            });
        }

        Ok(())
    }
}
