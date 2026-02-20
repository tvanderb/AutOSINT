use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde::Deserialize;
use serde_json::{json, Value};

use autosint_common::types::{SourceGuidance, WorkOrder, WorkOrderPriority};
use autosint_common::EntityId;

use crate::tools::registry::{ToolHandler, ToolHandlerContext};

#[derive(Deserialize)]
struct Args {
    objective: String,
    #[serde(default)]
    referenced_entities: Vec<String>,
    #[serde(default)]
    source_guidance: Option<SourceGuidanceArgs>,
    #[serde(default)]
    priority: Option<String>,
}

#[derive(Deserialize)]
struct SourceGuidanceArgs {
    #[serde(default)]
    prefer: Vec<String>,
}

pub fn handler() -> ToolHandler {
    Arc::new(|args: Value, ctx: Arc<ToolHandlerContext>| {
        Box::pin(async move {
            let args: Args =
                serde_json::from_value(args).map_err(|e| format!("Invalid arguments: {}", e))?;

            // Verify analyst context is available.
            let store = ctx.store.as_ref().ok_or_else(|| {
                "Work order creation not available (store not configured)".to_string()
            })?;
            let queue = ctx.queue.as_ref().ok_or_else(|| {
                "Work order creation not available (queue not configured)".to_string()
            })?;
            let investigation_id = ctx.investigation_id.ok_or_else(|| {
                "Work order creation not available (no investigation context)".to_string()
            })?;
            let cycle = ctx.investigation_cycle.unwrap_or(0);

            // Enforce work order limit per cycle.
            if let Some(max) = ctx.max_work_orders_per_cycle {
                let current = ctx
                    .session_counters
                    .work_orders_created
                    .load(Ordering::Relaxed);
                if current >= max {
                    return Err(format!(
                        "Work order limit reached ({}/{}). Cannot create more work orders this cycle.",
                        current, max
                    ));
                }
            }

            // Parse priority.
            let priority = match args.priority.as_deref() {
                Some("high") => WorkOrderPriority::High,
                Some("normal") | None => WorkOrderPriority::Normal,
                Some("low") => WorkOrderPriority::Low,
                Some(other) => {
                    return Err(format!(
                        "Invalid priority: '{}'. Use 'high', 'normal', or 'low'.",
                        other
                    ))
                }
            };

            // Parse referenced entity IDs.
            let referenced_entities: Vec<EntityId> = args
                .referenced_entities
                .iter()
                .map(|s| {
                    s.parse::<uuid::Uuid>()
                        .map(EntityId::from_uuid)
                        .map_err(|e| format!("Invalid entity ID '{}': {}", s, e))
                })
                .collect::<Result<_, _>>()?;

            // Build source guidance.
            let source_guidance = args.source_guidance.map(|sg| SourceGuidance {
                prefer: sg.prefer,
                extra: serde_json::Map::new(),
            });

            // Create work order.
            let mut wo = WorkOrder::new(investigation_id, args.objective.clone(), priority);
            wo.referenced_entities = referenced_entities;
            wo.source_guidance = source_guidance;
            wo.cycle = cycle;

            // Persist to PostgreSQL.
            let created = store
                .create_work_order(&wo)
                .await
                .map_err(|e| format!("Failed to create work order: {}", e))?;

            // Enqueue to Redis.
            let msg = autosint_common::types::WorkOrderMessage::from(&created);
            queue
                .enqueue(&msg, &created.priority)
                .await
                .map_err(|e| format!("Failed to enqueue work order: {}", e))?;

            // Increment counter.
            ctx.session_counters
                .work_orders_created
                .fetch_add(1, Ordering::Relaxed);

            tracing::info!(
                work_order_id = %created.id,
                investigation_id = %investigation_id,
                cycle = cycle,
                objective = %args.objective,
                "Work order created and enqueued"
            );

            Ok(json!({
                "work_order_id": created.id.to_string(),
                "objective": created.objective,
                "priority": format!("{:?}", created.priority).to_lowercase(),
                "cycle": created.cycle,
                "message": "Work order created and dispatched to Processors."
            }))
        })
    })
}
