-- Replace ivfflat index with HNSW for assessments embedding search.
-- ivfflat with lists=10 and default probes=1 misses rows at low table cardinality (cold start).
-- HNSW has no cold-start issue and works correctly at any table size.
DROP INDEX IF EXISTS idx_assessments_embedding;
CREATE INDEX idx_assessments_embedding ON assessments USING hnsw (embedding vector_cosine_ops);
