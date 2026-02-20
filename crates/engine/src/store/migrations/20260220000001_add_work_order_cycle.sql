-- Add cycle tracking and claims count to work orders
-- cycle: which investigation cycle created this work order (for get_investigation_history grouping)
-- claims_produced_count: written by Processor on completion

ALTER TABLE work_orders ADD COLUMN cycle INT NOT NULL DEFAULT 0;
ALTER TABLE work_orders ADD COLUMN claims_produced_count INT NOT NULL DEFAULT 0;
CREATE INDEX IF NOT EXISTS idx_work_orders_investigation_cycle ON work_orders(investigation_id, cycle);
