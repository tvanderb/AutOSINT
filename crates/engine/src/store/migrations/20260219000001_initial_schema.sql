-- Initial schema: investigations, work_orders, assessments
-- Per PLAN.md §4.4 PostgreSQL Schema

-- Enable pgvector extension
CREATE EXTENSION IF NOT EXISTS vector;

-- Investigation lifecycle tracking
CREATE TABLE IF NOT EXISTS investigations (
    id                      UUID PRIMARY KEY,
    prompt                  TEXT NOT NULL,
    status                  TEXT NOT NULL,   -- pending, analyst_running, processing, suspended, completed, failed
    parent_investigation_id UUID REFERENCES investigations(id),
    cycle_count             INT NOT NULL DEFAULT 0,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at            TIMESTAMPTZ,
    -- SUSPENDED state persistence (PLAN.md §4.7)
    suspended_reason        TEXT,
    suspended_at            TIMESTAMPTZ,
    resume_from             TEXT            -- 'analyst' or 'processing'
);

-- Persistent work order records
CREATE TABLE IF NOT EXISTS work_orders (
    id                  UUID PRIMARY KEY,
    investigation_id    UUID NOT NULL REFERENCES investigations(id),
    objective           TEXT NOT NULL,
    status              TEXT NOT NULL,   -- queued, processing, completed, failed
    priority            INT NOT NULL DEFAULT 1,
    referenced_entities JSONB,           -- Neo4j entity IDs for context
    source_guidance     JSONB,           -- directional hints about where to look
    processor_id        TEXT,            -- which processor handled this
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at        TIMESTAMPTZ
);

-- Analytical products
CREATE TABLE IF NOT EXISTS assessments (
    id               UUID PRIMARY KEY,
    investigation_id UUID NOT NULL REFERENCES investigations(id),
    content          JSONB NOT NULL,     -- structured assessment
    confidence       TEXT NOT NULL,      -- high / moderate / low
    entity_refs      JSONB NOT NULL DEFAULT '[]'::jsonb,  -- array of Neo4j entity IDs
    claim_refs       JSONB NOT NULL DEFAULT '[]'::jsonb,  -- array of Neo4j claim IDs
    embedding        vector(1536),       -- pgvector, for semantic search (dimensions match embedding config)
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_investigations_status ON investigations(status);
CREATE INDEX IF NOT EXISTS idx_work_orders_investigation ON work_orders(investigation_id);
CREATE INDEX IF NOT EXISTS idx_work_orders_status ON work_orders(status);
CREATE INDEX IF NOT EXISTS idx_assessments_investigation ON assessments(investigation_id);

-- Vector index for semantic search over assessments
-- Using ivfflat requires rows to exist for training; create with low list count initially.
-- Will need to be recreated or tuned once data volume grows.
CREATE INDEX IF NOT EXISTS idx_assessments_embedding ON assessments
    USING ivfflat (embedding vector_cosine_ops) WITH (lists = 10);
