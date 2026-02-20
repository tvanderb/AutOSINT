use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::tools::registry::{ToolHandler, ToolHandlerContext};

pub fn handler() -> ToolHandler {
    Arc::new(|_args: Value, ctx: Arc<ToolHandlerContext>| {
        Box::pin(async move {
            let store = ctx.store.as_ref().ok_or_else(|| {
                "Investigation history not available (store not configured)".to_string()
            })?;
            let investigation_id = ctx.investigation_id.ok_or_else(|| {
                "Investigation history not available (no investigation context)".to_string()
            })?;

            let work_orders = store
                .get_work_orders_by_investigation(investigation_id)
                .await
                .map_err(|e| format!("Failed to get investigation history: {}", e))?;

            // Group work orders by cycle.
            let mut cycles: BTreeMap<i32, Vec<Value>> = BTreeMap::new();
            for wo in &work_orders {
                let entry = json!({
                    "work_order_id": wo.id.to_string(),
                    "objective": wo.objective,
                    "status": wo.status.as_db_str(),
                    "priority": format!("{:?}", wo.priority).to_lowercase(),
                    "claims_produced_count": wo.claims_produced_count,
                });
                cycles.entry(wo.cycle).or_default().push(entry);
            }

            let cycle_summaries: Vec<Value> = cycles
                .into_iter()
                .map(|(cycle, orders)| {
                    json!({
                        "cycle": cycle,
                        "work_orders": orders,
                        "count": orders.len(),
                    })
                })
                .collect();

            Ok(json!({
                "investigation_id": investigation_id.to_string(),
                "total_work_orders": work_orders.len(),
                "cycles": cycle_summaries,
            }))
        })
    })
}
