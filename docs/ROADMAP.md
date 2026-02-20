# AutOSINT — Engineering Roadmap

> Checklist-based implementation plan. Each milestone produces a demonstrable capability. Each step references the relevant section of [PLAN.md](./PLAN.md) — review the referenced section before starting the step. (NOT A REPLACEMENT FOR PER-SESSION PLANNING- THIS IS TO KEEP WORK ORIENTED TOWARDS GOALS WITH ATOMIC UNITS TO CHECK OFF AS PROGRESS IS MADE)
>
> **Observability and error handling are woven through every milestone, not bolted on at the end.** Each milestone includes the logging, metrics, and resilience work appropriate to what's being built.
>
> **Prompt engineering is continuous, not a milestone.** Prompts are written when each LLM role is first built and iterated indefinitely. They live in `config/prompts/` and are loaded at runtime.

---

## M1: Foundation

**Output:** The project exists, compiles, connects to its databases, and we can develop in it.

Everything else depends on this. No intelligence logic — pure infrastructure.

### Workspace & Project Structure

> Review: [PLAN.md §10 Tech Stack — Project Structure](./PLAN.md#project-structure)

- [x] Cargo workspace root (`Cargo.toml` with workspace members)
- [x] `crates/common/` — `autosint-common` crate skeleton
- [x] `crates/engine/` — `autosint-engine` crate skeleton with internal module structure (`orchestrator/`, `analyst/`, `processor/`, `tools/`, `llm/`, `graph/`, `store/`, `queue/`, `config/`)
- [x] `crates/fetch/` — `autosint-fetch` crate skeleton
- [x] `crates/geo/` — `autosint-geo` crate skeleton
- [x] `services/fetch-browser/` — Node.js project skeleton (package.json, tsconfig)
- [x] `services/scribe/` — Python project skeleton (pyproject.toml or requirements.txt)
- [x] `.gitignore` covering Rust, Node.js, Python, Docker, IDE files, `.claude_context`
- [x] Workspace compiles clean (`cargo build` succeeds)

### Shared Types (`autosint-common`)

> Review: [PLAN.md §4.3 Storage Primitives](./PLAN.md#43-storage-primitives), [§4.4 Database Schemas](./PLAN.md#44-database-schemas), [§9 External Module Pattern](./PLAN.md#9-external-module-pattern)

- [x] Entity type (ID, canonical_name, aliases, kind, summary, is_stub, last_updated, freeform properties)
- [x] Claim type (ID, content, published_timestamp, ingested_timestamp, raw_source_link, attribution_depth)
- [x] Relationship type (ID, description, weight, confidence, bidirectional, timestamp)
- [x] Assessment type (ID, investigation_id, content, confidence, entity_refs, claim_refs)
- [x] Investigation type (ID, prompt, status enum, parent_investigation_id, cycle_count, timestamps)
- [x] Work order type (ID, investigation_id, objective, status enum, priority, referenced_entities, source_guidance)
- [x] API contract types for Fetch (request/response schemas for `/fetch`, `/browse`, `/sources`)
- [x] API contract types for Geo (request/response schemas for `/context`, `/spatial/*`, `/terrain`, etc.)
- [x] API contract types for Scribe (request/response schemas for `/transcribe`)
- [x] Common error types
- [x] Common identifier types (UUIDs, entity refs)

### Config System

> Review: [PLAN.md §2 Core Design Philosophy — All Numeric Parameters are Runtime-Configurable](./PLAN.md#all-numeric-parameters-are-runtime-configurable), [§4.9 Tool Layer — Tool Definitions in Config](./PLAN.md#tool-definitions-in-config)

- [x] `config/` directory structure: `config/prompts/`, `config/tools/analyst/`, `config/tools/processor/`, `config/system.toml`
- [x] Config loading module in Engine (`crates/engine/src/config/`)
- [x] `system.toml` with all numeric parameters: safety limits, concurrency, timeouts, retry config, embedding config, LLM provider/model config
- [x] Config structs in `autosint-common` (deserialized via serde from TOML)
- [x] Tool schema loading from JSON files (deferred: actual tool schemas written in M3/M4)
- [x] Prompt template loading from text files (deferred: actual prompts written in M3/M4)
- [x] Validation on load (required fields, sane ranges) — **Engine refuses to start on validation failure** with clear error messages
- [ ] Cross-validation: tool schemas reference only registered handlers, all prompt files exist, no orphaned config

### Database Clients

> Review: [PLAN.md §4.4 Database Schemas](./PLAN.md#44-database-schemas), [§10 Tech Stack — Key Rust Crates](./PLAN.md#key-rust-crates)

- [x] Neo4j client module (`crates/engine/src/graph/`) using `neo4rs`
  - [x] Connection pool initialization
  - [x] Health check query
  - [x] Schema initialization (indexes, constraints) — run on startup — 14 indexes (2 uniqueness constraints, 4 range, 3 fulltext, 3 vector incl. relationship vector)
- [x] PostgreSQL client module (`crates/engine/src/store/`) using `sqlx`
  - [x] Connection pool initialization
  - [x] Health check query
  - [x] Migration system (sqlx migrations)
  - [x] Initial migration: `investigations`, `work_orders`, `assessments` tables per PLAN.md schema (+ SUSPENDED columns on investigations)
  - [x] pgvector extension enabled, vector(1536) column on assessments, ivfflat index
- [x] Redis client module (`crates/engine/src/queue/`) using `redis-rs`
  - [x] Connection initialization
  - [x] Health check (PING)
  - [x] Stream and consumer group creation for work order queues (`workorders:high`, `workorders:normal`, `workorders:low`)

### Docker Compose

> Review: [PLAN.md §13 Deployment — Stage 1: Local Development](./PLAN.md#stage-1-local-development-docker-compose)

- [x] `docker-compose.yml` with core services:
  - [x] Neo4j (pinned version 5.18+, named volume, health check) — Neo4j 5.26.21
  - [x] PostgreSQL + pgvector (pinned version, named volume, health check) — PG 17.8 + pgvector 0.8.1
  - [x] Redis (pinned version, named volume, health check) — Redis 7.4.7
  - [x] Engine (Dockerfile, depends_on with health checks)
  - [x] Fetch (Dockerfile, depends_on)
  - [x] Geo (Dockerfile, depends_on)
- [x] Docker Compose profiles:
  - [x] Default: core services (Engine, Fetch, Geo, databases)
  - [x] `full`: adds fetch-browser sidecar, Scribe
  - [x] `observability`: adds Grafana, Prometheus, Loki
- [x] `config/` directory volume-mounted into Engine container
- [x] Named volumes for all database persistence
- [x] Dockerfiles:
  - [x] Engine (multi-stage Rust build via shared `Dockerfile.rust`)
  - [x] Fetch (multi-stage Rust build via shared `Dockerfile.rust`)
  - [x] Geo (multi-stage Rust build via shared `Dockerfile.rust`)
  - [x] fetch-browser (Node.js + Playwright — skeleton only, built out in M5)
  - [x] Scribe (Python + Whisper — skeleton only, built out in M5)
- [x] `docker compose up` starts core services, all healthy

### CI Pipeline

> Review: [PLAN.md §12 CI & Testing](./PLAN.md#12-ci--testing)

- [x] GitHub Actions workflow (`.github/workflows/ci.yml`)
- [x] Path-filtered triggers (all jobs run on push/PR; path filtering deferred to when Node.js/Python have real code):
  - [x] Rust workspace: fmt, clippy, test, build release
  - [x] Node.js: skeleton check
  - [x] Python: skeleton check
- [x] Rust checks: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test --workspace`, `cargo build --release`
- [x] Service containers for integration tests (Neo4j 5.26, pgvector/pg17, Redis 7.4 — matching docker-compose.yml)
- [x] Integration test job (separate from unit tests, runs `cargo test -- --ignored`)
- [x] Merge gate: all jobs required to pass

### Observability Foundation

> Review: [PLAN.md §2 Core Design Philosophy — Observability is Built In](./PLAN.md#observability-is-built-in-not-added-later)

- [x] `tracing` crate setup in all Rust services (Engine, Fetch, Geo)
  - [x] JSON-structured log output
  - [x] `investigation_id` as correlation key in spans (pattern established; wired per-investigation in M4)
  - [x] Log level configuration via environment variable (`RUST_LOG`)
- [x] `/health` endpoint on every Rust service (axum) — Engine checks all 3 databases, Fetch/Geo return simple healthy
- [x] `/metrics` endpoint skeleton on every Rust service (Prometheus format via `metrics-exporter-prometheus`)
- [x] Observability container configs (skeleton, fleshed out in M6):
  - [x] `observability/prometheus/prometheus.yml` — scrape targets for Engine, Fetch, Geo
  - [x] `observability/grafana/` — datasource config pointing to Prometheus and Loki
  - [x] `observability/loki/` — basic config

---

## M2: Knowledge Graph Operations

**Output:** We can store and retrieve structured world knowledge. All three search modes work. Embeddings are computed and stored.

This is the foundation the entire system reasons over. Every subsequent milestone reads from or writes to the graph.

### Neo4j Schema & Indexes

> Review: [PLAN.md §4.4 Neo4j Knowledge Graph Schema](./PLAN.md#neo4j-knowledge-graph-schema)

- [x] Schema initialization on Engine startup:
  - [x] Entity node constraints (uniqueness on `id`)
  - [x] Claim node constraints (uniqueness on `id`)
  - [x] Full-text index on Entity (`canonical_name`, `aliases`)
  - [x] Vector index on Entity (`embedding`)
  - [x] Range index on Entity (`kind`)
  - [x] Range index on Entity (`last_updated`)
  - [x] Full-text index on Claim (`content`)
  - [x] Vector index on Claim (`embedding`)
  - [x] Range index on Claim (`published_timestamp`)
  - [x] Range index on Claim (`ingested_timestamp`)
  - [x] Full-text index on RELATES_TO (`description`)
  - [x] Vector index on RELATES_TO (`embedding`) — requires Neo4j 5.18+
- [x] Verify all indexes created successfully on startup (log warnings on failure)

### Entity Operations

> Review: [PLAN.md §4.3 Entities](./PLAN.md#entities-in-knowledge-graph), [§4.8 Search & Retrieval](./PLAN.md#48-search--retrieval)

- [x] `create_entity` — write node with all required fields, compute embedding (name + summary), store freeform properties as node properties
- [x] `get_entity` — retrieve by ID with all properties
- [x] `update_entity` — update specified fields, recompute embedding if name/summary changed, update `last_updated`
- [x] `search_entities` — semantic/vector search (query → embed → vector similarity)
- [x] `search_entities` — full-text/keyword search (name, aliases)
- [x] `search_entities` — filtered by `kind`, temporal range on `last_updated`
- [x] Stub entity support: `is_stub` flag, create stubs with minimal data
- [x] External identifier storage (wikidata_qid, stock_ticker, iso_code, etc. as node properties)
- [x] `merge_entities` — merge source entity into target: reassign all PUBLISHED/REFERENCES edges from claims, reassign all RELATES_TO edges (both directions), combine aliases, delete source entity, log merge event for audit

### Claim Operations

> Review: [PLAN.md §4.3 Claims](./PLAN.md#claims-in-knowledge-graph)

- [x] `create_claim` — write node, compute embedding (content), create PUBLISHED edge from source entity, create REFERENCES edges to referenced entities
- [x] `search_claims` — semantic/vector search
- [x] `search_claims` — keyword/full-text search
- [x] `search_claims` — temporal filtering (`published_after`, `published_before`, sort by `published_timestamp`)
- [x] `search_claims` — by referenced entity ID
- [x] `search_claims` — by source entity ID
- [x] `search_claims` — by attribution depth (primary/secondhand)

### Relationship Operations

> Review: [PLAN.md §4.3 Relationships](./PLAN.md#relationships-in-knowledge-graph)

- [x] `create_relationship` — write RELATES_TO edge with properties, compute embedding (description)
- [x] `update_relationship` — update specified properties, recompute embedding if description changed
- [x] `traverse_relationships` — from entity ID, with optional direction filter, description query (semantic), min_weight filter, limit
- [x] `search_relationships` — semantic search across all relationships
- [x] Bidirectional relationship handling: store one edge with `bidirectional: true`, query tools traverse both directions

### Embedding Pipeline

> Review: [PLAN.md §11 Error Handling — Embedding Pipeline](./PLAN.md#embedding-pipeline)

- [x] Embedding API client (OpenAI `text-embedding-3-small` initially)
  - [x] Single text embedding
  - [x] Batch embedding (multiple texts in one API call)
  - [x] Configurable provider, model, dimensions, batch_size from `system.toml`
- [x] Normal flow: collect texts → batch embed → write to Neo4j with embeddings in single transaction
- [x] Failure flow: write without embeddings, flag `embedding_pending: true`
- [x] Background backfill task: periodic query for `embedding_pending: true`, compute embeddings, update records
  - [x] Configurable interval from `system.toml`
- [x] Retry logic on embedding API calls (per retry config)

### Entity Deduplication

> Review: [PLAN.md §4.3 Entities — Deduplication](./PLAN.md#entities-in-knowledge-graph)

- [x] Stage 1: exact string matching on canonical_name and aliases
- [x] Stage 2: fuzzy string matching (edit distance / Jaro-Winkler)
- [x] Stage 3: embedding similarity (compare candidate embedding against existing entity embeddings)
- [x] Stage 4 interface: LLM judgment for ambiguous cases (implementation deferred to M3 when LLM client exists, but define the interface/trait now)
- [x] Cascading pipeline: fast stages filter, expensive stages only for uncertain matches
- [x] Dedup function returns: exact match, probable match (with confidence), no match

### Integration Tests

> Review: [PLAN.md §12 CI & Testing — Integration Tests](./PLAN.md#integration-tests)

- [x] Test harness: spin up Neo4j test container (or use CI service container), seed and tear down per test
- [x] Entity CRUD tests (create, read, update, search all modes)
- [x] Claim CRUD tests (create, search with temporal filters, attribution depth filter)
- [x] Relationship CRUD tests (create, traverse, search, bidirectional handling)
- [x] Vector search accuracy tests (create entities with known content, verify semantic search returns expected results)
- [x] Full-text search tests (exact name, alias, partial match)
- [ ] Multi-hop traversal test (A→B→C, query from A with depth 2) — deferred: single-hop traverse implemented; multi-hop requires application-level iteration
- [x] Embedding pipeline tests (batch embed, write, verify searchable)
- [x] `embedding_pending` backfill test (write without embedding, run backfill, verify searchable)
- [x] Entity merge test (create two entities with claims and relationships, merge, verify all edges reassigned, source deleted, aliases combined)

### Observability

- [x] Graph query latency metrics (histogram per operation type)
- [x] Embedding API call metrics (latency, tokens, errors)
- [ ] Entity/claim/relationship count metrics (gauges) — deferred: requires periodic count queries; add when dashboards are built (M6)
- [x] Embedding backfill queue depth metric
- [x] Entity dedup metrics: dedup stage hit rates (string/embedding/LLM), false positive count (tracked via `merge_entities` calls — each merge implies a prior dedup failure), entity merge count

---

## M3: Processor Pipeline

**Output:** Given a URL or document, the system extracts structured knowledge into the graph automatically. The first time you watch the system do something impressive.

This is the first moment of truth. The LLM touches the system for the first time. Expect this milestone to be the most iterative — tool schemas, prompt wording, and extraction quality all get refined here.

### LLM Client

> Review: [PLAN.md §10 Tech Stack — LLM Integration](./PLAN.md#llm-integration), [§4.9 Tool Layer — Agentic Loop Mechanics](./PLAN.md#agentic-loop-mechanics)

- [x] LLM provider trait (abstraction over Anthropic/OpenAI):
  - [x] Send messages with tool definitions → get response
  - [x] Parse response: text content, tool calls, or mixed
  - [x] Token usage tracking
- [x] Anthropic API client (reqwest + serde):
  - [x] Messages API with tool use
  - [x] Tool call response parsing
  - [x] Tool result message construction
- [x] OpenAI API client (for embeddings; optionally for LLM calls):
  - [x] Chat completions with function calling
  - [x] Embeddings API (may already exist from M2)
- [x] Provider + model configurable per role in `system.toml` (Analyst model, Processor model, embedding model)
- [x] Non-streaming responses (streaming deferred per key decision)
- [x] Retry logic: exponential backoff with jitter, respect `Retry-After` headers
  - [x] Max 3 attempts, 1s initial, 30s max backoff (configurable)
  - [x] No retry on auth failures or context window exceeded

### Agentic Loop

> Review: [PLAN.md §4.9 Tool Layer — Agentic Loop Mechanics](./PLAN.md#agentic-loop-mechanics), [§4.9 Session Termination](./PLAN.md#session-termination)

- [x] Core loop implementation:
  1. Build messages: system prompt + conversation history + tool definitions
  2. Send to LLM API
  3. Response contains tool call(s) → execute each, append tool_result to history, loop
  4. Response contains text only → session complete
- [x] Max turns enforcement (configurable safety limit)
- [x] Session result type: `Completed(text)`, `ToolCalls(results)`, `MaxTurnsReached`, `Failed(error)`
- [x] Conversation history management (append messages, tool results)

### Tool Layer

> Review: [PLAN.md §4.9 Tool Layer](./PLAN.md#49-tool-layer), [§4.9 Tool Definitions in Config](./PLAN.md#tool-definitions-in-config)

- [x] Tool schema loading: read JSON files from `config/tools/processor/`, parse into LLM-compatible tool definitions
- [x] Handler registry: map tool name → Rust handler function
- [x] Tool execution dispatcher: receive tool call from LLM response → find handler → execute → return tool_result
- [x] Error as tool_result: handler failures return `{ "is_error": true, "content": "..." }` to LLM
- [x] LLM self-correction tracking: count consecutive malformed tool calls, end session after 3 (configurable)
- [x] Tool result size limits (from `handler_config` in tool JSON files)
- [x] Intelligent truncation: search results return top N with omitted count (not byte-level cutoff), entity details truncate freeform properties before core fields, claim searches truncate content previews before dropping results — LLM always knows what was truncated and how much it's missing

### Processor Tool Schemas

> Review: [PLAN.md §4.9 Processor Tools](./PLAN.md#processor-tools)

- [x] `config/tools/processor/search_entities.json`
- [x] `config/tools/processor/create_entity.json`
- [x] `config/tools/processor/update_entity.json`
- [x] `config/tools/processor/create_claim.json`
- [x] `config/tools/processor/create_relationship.json`
- [x] `config/tools/processor/update_relationship.json`
- [x] `config/tools/processor/fetch_url.json`
- [x] `config/tools/processor/update_entity_with_change_claim.json`
- [x] `config/tools/processor/fetch_source_catalog.json`
- [x] `config/tools/processor/fetch_source_query.json`

### Processor Tool Handlers

> Review: [PLAN.md §4.9 Processor Tools](./PLAN.md#processor-tools)

Wire each tool to the graph operations from M2 and Fetch from below:

- [x] `search_entities` handler → graph client semantic + full-text search
- [x] `create_entity` handler → graph client create_entity (with dedup check first)
- [x] `update_entity` handler → graph client update_entity
- [x] `update_entity_with_change_claim` handler → graph client update_entity + create_claim in single transaction (atomic "changes as claims" pattern)
- [x] `create_claim` handler → graph client create_claim
- [x] `create_relationship` handler → graph client create_relationship
- [x] `update_relationship` handler → graph client update_relationship
- [x] `fetch_url` handler → Fetch service `/fetch` endpoint
- [x] `fetch_source_catalog` handler → Fetch service `/sources` endpoint
- [x] `fetch_source_query` handler → Fetch service `/sources/{id}/query` endpoint

### AutOSINT Fetch (Basic)

> Review: [PLAN.md §5 AutOSINT Fetch](./PLAN.md#5-autosint-fetch), [§5.3 Caching](./PLAN.md#53-caching), [§5.5 API](./PLAN.md#55-api)

Minimal viable Fetch — enough for Processors to retrieve web content. Source adapters and browser automation come in M5.

- [x] Axum HTTP service with `/health` endpoint
- [x] `POST /fetch` — raw HTTP fetch (reqwest):
  - [x] Accept URL + options
  - [x] Fetch content
  - [x] Return content + metadata (status, content_type, etc.)
  - [x] HTML content extraction (scraper crate — strip to readable text)
- [x] `GET /sources` — source catalog (returns empty list initially; populated in M5)
- [x] URL-keyed response cache with configurable TTL
- [x] Rate limiting foundation (per-domain, configurable)
- [x] Structured logging with `tracing`
- [x] `/metrics` endpoint

### Processor Session Management

> Review: [PLAN.md §4.5 Processor](./PLAN.md#processor), [§4.9 Session Model](./PLAN.md#session-model)

- [x] Processor system prompt (`config/prompts/processor.md`):
  - [x] Role definition (discovery + extraction worker)
  - [x] Extraction guidance: extract ALL key claims, not just what the work order asked about
  - [x] Claims are units of information, not text — scale with information density
  - [x] Attribution depth guidance (primary vs secondhand)
  - [x] Dual timestamp awareness (published vs ingested)
  - [x] Entity deduplication guidance (search before creating)
  - [x] Changes-as-claims guidance
- [x] Processor session runner:
  - [x] Accept work order (objective, referenced entities, source guidance)
  - [x] Build initial message with system prompt + work order context
  - [x] Run agentic loop with Processor tools
  - [x] Session ends on text-only response
  - [x] Return session result (claims created count, entities created count, errors)

### End-to-End Validation

- [ ] Manual test: give a Processor a URL (e.g., a news article), watch it fetch, extract entities, create claims, build graph structure
- [ ] Verify entity deduplication works (process two articles about the same topic, entities should merge not duplicate)
- [ ] Verify claim extraction quality (information density, not text regurgitation)

### Observability

- [x] LLM API metrics: request latency, input/output tokens, cost estimate, error rate, per-provider
- [x] Processor session metrics: duration, tool call count, entities created, claims created, relationships created
- [x] Fetch metrics: request latency, cache hit rate, error rate, per-domain
- [x] Tool execution metrics: per-tool call count, latency, error rate

---

## M4: Investigation Loop

**Output:** Given a question, the system investigates and produces an intelligence assessment. This is the core product.

The Analyst drives its own investigation: queries the graph, identifies gaps, creates work orders, waits for Processors, reasons over new data, and produces an assessment. The Orchestrator manages the lifecycle.

### Work Order System

> Review: [PLAN.md §4.6 Work Order System](./PLAN.md#46-work-order-system), [§4.4 Redis Schema](./PLAN.md#redis-schema-work-order-queue)

- [ ] Work order enqueue: write to PostgreSQL (status: queued) + XADD to appropriate Redis priority stream
- [ ] Work order dequeue: XREADGROUP from consumer group, update PostgreSQL (status: processing)
- [ ] Work order completion: XACK in Redis, update PostgreSQL (status: completed)
- [ ] Work order failure: update PostgreSQL (status: failed), retry once (re-queue), then permanent failure
- [ ] Pending entry reclamation: detect unacknowledged messages past heartbeat TTL, reclaim and re-queue
- [ ] Priority checking: Processors check `workorders:high` → `workorders:normal` → `workorders:low`

### Work Order Integration Tests

> Review: [PLAN.md §12 CI & Testing — Integration Tests](./PLAN.md#integration-tests)

- [ ] Redis Streams: XADD → XREADGROUP → XACK lifecycle
- [ ] Consumer group behavior: multiple consumers, message assignment
- [ ] Pending entry detection and reclamation after timeout
- [ ] Priority ordering: high-priority messages consumed before normal/low

### Processor Pool

> Review: [PLAN.md §4.7 Orchestration — Processor Liveness](./PLAN.md#processor-liveness-heartbeats)

- [ ] Tokio task pool: spawn Processor sessions as tasks
- [ ] Configurable pool size (from `system.toml`)
- [ ] Heartbeat system: Processor writes Redis key `processor:{id}:heartbeat` with short TTL, refreshes periodically — **heartbeat runs as an independent tokio task**, separate from the main processing work (critical for Processors blocked on long operations like Scribe long-polls)
- [ ] Heartbeat monitoring: Orchestrator checks for expired heartbeat keys
- [ ] Dead Processor handling: expired heartbeat → reclaim work order, log event
- [ ] Processor lifecycle: idle → claim work order → run session → report result → idle

### Orchestrator State Machine

> Review: [PLAN.md §4.7 Orchestration](./PLAN.md#47-orchestration), [§4.7 Investigation State Machine](./PLAN.md#investigation-state-machine)

- [ ] Investigation creation: receive prompt → write to PostgreSQL (PENDING)
- [ ] State transition engine: enforce valid transitions per state machine diagram
- [ ] PENDING → ANALYST_RUNNING: start Analyst session
- [ ] ANALYST_RUNNING → PROCESSING: Analyst created work orders, dispatch to Redis, increment cycle_count
- [ ] ANALYST_RUNNING → COMPLETED: Analyst called produce_assessment
- [ ] ANALYST_RUNNING → ANALYST_RUNNING: empty session (no work orders, no assessment) → retry once, second empty → force final assessment mode
- [ ] PROCESSING → ANALYST_RUNNING: all work orders resolved (completed or permanently failed)
- [ ] Any active → COMPLETED: max cycles reached → force final Analyst session with modified prompt
- [ ] PROCESSING → FAILED: two consecutive cycles where ALL work orders failed
- [ ] State persistence: all transitions written to PostgreSQL
- [ ] Concurrent investigation support: each investigation is independent state machine

### Safety Limits

> Review: [PLAN.md §4.7 Safety Limits](./PLAN.md#safety-limits)

- [ ] Max cycles per investigation (configurable, default ~10)
- [ ] Max turns per Analyst session (configurable, default ~50)
- [ ] Max work orders per cycle (configurable, default ~20)
- [ ] Heartbeat TTL (configurable, default ~60s)
- [ ] Consecutive all-fail cycle limit (2)
- [ ] All limits read from `system.toml`

### Assessment Store

> Review: [PLAN.md §4.2 Assessment Store](./PLAN.md#assessment-store-postgresql--pgvector), [§4.3 Assessments](./PLAN.md#assessments-in-assessment-store)

- [ ] Write assessment: insert to PostgreSQL with embedding (semantic search over assessments)
- [ ] Search assessments: semantic/vector search via pgvector
- [ ] Get assessment by ID: full content retrieval
- [ ] Assessment references graph entities/claims by ID (JSONB arrays, cross-database references)

### Assessment Store Integration Tests

- [ ] Write and retrieve assessment
- [ ] Semantic search over assessments (write multiple, query, verify relevance ranking)
- [ ] Entity/claim reference integrity (references stored correctly as JSONB)

### Analyst Tool Schemas

> Review: [PLAN.md §4.9 Analyst Tools](./PLAN.md#analyst-tools)

- [ ] `config/tools/analyst/search_entities.json`
- [ ] `config/tools/analyst/get_entity.json`
- [ ] `config/tools/analyst/traverse_relationships.json`
- [ ] `config/tools/analyst/search_relationships.json`
- [ ] `config/tools/analyst/search_claims.json`
- [ ] `config/tools/analyst/search_assessments.json`
- [ ] `config/tools/analyst/get_assessment.json`
- [ ] `config/tools/analyst/create_work_order.json`
- [ ] `config/tools/analyst/produce_assessment.json`
- [ ] `config/tools/analyst/merge_entities.json`
- [ ] `config/tools/analyst/get_investigation_history.json`
- [ ] `config/tools/analyst/list_fetch_sources.json`
- [ ] `config/tools/analyst/query_geo.json` (handler returns "Geo unavailable" until M5)

### Analyst Tool Handlers

> Review: [PLAN.md §4.9 Analyst Tools](./PLAN.md#analyst-tools)

- [ ] `search_entities` handler → graph client (same as Processor but may return different fields: id, canonical_name, kind, summary, score)
- [ ] `get_entity` handler → graph client get_entity with all properties
- [ ] `traverse_relationships` handler → graph client traverse
- [ ] `search_relationships` handler → graph client relationship search
- [ ] `search_claims` handler → graph client claim search (all filter modes)
- [ ] `search_assessments` handler → Assessment Store semantic search
- [ ] `get_assessment` handler → Assessment Store get by ID
- [ ] `merge_entities` handler → graph client merge_entities (reassign edges, combine aliases, delete source, log merge event)
- [ ] `create_work_order` handler → work order enqueue (PostgreSQL + Redis)
- [ ] `produce_assessment` handler → Assessment Store write
- [ ] `get_investigation_history` handler → PostgreSQL query (work orders for current investigation, grouped by cycle)
- [ ] `list_fetch_sources` handler → Fetch `/sources` endpoint
- [ ] `query_geo` handler → stub returning "AutOSINT Geo not yet available" (wired to Geo in M5)

### Analyst Session Management

> Review: [PLAN.md §4.5 Analyst](./PLAN.md#analyst), [§4.9 Session Model](./PLAN.md#session-model), [§4.9 Session Termination](./PLAN.md#session-termination)

- [ ] Analyst system prompt (`config/prompts/analyst.md`):
  - [ ] Role definition (central intelligence actor, self-regulating feedback loop)
  - [ ] Self-serve context guidance (query graph, query assessments, query Geo)
  - [ ] Gap identification → work order creation guidance
  - [ ] "Do I know enough?" decision framework
  - [ ] Temporal relevance guidance (consider claim age per-topic)
  - [ ] Assessment production guidance (conclusions, confidence, reasoning, gaps, competing hypotheses, forward indicators)
  - [ ] Honest uncertainty guidance (always produce assessment, state limitations)
  - [ ] `create_work_order` vs `produce_assessment` as mutually exclusive session outcomes
- [ ] Analyst session runner:
  - [ ] Fresh session per cycle (no conversation continuity)
  - [ ] Investigation prompt + tool access
  - [ ] Run agentic loop with Analyst tools
  - [ ] Detect session outcome: work orders created → PROCESSING, assessment produced → COMPLETED
  - [ ] Max turns enforcement
- [ ] Force-final-assessment mode: modified system prompt for max-cycle and empty-session-retry scenarios

### Error Handling

> Review: [PLAN.md §11 Error Handling](./PLAN.md#11-error-handling)

- [ ] Dependency classification:
  - [ ] Hard: Neo4j, PostgreSQL, Redis, LLM API
  - [ ] Soft: Fetch, Geo, Scribe
- [ ] Circuit breakers on all dependencies:
  - [ ] Closed → Open on failure threshold
  - [ ] Open → Half-Open after cooldown
  - [ ] Half-Open → Closed on probe success, Open on probe failure
  - [ ] Configurable thresholds and cooldowns
- [ ] SUSPENDED state:
  - [ ] Hard dependency circuit opens → transition active investigations to SUSPENDED
  - [ ] Persist reason, timestamp, resume point in PostgreSQL
  - [ ] Circuit closes → auto-resume SUSPENDED investigations
- [ ] Soft dependency failures → error as tool_result (LLM adapts)
- [ ] Engine restart recovery:
  - [ ] On startup, query PostgreSQL for non-terminal investigations
  - [ ] SUSPENDED → check dependency health, resume if available
  - [ ] ANALYST_RUNNING / PROCESSING (crashed mid-operation) → treat as SUSPENDED

### Failed Investigation Handling

> Review: [PLAN.md §4.7 Failed Investigations](./PLAN.md#failed-investigations)

- [ ] FAILED investigation triggers final Analyst session with modified prompt
- [ ] "Produce the best assessment you can with available information, noting all gaps, limitations, and failures encountered"
- [ ] Partial assessment written to Assessment Store

### Investigation History Tool

> Review: [PLAN.md §4.7 Investigation History (Anti-Dedup)](./PLAN.md#investigation-history-anti-dedup)

- [ ] `get_investigation_history` returns: cycle number, work orders per cycle (objective, status, claims_produced_count)
- [ ] Analyst sees full investigation history — avoids redundant work orders via judgment

### End-to-End Validation

- [ ] Manual test: submit investigation prompt → Analyst queries graph → creates work orders → Processors execute → Analyst re-evaluates → produces assessment
- [ ] Verify multi-cycle investigation (Analyst requests data, gets it, requests more, then produces assessment)
- [ ] Verify assessment quality (explicit reasoning, confidence levels, entity/claim references, gaps noted)
- [ ] Verify safety limits (max cycles triggers forced assessment, max turns ends session)

### Observability

- [ ] Investigation lifecycle metrics: created, active, completed, failed, suspended counts
- [ ] Cycle count distribution per investigation
- [ ] Work order queue depth (per priority)
- [ ] Processor pool utilization (active/idle/dead)
- [ ] Analyst session metrics: duration, tool calls, outcome (work_orders vs assessment)
- [ ] Assessment production metrics: count, confidence distribution
- [ ] Circuit breaker state metrics (per dependency)

---

## M5: External Modules

**Output:** The system can access diverse data sources, reason about physical geography, and process multimedia. These extend capability without changing the core.

Each module is independently deployable. Build order within this milestone is flexible — modules are independent of each other.

### Fetch Source Adapters

> Review: [PLAN.md §5.2 Source Adapters](./PLAN.md#source-adapters), [§5.2 Planned Source Adapters](./PLAN.md#planned-source-adapters-initial)

- [ ] Source adapter trait: authentication, query translation, rate limiting, pagination, response normalization, coverage metadata
- [ ] Source registration: adapters register on startup, appear in `/sources` catalog
- [ ] Adapter: web search API (Google/Bing/Brave — pick one to start)
- [ ] Adapter: RSS feed aggregator (wire services, major outlets)
- [ ] Adapter: NewsAPI
- [ ] `POST /sources/{id}/query` wired to adapter dispatch
- [ ] Per-source rate limiting (configurable per adapter)
- [ ] Per-source authentication (API keys from environment/config)
- [ ] Response normalization (all adapters return consistent format)
- [ ] Additional adapters as needed (SEC EDGAR, Reddit, government databases — each is incremental)

### Fetch Browser Automation

> Review: [PLAN.md §5.4 Browser Automation](./PLAN.md#54-browser-automation)

- [ ] Node.js + Playwright sidecar service (`services/fetch-browser/`):
  - [ ] Always-running Docker container with headless Chromium
  - [ ] HTTP endpoint for simple render requests
  - [ ] WebSocket endpoint for interactive sessions
  - [ ] Concurrent browser context management (configurable cap, e.g. 4-6)
  - [ ] Context isolation (separate cookies, sessions, storage per request)
  - [ ] Session timeout and cleanup
  - [ ] Health endpoint
  - [ ] `pino` structured logging
- [ ] Fetch integration:
  - [ ] `POST /browse` → delegate to sidecar HTTP endpoint
  - [ ] `WS /browse/session` → proxy WebSocket to sidecar
- [ ] Processor browser tools:
  - [ ] `browse_url` tool schema + handler
  - [ ] `browser_open` tool schema + handler
  - [ ] `browser_click` tool schema + handler
  - [ ] `browser_fill` tool schema + handler
  - [ ] `browser_scroll` tool schema + handler
  - [ ] `browser_close` tool schema + handler
- [ ] Browser tools return rendered DOM/text content (not screenshots)

### AutOSINT Geo

> Review: [PLAN.md §6 AutOSINT Geo](./PLAN.md#6-autosint-geo)

- [ ] PostGIS database container in Docker Compose
- [ ] Geographic data loading pipeline:
  - [ ] Research and select initial data sources (Natural Earth, OpenStreetMap, UN LOCODE, etc.)
  - [ ] Import scripts for each data source
  - [ ] Verify coverage for initial geographic scope (US, Canada, Mexico, Europe, global features)
- [ ] Perception script engine:
  - [ ] Location → structured text summary (terrain, borders, nearby features, infrastructure, climate)
  - [ ] Named places and descriptive language output (not coordinates)
- [ ] API endpoints (axum):
  - [ ] `GET /health`
  - [ ] `GET /capabilities`
  - [ ] `POST /context` — perception script for a location
  - [ ] `POST /spatial/nearby` — features within radius
  - [ ] `POST /spatial/distance` — distance and terrain between two points
  - [ ] `POST /spatial/route` — what a path crosses
  - [ ] `POST /terrain` — terrain description for an area
  - [ ] `POST /borders` — border information
  - [ ] `POST /features` — features in a region
- [ ] Wire Analyst's `query_geo` tool handler to Geo service API (replace stub from M4)
- [ ] Structured logging with `tracing`, `/metrics` endpoint

### AutOSINT Scribe

> Review: [PLAN.md §7 AutOSINT Scribe](./PLAN.md#7-autosint-scribe)

- [ ] Python service (`services/scribe/`):
  - [ ] Whisper integration for speech-to-text
  - [ ] Speaker diarization (pyannote.audio)
  - [ ] Original language only (no translation)
  - [ ] Per-segment confidence scores
  - [ ] Timecoded segments (start/end timestamps)
  - [ ] `structlog` structured logging
- [ ] Platform adapters:
  - [ ] YouTube (yt-dlp or similar for media download)
  - [ ] Direct audio/video URLs
  - [ ] Platform auto-detection from URL
  - [ ] Platform catalog endpoint (`GET /platforms`)
- [ ] Job-based async processing:
  - [ ] `POST /transcribe` → submit job, return job_id
  - [ ] `GET /transcribe/{id}?block=true&timeout=300` → long-poll for result
  - [ ] `DELETE /transcribe/{id}` → cancel pending job
  - [ ] `GET /health`
- [ ] Output format: JSON with segments (speaker, content, start, end, confidence), context passthrough, language detected
- [ ] Processor transcription tools:
  - [ ] `submit_transcription` tool schema + handler
  - [ ] `get_transcription` tool schema + handler
- [ ] Docker container with Whisper model, ffmpeg, diarization model

### Observability

- [ ] Fetch adapter metrics: per-source request count, latency, error rate, cache hit rate
- [ ] Browser sidecar metrics: active contexts, session duration, render time
- [ ] Geo query metrics: per-endpoint latency, error rate
- [ ] Scribe job metrics: queue depth, transcription duration, segments produced, confidence distribution

---

## M6: Production Hardening

**Output:** The system runs reliably on a VPS and we can observe its behavior through dashboards and alerts.

### VPS Deployment

> Review: [PLAN.md §13 Deployment — Stage 2: Single VPS Production](./PLAN.md#stage-2-single-vps-production-docker-compose)

- [ ] `docker-compose.prod.yml` overrides:
  - [ ] Resource limits per container
  - [ ] Restart policies (`unless-stopped`)
  - [ ] Production logging drivers
  - [ ] Built images only (no source mounts)
  - [ ] Volume management for databases
- [ ] CI → production pipeline:
  - [ ] GitHub Actions: build container images on merge to main
  - [ ] Push to GitHub Container Registry (GHCR)
  - [ ] Deploy script: pull new images + `docker compose -f docker-compose.yml -f docker-compose.prod.yml up -d`
- [ ] VPS setup:
  - [ ] Docker + Docker Compose installed
  - [ ] Firewall configuration
  - [ ] SSH key access only
  - [ ] Environment variables for secrets (API keys, database passwords)

### Database Backups

> Review: [PLAN.md §13 Deployment — Stage 2](./PLAN.md#stage-2-single-vps-production-docker-compose)

- [ ] Neo4j: `neo4j-admin dump` cron job
- [ ] PostgreSQL: `pg_dump` cron job
- [ ] Backup rotation (keep N most recent)
- [ ] Backup push to object storage (S3-compatible)
- [ ] Redis: no backup needed (work orders persist in PostgreSQL; Redis loss = re-queue from PG state)
- [ ] Backup verification: periodic restore test

### Monitoring Dashboards

> Review: [PLAN.md §2 Core Design Philosophy — Observability](./PLAN.md#observability-is-built-in-not-added-later)

- [ ] Prometheus scrape configuration (all service `/metrics` endpoints)
- [ ] Loki log aggregation (all service JSON logs)
- [ ] Grafana dashboards:
  - [ ] **System Overview**: service health, resource usage, database connection pools, overall throughput
  - [ ] **Investigation Detail**: filter by investigation_id, see full lifecycle (Analyst sessions, work orders, Processor execution, assessment production)
  - [ ] **Processor Pool**: active/idle/dead Processors, work order throughput, claim extraction rate, dedup hit rate
  - [ ] **LLM Usage**: API call volume, latency, token usage, cost estimates, error rates, per-provider, per-model
  - [ ] **Infrastructure Health**: database health, Redis queue depth, circuit breaker states, embedding backfill queue

### Alerting

> Review: [PLAN.md §2 Core Design Philosophy — Observability](./PLAN.md#observability-is-built-in-not-added-later)

- [ ] Critical alerts:
  - [ ] Any hard dependency unreachable (Neo4j, PostgreSQL, Redis, LLM API)
  - [ ] All Processors dead (no heartbeats)
  - [ ] Engine process down
- [ ] Warning alerts:
  - [ ] High Processor utilization (>80% pool active)
  - [ ] Growing work order queue depth (threshold configurable)
  - [ ] Elevated error rates (LLM API, database queries)
  - [ ] Embedding backfill queue growing
- [ ] Informational alerts:
  - [ ] Investigation completed/failed
  - [ ] Processor restarted after crash
  - [ ] Circuit breaker state change

### Performance Tuning

- [ ] Run real investigations, observe behavior through dashboards
- [ ] Database query optimization (identify slow queries via metrics)
- [ ] Embedding batch size tuning (balance latency vs throughput)
- [ ] Processor pool size tuning (balance concurrency vs resource usage)
- [ ] LLM token usage optimization (tool result size limits, prompt efficiency)
- [ ] Cache TTL tuning for Fetch
- [ ] Identify bottlenecks for future scaling decisions

---

## Future: Not on This Roadmap

These are explicitly deferred. They need the system to exist and produce real data before they can be designed.

> Review: [PLAN.md §17 Not Yet Designed](./PLAN.md#17-not-yet-designed)

- **Multi-Analyst investigations** — decomposition into parallel sub-investigations, synthesis Analyst. Needs empirical data on where single Analysts struggle. ([PLAN.md §4.7 Multi-Analyst](./PLAN.md#multi-analyst-investigations-future))
- **Processor atomization** — extract Processors to separate binary for Kubernetes scaling. Build when scaling demands it, not before. ([PLAN.md §13 Processor Atomization](./PLAN.md#processor-atomization-future))
- **Kubernetes deployment** — Stage 3. Only when horizontal Processor scaling or HA is needed. ([PLAN.md §13 Stage 3](./PLAN.md#stage-3-kubernetes))
- **AutOSINT Triage** — raw signal monitoring, significance detection, investigation triggering. Needs Engine to exist first. ([PLAN.md §8](./PLAN.md#8-autosint-triage))
- **UX Layer** — user interfaces, Monitor module, user interest modeling, delivery. Designed after Engine is functional. ([PLAN.md §17](./PLAN.md#17-not-yet-designed))
- **Prompt engineering iteration** — continuous, not a milestone. Starts in M3 (Processor) and M4 (Analyst), never stops.
- **Distributed tracing (Tempo)** — add when investigating cross-service latency issues becomes necessary.

---

*This roadmap reflects the system design in [PLAN.md](./PLAN.md) as of its current state. Update this document when design decisions change or milestones are completed.*
