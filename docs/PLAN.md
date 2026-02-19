# AutOSINT — System Design Document

> This document captures the complete system design as discussed and agreed upon. It is the authoritative reference for implementation.

---

## Table of Contents

1. [Vision & Purpose](#1-vision--purpose)
2. [Core Design Philosophy](#2-core-design-philosophy)
3. [System Components Overview](#3-system-components-overview)
4. [AutOSINT Engine](#4-autosint-engine)
5. [AutOSINT Fetch](#5-autosint-fetch)
6. [AutOSINT Geo](#6-autosint-geo)
7. [AutOSINT Scribe](#7-autosint-scribe)
8. [AutOSINT Triage](#8-autosint-triage)
9. [External Module Pattern](#9-external-module-pattern)
10. [Tech Stack](#10-tech-stack)
11. [Error Handling](#11-error-handling)
12. [CI & Testing](#12-ci--testing)
13. [Deployment](#13-deployment)
14. [Research Findings](#14-research-findings)
15. [Stress Test Results](#15-stress-test-results)
16. [Key Decisions Log](#16-key-decisions-log)
17. [Not Yet Designed](#17-not-yet-designed)

---

## 1. Vision & Purpose

AutOSINT is a democratized intelligence analysis system. It is NOT a news aggregator. It is an LLM-powered intelligence operation that produces analytical products (intelligence reports) about geopolitics, macroeconomics, and regional politics.

### The Problem

Signal/noise. The internet is full of noise. AutOSINT extracts signal, personalized to what each user cares about.

### What Makes This Different

The signal isn't summarized news. It's actual intelligence:

- **Causal reasoning**: not "X happened" but "X happened, likely because of Y and Z"
- **Competing hypotheses**: "there's a 70% likelihood the driver is A, 30% likelihood it's B"
- **Impact analysis**: "this affects these entities, these supply chains, these relationships"
- **Forward-looking indicators**: "watch for W — if it happens, hypothesis A is confirmed"
- **Explicit uncertainty**: the system says what it doesn't know and how confident it is

This is the kind of analysis that intelligence agencies and expensive firms (Stratfor, RANE, Oxford Analytica) produce. AutOSINT democratizes it.

### Domain Scope

- Geopolitics
- Macroeconomics
- Regional/country-level politics

### Geographic Scope (Initial)

Soft boundaries on the user-facing side:
- United States
- Canada
- Mexico
- Europe
- Global Events

---

## 2. Core Design Philosophy

### We Build the Workstation, Not the Analyst

We don't encode analytical logic in code. We build the infrastructure — storage, tools, frameworks, orchestration — and let the LLM do the analysis. The LLM IS the analyst. Our job is to give it the best possible workstation: reliable storage, powerful tools, good data, and clear guidance about the purpose of each component.

### What's Hardcoded vs What the LLM Decides

**Hardcoded (we build this):**
- Storage schema (minimum required fields)
- Tool interfaces (predefined functions)
- Work order system
- Orchestration infrastructure
- Interop schemas for external modules
- Prompt frameworks and guidance

**LLM decides (we don't encode this):**
- What to extract from documents
- What properties matter for an entity
- How to assess source reliability
- What connections to draw between entities
- What level of detail to store
- All analytical judgments
- How deep to investigate
- What work orders to create
- When it knows enough to produce an assessment

### Source Evaluation is Emergent, Not Tiered

We do NOT predefine source reliability tiers. Sources are entities in the knowledge graph — with ownership, track record, incentive structures, geographic base, and relationships. The LLM assesses reliability by traversing this information, not by reading a predefined tier label. This means the system can discover things like: "this outlet is owned by this conglomerate, which has a stake in this industry, and this article is about regulation of that industry" — without anyone hardcoding that bias.

### All Numeric Parameters are Runtime-Configurable

No numeric limit, threshold, or tuning parameter is hardcoded into compiled code. Safety limits (max cycles, max turns, max work orders), concurrency parameters (Processor pool size, browser context cap), retry counts, backoff intervals, timeouts, cache TTLs, tool result size limits, embedding dimensions — all configurable at runtime. The right values for these are discovered empirically by running the system, not by guessing upfront. A recompile-to-test-a-different-number iteration loop is unacceptable for the kind of rapid tuning this system requires.

### Depth = Graph Density, Not Record Complexity

The LLM goes deeper by creating more entities and relationships, not by adding more properties to existing ones. Understanding emerges from the structure of the graph. A densely connected area of the graph represents deep understanding. A sparse area represents shallow or no understanding. The LLM's editorial judgment about depth manifests as how far it expands the graph, not how detailed individual records are.

### Observability is Built In, Not Added Later

Every service emits structured logs, exposes Prometheus metrics, and participates in distributed traces from the first line of code. This is not extra work after the system works — it's how we know the system works.

**Structured logging**: all services emit JSON-structured logs with consistent fields. `investigation_id` is the critical correlation key — filter by it and see the entire investigation story end-to-end across all services. Rust uses the `tracing` crate (structured, async-aware, OpenTelemetry-compatible). Python (Scribe) uses `structlog`. Node.js (fetch-browser) uses `pino`.

**Metrics**: every service exposes a `/metrics` endpoint in Prometheus format. Prometheus scrapes on interval. Key metrics cover: investigation lifecycle, Processor pool utilization, LLM API latency/cost/errors, database health and query latency, work order queue depth, cache hit rates, Fetch/Geo/Scribe throughput.

**Distributed tracing**: OpenTelemetry trace context propagated in HTTP headers across service boundaries. An investigation trace spans Analyst sessions, work orders, Processor execution, Fetch/Geo/Scribe calls — full request flow visibility.

**Alerting**: critical (infrastructure unreachable, all Processors dead, LLM API down), warning (high utilization, growing queue depth, elevated error rates), informational (investigation completed/failed, Processor restarted).

**Observability stack**: Grafana (dashboards + alerting), Prometheus (metrics), Loki (log aggregation), Tempo (distributed tracing — add when needed). All open source, all Docker containers. Pre-built dashboards: System Overview, Investigation Detail, Processor Pool, LLM Usage, Infrastructure Health.

---

## 3. System Components Overview

### AutOSINT Engine (Core)
The core system: discovery, consolidation, analysis, and storage layers. This is the intelligence engine. Contains the Analyst, Processors, Orchestrator, Knowledge Graph, Assessment Store, Work Order System, and Tool Layer.

### AutOSINT Fetch (External Module)
Browser control + hardcoded data sources. Provides all data retrieval capabilities to Engine Processors. Includes structured API adapters, raw HTTP fetching, and Playwright-based browser automation. All data retrieval goes through Fetch — the Engine never fetches directly.

### AutOSINT Geo (External Module)
Geographic knowledge, spatial queries, geographic context generation. Translates physical geography into structured text that LLMs can reason over. Terrain, borders, elevation, rivers, chokepoints, infrastructure, resource deposits.

### AutOSINT Scribe (External Module)
Multimedia/transcription module. Converts audio and video into timestamped, speaker-diarized transcripts. Enables Processors to extract claims from spoken-word primary sources (press conferences, podcasts, diplomatic meetings, etc.).

### AutOSINT Triage (External Module — Design TBD)
Monitors raw incoming signals (breaking news, major events) and prioritizes for Analysts to investigate. Plug-and-play with Engine. Not yet designed.

### UX Layer (Future — Not Yet Designed)
User-facing interfaces and the Monitor module (matching assessments to user interests and delivering intelligence). Entirely separate from the Engine. Designed and built after the Engine is functional.

---

## 4. AutOSINT Engine

### 4.1 Operational Model

The system is **demand-driven, not continuous**. Like a car engine with an ignition and throttle:

- **Ignition**: something triggers an investigation (a prompt, a triage alert, a user request)
- **Throttle**: the Analyst decides how deep to go, how much understanding to build
- **Byproduct**: the knowledge graph grows as a byproduct of investigation, not as a goal

The system does NOT try to maintain comprehensive world understanding. It does NOT continuously monitor and update. It activates when given something to investigate and builds understanding only to the degree the investigation requires. Dense areas of the graph represent topics that have been heavily investigated. Sparse or empty areas represent things that have never been relevant to an investigation. That's fine.

### 4.2 Two Data Stores

#### Knowledge Graph (Neo4j)

World knowledge only. Contains three primitives: entities, relationships, and claims. This is what the Analyst reads from and what Processors write to. It represents the accumulated understanding built through all investigations.

The Knowledge Graph does **NOT** contain assessments. Assessments are analytical products, not world knowledge.

#### Assessment Store (PostgreSQL + pgvector)

Analytical products. A separate, simpler database for the Analyst's assessments. Also stores orchestrator state (investigation records, work order tracking).

**One-directional dependency**: the Assessment Store references graph entities and claims by ID. The Knowledge Graph does NOT reference assessments. The graph doesn't know the Assessment Store exists.

**Access patterns:**
- **Analyst**: queries prior assessments during investigations ("have I assessed this before?") via semantic search
- **UX Layer / Monitor** (future): matches assessments against user interests, delivers intelligence
- **Curation**: browse, search, archive assessments

**Data flow:**
```
Knowledge Graph (entities, relationships, claims)
    |
    v
Analyst reads graph via tool access
    |
    v
Analyst writes assessments to Assessment Store
    |
    v
[Future] Monitor reads Assessment Store, matches user interests, delivers
```

### 4.3 Storage Primitives

#### Entities (in Knowledge Graph)

**Minimal universal schema:**
- `id` — system-generated unique identifier
- `canonical_name` — primary name
- `aliases` — alternative names (list)
- `kind` — loose descriptive label ("organization", "person", "country", "resource", "facility", etc.). This is NOT a rigid type that determines schema. It's a human-readable hint. The kind does not control what fields exist.
- `summary` — LLM-generated living summary, periodically refreshed. Quick-reference orientation, not analysis.
- `last_updated` — timestamp of last modification

**Additional properties**: the LLM can attach whatever properties it deems relevant. Some entities get 2 additional properties, some get 20. That's the LLM's editorial judgment. Properties are freeform key-value pairs.

**Entities hold current state only.** History lives in claims. When an entity changes (rebrand, leadership change, acquisition), the entity record is updated to reflect current reality. The change itself is captured as a claim.

**Stub entities:** When a Processor encounters a referenced entity it can't fully flesh out, it creates a stub — explicitly flagged as a stub — with just name, kind, and ID. Claims can reference stubs immediately. Stubs signal graph sparseness: "something exists here but hasn't been investigated." Future investigations or Processor work can flesh out stubs into full entities.

**External identifiers:** Where stable external IDs exist, entities carry them as properties:
- Wikidata QIDs
- Stock tickers
- ISO country codes
- LEI codes (Legal Entity Identifiers)
- Other domain-specific identifiers

These are NOT the primary ID but serve as strong deduplication signals.

**Deduplication:** The Processor searches existing entities by name and aliases before creating new ones. This is the critical quality gate — entity resolution below 85% accuracy degrades the entire system. Cascading approach:
1. Fast string matching (exact and fuzzy) for obvious cases
2. Embedding similarity for semantic matching
3. LLM judgment for the hard tail (ambiguous cases)

#### Relationships (in Knowledge Graph)

First-class objects connecting entities:

- `source_entity` — one end of the relationship
- `target_entity` — other end
- `description` — **natural language, freeform**. Example: "TSMC supplies approximately 40% of Apple's A-series chip production." There are NO enumerated relationship types. The description carries the meaning. Relationships are searched semantically, not by category.
- `direction` — directional or bidirectional
- `weight` / `strength` — numeric significance signal
- `confidence` — how certain the system is about this relationship
- `timestamp` — when this relationship was established/last confirmed

#### Claims (in Knowledge Graph)

How new information enters the system. Claims are the raw input that the Analyst reasons over.

**Schema:**
- `source_entity` — which entity (a source/publication) produced this claim (linked to graph)
- `published_timestamp` — when the information was published/occurred in the real world
- `ingested_timestamp` — when the system added this claim to the graph
- `referenced_entities` — which entities this claim is about (linked to graph)
- `content` — the information itself (LLM-decided format and depth)
- `raw_source_link` — URL/reference back to the original document
- `attribution_depth` — primary source vs secondhand

**Key principles:**

**Claims are units of information, not text.** A 50-page SEC filing (dense with factual data) produces many claims. A 1,500-word news article might produce only 3. Claims scale with information density, not word count. The Processor's job is to reduce documents to their essential informational content while preserving richness — extracting the signal, discarding the filler (narrative, scene-setting, restatement of known context, opinion framing).

**Claims carry attribution depth.** A primary source claim (the actual statement, the actual filing, the actual data) is fundamentally different from a secondhand claim. When a source reports another source's statement without direct quotes, the claim must note the intermediary:

- Primary: "In SEC filing dated [date], TSMC reported revenue of $23.5B"
- Secondhand: "*According to Reuters,* Russia's foreign ministry described the sanctions as economic warfare"

The Analyst uses attribution depth to judge when it needs to seek primary sources. Secondhand claims are useful for awareness but should not be the sole basis for assessments without corroboration or primary source verification.

**Dual timestamps are critical.** A Processor might process a 6-month-old article today. The published timestamp (6 months ago) tells the Analyst how current the information is. The ingested timestamp (today) tells when the system became aware. The Analyst is guided to consider temporal relevance per-claim, per-topic: "is information from this long ago still relevant for this particular topic?" A country's borders from 6 months ago — probably still valid. A company's CEO from 6 months ago — might have changed.

**Changes are claims.** When entity state changes (rebrand, leadership change, acquisition), the Processor both updates the entity record (current state) AND extracts a claim capturing the change (history). The claim preserves what changed, when, according to whom. The entity always reflects current reality.

#### Assessments (in Assessment Store)

The Analyst's analytical products. Distinct from claims — these are the system's synthesis, not raw information.

**Contents:**
- Conclusions with explicit reasoning
- Confidence levels (high, moderate, low — with explanation of why)
- References to supporting graph entities and claims by ID
- Intelligence gaps — what the Analyst doesn't know and couldn't find
- Competing hypotheses with relative likelihood assessments
- Forward-looking indicators — what to watch for that would change the assessment

**Assessments are always produced.** Even when the investigation yields insufficient data or irreconcilable contradictions, the Analyst produces an honest assessment stating its limitations:
- "We assess with low confidence that X, based on limited sourcing..."
- "Key intelligence gap: no reliable primary sources on Y"
- "Sources conflict on Z — Source A claims [this], Source B claims [that]. We cannot adjudicate with available information."

An assessment that honestly says "I don't know, and here's specifically what I don't know and why" is a complete, valuable product.

### 4.4 Database Schemas

#### Neo4j Knowledge Graph Schema

**Minimum version: Neo4j 5.18+** (required for vector indexes on relationship properties).

**Node: Entity**
```
(:Entity {
  id:             String,       // system-generated UUID
  canonical_name: String,
  aliases:        [String],
  kind:           String,       // loose descriptive label
  summary:        String,       // LLM-generated living summary
  is_stub:        Boolean,
  last_updated:   DateTime,
  embedding:      [Float],      // vector for semantic search (name + summary)
  // ... freeform properties the LLM adds
  // ... external identifiers (wikidata_qid, stock_ticker, iso_code, etc.)
})
```

**Node: Claim**
```
(:Claim {
  id:                  String,
  content:             String,
  published_timestamp: DateTime,
  ingested_timestamp:  DateTime,
  raw_source_link:     String,
  attribution_depth:   String,   // "primary" or "secondhand"
  embedding:           [Float],  // vector for semantic search (content)
})
```

**Edges:**
- `(:Entity)-[:PUBLISHED]->(:Claim)` — source entity (publication/outlet) that produced the claim
- `(:Claim)-[:REFERENCES]->(:Entity)` — entities the claim is about (one claim, many referenced entities)
- `(:Entity)-[:RELATES_TO]->(:Entity)` — relationships between entities, with properties:

```
[:RELATES_TO {
  id:            String,
  description:   String,       // freeform natural language, semantically searched
  weight:        Float,
  confidence:    Float,
  bidirectional: Boolean,
  timestamp:     DateTime,
  embedding:     [Float],      // vector for semantic search (description)
}]
```

For bidirectional relationships, one edge is stored with `bidirectional: true`; query tools traverse in both directions.

**Indexes:**

| Index Type | Target | Property | Purpose |
|---|---|---|---|
| Full-text | :Entity | canonical_name, aliases | Processor dedup, keyword search |
| Vector | :Entity | embedding | Semantic entity search |
| Uniqueness | :Entity | id | Integrity |
| Range | :Entity | kind | Filtering by entity type |
| Range | :Entity | last_updated | Temporal queries |
| Full-text | :Claim | content | Keyword claim search |
| Vector | :Claim | embedding | Semantic claim search |
| Range | :Claim | published_timestamp | Temporal filtering/sorting |
| Range | :Claim | ingested_timestamp | Temporal filtering |
| Uniqueness | :Claim | id | Integrity |
| Full-text | :RELATES_TO | description | Keyword relationship search |
| Vector | :RELATES_TO | embedding | Semantic relationship search |

**What gets embedded:**
- Entity: canonical_name + summary, concatenated
- Claim: content
- Relationship: description

Embeddings computed at write time via embedding API, stored as node/relationship properties.

#### PostgreSQL Schema (Assessment Store + Orchestrator State)

```sql
-- Analytical products
CREATE TABLE assessments (
    id               UUID PRIMARY KEY,
    investigation_id UUID NOT NULL REFERENCES investigations(id),
    content          JSONB NOT NULL,     -- structured assessment (schema TBD with prompt engineering)
    confidence       TEXT NOT NULL,      -- high / moderate / low
    entity_refs      JSONB NOT NULL,     -- array of Neo4j entity IDs
    claim_refs       JSONB NOT NULL,     -- array of Neo4j claim IDs
    embedding        vector,             -- pgvector, for semantic search
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Investigation lifecycle tracking
CREATE TABLE investigations (
    id                      UUID PRIMARY KEY,
    prompt                  TEXT NOT NULL,
    status                  TEXT NOT NULL,   -- pending, active, completed, failed
    parent_investigation_id UUID REFERENCES investigations(id),
    cycle_count             INT NOT NULL DEFAULT 0,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at            TIMESTAMPTZ
);

-- Persistent work order records
CREATE TABLE work_orders (
    id                  UUID PRIMARY KEY,
    investigation_id    UUID NOT NULL REFERENCES investigations(id),
    objective           TEXT NOT NULL,
    status              TEXT NOT NULL,   -- queued, processing, completed, failed
    priority            INT NOT NULL DEFAULT 0,
    referenced_entities JSONB,           -- Neo4j entity IDs for context
    source_guidance     JSONB,           -- directional hints about where to look
    processor_id        TEXT,            -- which processor handled this
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at        TIMESTAMPTZ
);

-- Indexes
CREATE INDEX idx_assessments_embedding ON assessments
    USING ivfflat (embedding vector_cosine_ops);
CREATE INDEX idx_assessments_investigation ON assessments(investigation_id);
CREATE INDEX idx_investigations_status ON investigations(status);
CREATE INDEX idx_work_orders_investigation ON work_orders(investigation_id);
CREATE INDEX idx_work_orders_status ON work_orders(status);
```

Entity/claim refs stored as JSONB arrays (not join tables) — these are cross-database references to Neo4j IDs with no FK constraint possible. Primary assessment query pattern is semantic search, not entity-based lookup. Assessment `content` JSONB schema will be defined alongside Analyst prompt engineering.

#### Redis Schema (Work Order Queue)

**Redis Streams** (not Lists) — consumer groups, acknowledgment, pending entry detection for crash recovery.

**Priority streams:**
```
workorders:high
workorders:normal
workorders:low
```

One consumer group (`processors`) per stream. Processors check high → normal → low.

**Message payload:**
```json
{
  "work_order_id": "uuid",
  "investigation_id": "uuid",
  "objective": "find military installations near Djibouti",
  "referenced_entities": ["entity-uuid-1", "entity-uuid-2"],
  "source_guidance": {"prefer": ["web_search", "government_databases"]}
}
```

**Lifecycle:**
1. Analyst creates work order → Orchestrator writes to PostgreSQL (queued) + XADD to Redis stream
2. Processor XREADGROUP → claims message → PostgreSQL updated (processing)
3. Processor finishes → XACK in Redis → PostgreSQL updated (completed)
4. Processor crash → message stays in pending entries list → Orchestrator reclaims after timeout

### 4.5 LLM Roles

#### Processor

**Role:** Discovery and consolidation worker. The Analyst's hands.

**Two-phase job:**

1. **Discovery** (guided by work order): Finds documents relevant to the work order's objective. Uses the data source hierarchy via AutOSINT Fetch:
   - First: Fetch hardcoded source adapters (structured APIs — most reliable)
   - Second: Fetch raw HTTP (simple web pages not yet programmed as adapters)
   - Last resort: Fetch browser automation (JavaScript-heavy, anti-bot, login-required)

2. **Extraction** (comprehensive, NOT scoped to work order): Extracts ALL key claims from every document found. This is critical — extraction is not limited to what the work order asked about. If the work order was "find military installations near Djibouti" and the Processor finds an article that also mentions a new pipeline project, it extracts the pipeline claim too. The Processor's value is turning full documents into rich, dense, structured claims without filler.

**What the Processor does NOT do:**
- Analyze or assess significance
- Make strategic judgments about what's important
- Decide what to investigate next (that's the Analyst)

**Deduplication:** The Processor's hardest mechanical job. Before creating a new entity, it searches the graph for existing matches. Cascading approach (string matching → embedding similarity → LLM judgment).

**Concurrency:** Multiple Processors run in parallel on different work orders. Could use a faster/cheaper LLM model since the task is extraction, not deep reasoning.

**Concurrency conflicts:** Two Processors might discover the same entity simultaneously. Handled via check-and-create with conflict resolution. Claim writes are append-only (no conflicts — multiple claims about the same thing from different sources is by design).

#### Analyst

**Role:** Central intelligence actor. The brain of the system.

**Self-regulating feedback loop:**

1. Receives investigation prompt (from Triage, user request, or scheduled review)
2. **Self-serves context via tool access** — queries the graph, queries the Assessment Store for prior assessments, queries AutOSINT Geo for geographic context. The Analyst drives its own retrieval based on its own reasoning. The orchestrator does NOT pre-query or assemble context.
3. Identifies gaps in understanding → creates specific, atomic work orders
4. Processors deliver → new claims land in graph
5. Analyst reviews new claims (via tool access to graph): "Do I now know enough to assess? Can I identify key actors, competing explanations, likelihoods, and what would change my assessment?"
6. If no → "What specifically am I still missing?" → creates more targeted work orders → back to step 4
7. If yes → produce assessment → write to Assessment Store

**The feedback loop IS the depth control.** Early work orders are broader ("find military installations near Djibouti"). Later ones are precise because the Analyst has seen the landscape ("find details on the 2017 agreement between China and Djibouti regarding the PLA Support Base lease terms"). The Analyst stops when it judges it knows enough. No external budget or termination condition — empirical observation will tell us if guardrails are needed.

**Temporal awareness:** The Analyst is guided to consider temporal relevance for each claim: "is information from this long ago still relevant for this particular topic?" It can sort and filter claims by published timestamp using its query tools.

**Model requirements:** Needs the most capable model available. This is where reasoning quality directly affects output quality.

### 4.6 Work Order System

Work orders are **discovery directives**, not extraction scopes. They direct WHERE to look and WHAT to look for, not what to extract from found documents (extraction is always comprehensive).

**Schema:**
- **Objective** — what to find. A search directive, not an analytical question. "Find military installations near Djibouti" — NOT "investigate Djibouti's strategic significance."
- **Referenced entities** — existing graph entities this relates to, for linking and deduplication
- **Source guidance** — where to look: specific Fetch source adapters, web search, specific source types. Directional, not prescriptive.
- **Priority** — urgency level

**Explicitly NOT in a work order:**
- Investigation context (why this matters)
- Analytical framing
- Extraction guidance

The Processor doesn't need to know WHY — it finds documents matching the objective and extracts all key claims from them.

**Centralized queue (Redis)** enables tracking, parallel execution across multiple Processors, and work order deduplication (to prevent redundant work when multiple Analysts create similar orders).

### 4.7 Orchestration

The **orchestrator** is deterministic Rust code — not an LLM. It is a **session manager and work dispatcher**. It does not make analytical decisions, assemble context, or decide what's relevant.

#### Investigation State Machine

**States:**

```
PENDING → ANALYST_RUNNING → PROCESSING → ANALYST_RUNNING → ... → COMPLETED
               ↓    ↑            ↓    ↑
            FAILED   |         FAILED  |
               ↓     |            ↓    |
           SUSPENDED -+       SUSPENDED-+
```

- **`PENDING`** — Investigation record created, not yet started.
- **`ANALYST_RUNNING`** — Analyst agentic session in progress.
- **`PROCESSING`** — Work orders dispatched, Processors working.
- **`SUSPENDED`** — Paused due to hard dependency failure. Persisted in PostgreSQL with reason, timestamp, and resume point. Not failed (retryable), not active (can't proceed). Survives Engine restarts.
- **`COMPLETED`** — Assessment produced. Terminal.
- **`FAILED`** — Unrecoverable error. Terminal, but still produces a partial assessment (see below).

**Transitions:**

| From | To | Trigger |
|---|---|---|
| PENDING | ANALYST_RUNNING | Orchestrator starts Analyst session |
| ANALYST_RUNNING | PROCESSING | Analyst created work orders. Orchestrator dispatches to Redis, increments cycle_count. |
| ANALYST_RUNNING | COMPLETED | Analyst called `produce_assessment`. Assessment written to store. |
| ANALYST_RUNNING | ANALYST_RUNNING | Analyst session ended with no work orders and no assessment (empty session). Retry once. Second empty session → force final assessment mode. |
| ANALYST_RUNNING | FAILED | Unrecoverable error (LLM API consistently down, repeated session failures). |
| ANALYST_RUNNING | SUSPENDED | Hard dependency circuit opens during Analyst session. |
| PROCESSING | ANALYST_RUNNING | All work orders for this cycle resolved (completed or permanently failed). Start new Analyst session. |
| PROCESSING | FAILED | Two consecutive cycles where ALL work orders failed. |
| PROCESSING | SUSPENDED | Hard dependency circuit opens during processing. |
| SUSPENDED | ANALYST_RUNNING | Dependency recovers (circuit closes). Fresh Analyst session — graph is the memory. |
| any active | COMPLETED | Max cycles reached → force final Analyst session with modified prompt. Not a failure — resource-bounded completion. |

**SUSPENDED persistence** (additional columns on investigations table):
```sql
suspended_reason  TEXT,          -- 'neo4j_unavailable', 'llm_api_down', etc.
suspended_at      TIMESTAMPTZ,
resume_from       TEXT,          -- 'analyst' or 'processing'
```

**Engine restart recovery:** On startup, the Orchestrator queries PostgreSQL for non-terminal investigations. SUSPENDED → check dependency health, resume if available. ANALYST_RUNNING/PROCESSING → Engine crashed mid-operation, treat as suspended. PostgreSQL is the durable source of truth; the Orchestrator reconstructs working state from it.

**Work order sub-states:**

```
QUEUED → ASSIGNED → COMPLETED
                  → FAILED → QUEUED (retry once)
                           → FAILED (permanent)
```

The Orchestrator waits for all work orders in a cycle to resolve (completed OR permanently failed), then starts the next Analyst cycle regardless of partial failures. The Analyst sees the graph state and can re-request data it's missing.

#### Processor Liveness: Heartbeats

Processors send periodic heartbeats while working, rather than relying on fixed timeouts. A Processor blocked on a long operation (e.g., waiting 30+ minutes for Scribe to transcribe a 3-hour video in a foreign language) continues heartbeating the entire time.

- Processor writes to Redis key `processor:{id}:heartbeat` with a short TTL (e.g., 60 seconds), refreshed periodically.
- Orchestrator checks for expired heartbeat keys. Expired = Processor dead → reclaim its work order from Redis pending entries list, re-queue.
- Decouples "how long does the work take" (unbounded) from "is the Processor alive" (heartbeat interval).

#### Investigation History (Anti-Dedup)

No algorithmic work order deduplication. Instead, the Analyst has visibility into its own investigation history via tool access (`get_investigation_history`). It can see what was requested in prior cycles, what succeeded, what failed, and how many claims were produced. The Analyst naturally avoids redundant requests because it has the context to make that judgment. If it re-requests something, it has a reason.

This aligns with the core design philosophy: the LLM makes the judgment, we provide the information. Max cycles is the safety net for genuine loops.

#### Failed Investigations

A `FAILED` investigation still produces a partial assessment. If any graph data was accumulated before failure, the Orchestrator runs one final Analyst session with a modified prompt: "Produce the best assessment you can with available information, noting all gaps, limitations, and failures encountered." Intelligence reports are never expected to be complete — they are as complete as possible given available information. A partial assessment with honest limitations is a valid product.

#### Safety Limits

| Limit | Default | On trigger |
|---|---|---|
| Max cycles per investigation | Configurable (e.g., 10) | Force final Analyst session: "This is your final cycle. Produce an assessment with available information." |
| Max turns per Analyst session | Configurable (e.g., 50 tool calls) | End session. If no actionable output, treat as empty session (retry once). |
| Heartbeat TTL | Configurable (e.g., 60 seconds) | Expired = Processor considered dead, work order reclaimed. |
| Consecutive all-fail cycles | 2 | Transition to FAILED (with partial assessment attempt). |
| Max work orders per cycle | Configurable (e.g., 20) | Prevents runaway discovery in a single cycle. |

All limits are configurable at runtime via the config system.

#### Concurrent Investigations

Each investigation is an independent state machine. The Orchestrator manages a collection concurrently. They share the Processor pool — work orders from different investigations go into the same Redis streams. Processors are investigation-agnostic; they just process work orders. The `investigation_id` on each work order tracks which investigation it belongs to.

#### Orchestrator Responsibilities

- Investigation lifecycle state management
- Work order dispatch and completion tracking
- Processor pool management and heartbeat monitoring
- Assessment routing to Assessment Store
- Providing tool access to Analyst/Processor sessions
- Safety limit enforcement

**What the orchestrator does NOT do:**
- Make analytical decisions
- Pre-query the graph or assemble context
- Decide when an investigation is complete (the Analyst decides)
- Decide what's relevant (the Analyst decides)
- Deduplicate work orders (the Analyst avoids redundancy via investigation history)

#### Multi-Analyst Investigations (Future)

For large investigations, a planning Analyst call can decompose the investigation into parallel sub-investigations. Multiple Analysts run concurrently, all sharing the same graph. A synthesis Analyst produces a final assessment built from the child assessments. Child Analysts produce normal assessments — no special types. The orchestrator manages parent-child investigation records and triggers synthesis on completion.

This is NOT built first. Build single-Analyst investigations first. Add multi-Analyst decomposition when empirical data shows where single Analysts struggle. The decomposition guidance (how big is "too big," what are right-sized sub-investigations) can only be determined from experience.

### 4.8 Search & Retrieval

Three search modes, all native in Neo4j:

1. **Graph traversal** — "starting from this entity, what's connected?" For investigating known entities and following relationship chains outward.
2. **Semantic/vector search** — "find things conceptually related to this topic." For discovery, finding unexpected connections, and searching across unconnected parts of the graph. Uses Neo4j's native vector indexes.
3. **Keyword/full-text** — exact entity name matching, specific terms. For Processor deduplication and precise lookups. Uses Neo4j's native full-text indexes.

**LLM-to-graph interface uses predefined retrieval functions (function calling), NOT raw Cypher query generation.** Research shows Text2Cypher is unreliable for complex queries. The LLM calls structured tool functions with parameters; the tool layer translates those into database queries. More reliable, more testable.

**Query tools support temporal filtering and sorting.** The Analyst can request "claims about Entity X from the last 30 days" or "claims older than 6 months" or sort claims by published timestamp.

**Who searches what:**

| Who | What | Mode | Purpose |
|-----|------|------|---------|
| Analyst | Entities/claims around a topic | Graph traversal + semantic | Investigation context |
| Analyst | Unexpected connections | Semantic search across graph | Discovery |
| Analyst | Prior assessments | Semantic search over Assessment Store | Analytical continuity |
| Processor | Existing entities | Keyword/name + embedding similarity | Deduplication |
| [Future] Monitor | Assessments vs user interests | Semantic matching over Assessment Store | User delivery |

### 4.9 Tool Layer

LLMs interact with the system through **predefined functions** (not raw queries). Interfaces are hardcoded; usage is LLM-decided.

#### Agentic Loop Mechanics

The core cycle for both Analyst and Processor sessions:

1. Build messages: system prompt + conversation history + tool definitions
2. Send to LLM API with tool definitions
3. Response contains either:
   - **Text only** → session complete
   - **Tool call(s)** → execute each, append `tool_result` to conversation history, back to step 1
4. Repeat until session termination

The Engine's `llm/` module implements this loop as a thin wrapper — tool calling, response parsing, session management, error handling, retries.

#### Session Model

Each Analyst cycle is a **fresh session** — same investigation prompt, same tools, clean conversation history. The Analyst's memory between cycles is the graph itself. If it requested data in cycle 1, those claims are now in the graph. In cycle 2 it queries the graph, finds them, reasons from there.

Benefits of fresh sessions per cycle:
- No context window accumulation across cycles
- No risk of anchoring on prior reasoning instead of re-evaluating with new data
- Graph is the single source of truth, not conversation history

The Analyst can query the Assessment Store for prior assessments on related topics and query work order history in PostgreSQL to see what's already been requested — orientation without conversation continuity.

#### Session Termination

**Analyst sessions** end in one of two ways (mutually exclusive):
- Calls `create_work_order` (one or more times) → "I need more data." Orchestrator dispatches work orders, starts new cycle after completion.
- Calls `produce_assessment` → "I'm done." Writes to Assessment Store, marks investigation complete.

These are mutually exclusive by design. The Analyst system prompt should align the Analyst to naturally reason toward one or the other — framed as a decision point, not a rule to follow.

Safety limit: max turns per session. If reached, Orchestrator flags the investigation for review.

**Processor sessions** end when the LLM stops calling tools (text-only response). The Processor has no special termination tools — it extracts claims, creates entities/relationships, and stops when done.

#### Tool Definitions in Config

Tool schemas live in `config/tools/` as JSON files, loaded at runtime:

```
config/tools/
├── analyst/           # tool schemas for Analyst sessions
│   ├── search_entities.json
│   ├── search_claims.json
│   ├── traverse_relationships.json
│   └── ...
└── processor/         # tool schemas for Processor sessions
    ├── search_entities.json
    ├── create_entity.json
    ├── create_claim.json
    └── ...
```

Each file is the JSON schema the LLM sees (name, description, parameters). The Engine loads these at startup and passes them to the LLM API. The Rust `tools/` module has a handler registry mapping tool names to implementation functions. Schema describes the interface; Rust code implements the behavior. Tool descriptions can be iterated without recompiling; handler behavior changes require recompile.

#### Analyst Tools

```
search_entities(query, kind?, limit?)
    → [{id, canonical_name, kind, summary, score}]

get_entity(entity_id)
    → {full entity with all properties}

traverse_relationships(entity_id, direction?, description_query?, min_weight?, limit?)
    → [{relationship, connected_entity}]

search_relationships(query, limit?)
    → [{relationship, source_entity, target_entity}]

search_claims(query?, entity_id?, source_entity_id?, published_after?,
              published_before?, attribution_depth?, sort_by?, limit?)
    → [{id, content, published_timestamp, source, attribution_depth, referenced_entities}]

search_assessments(query, limit?)
    → [{id, confidence, summary, created_at}]

get_assessment(assessment_id)
    → {full assessment content}

create_work_order(objective, referenced_entities?, source_guidance?, priority?)
    → {work_order_id}

produce_assessment(content, confidence, entity_refs, claim_refs)
    → {assessment_id}

get_investigation_history()
    → [{cycle, work_orders: [{objective, status, claims_produced_count}]}]
    Current investigation's history. Prevents redundant work orders
    without algorithmic dedup — Analyst sees what was already requested.

query_geo(query_type, parameters)
    → structured text from AutOSINT Geo

list_fetch_sources()
    → [{id, name, description, capabilities}]
```

#### Processor Tools

```
search_entities(query, limit?)
    → [{id, canonical_name, aliases, kind, score}]
    For deduplication before creating entities.

create_entity(canonical_name, aliases, kind, summary?, is_stub?, properties?)
    → {entity_id}

update_entity(entity_id, updates)
    → success/failure

create_claim(source_entity_id, content, published_timestamp,
             referenced_entity_ids, raw_source_link, attribution_depth)
    → {claim_id}

create_relationship(source_entity_id, target_entity_id, description,
                    weight?, confidence?, bidirectional?, timestamp?)
    → {relationship_id}

update_relationship(relationship_id, updates)
    → success/failure

fetch_source_catalog()
    → [{id, name, description}]

fetch_source_query(source_id, query_params)
    → {content, metadata}

fetch_url(url)
    → {content, metadata}

browse_url(url)
    → {rendered_content, metadata}
    Simple render, no interaction.

browser_open(url)
    → {rendered_content, session_id}
    Starts interactive WebSocket session.

browser_click(session_id, selector)
    → {rendered_content}

browser_fill(session_id, selector, value)
    → {rendered_content}

browser_scroll(session_id, direction?)
    → {rendered_content}

browser_close(session_id)
    → confirmation

submit_transcription(url, platform?, context?, diarization?)
    → {job_id}

get_transcription(job_id, timeout?)
    → transcription result (blocking long-poll)
```

Browser tools return rendered DOM/text content, not screenshots. The Processor reasons over HTML structure to identify selectors. All write tools (create_entity, create_claim, create_relationship) compute and store embeddings at write time.

IO contracts for all tools will be refined during implementation to ensure seamless integration.

---

## 5. AutOSINT Fetch

### 5.1 Problem It Solves

Engine Processors need to discover and retrieve information from across the internet. Sources vary wildly: structured APIs with authentication and rate limits, government databases, simple web pages, JavaScript-heavy SPAs, anti-automation platforms, authenticated sources. The Processor shouldn't have to understand any of this. AutOSINT Fetch abstracts away the complexity of HOW to get information from WHERE it lives.

**ALL data retrieval goes through Fetch.** The Engine never fetches directly. This centralizes caching, rate limiting, and monitoring in one place.

### 5.2 Architecture

Fetch is a **source registry with adapters** plus browser automation capability.

#### Source Adapters

A hardcoded source is a **source adapter** — code within Fetch that knows how to talk to one specific data provider. Each adapter handles:

- **Authentication**: API keys, OAuth tokens, session management
- **Query translation**: turning a general request into the source-specific format
- **Rate limiting**: respecting per-source rate limits, queuing requests
- **Pagination**: handling multi-page results
- **Response normalization**: converting source-specific responses into a standard format
- **Coverage metadata**: description of what this source provides

Adding a new source means writing a new adapter and deploying an updated Fetch container. The Engine doesn't change — it just sees a new entry in the source catalog.

#### Planned Source Adapters (Initial)

- Web search APIs (Google Search, Bing, Brave Search)
- SEC EDGAR (US financial filings)
- NewsAPI (news articles)
- Government databases (UN data, White House transcripts, CDC, NASA)
- Reddit API
- RSS feed aggregator (wire services, major outlets)
- Hacker News API
- Central bank publications (Fed, ECB)
- Court filing databases
- Academic source APIs

(This list will grow continuously. Each new source is an adapter addition to Fetch.)

#### Data Source Hierarchy

Processors default to the most reliable path first:

1. **Fetch hardcoded source adapters** — structured APIs, databases. Most reliable, structured data.
2. **Fetch raw HTTP** — simple page fetches for sources not yet programmed as adapters.
3. **Fetch browser automation** — Playwright (Node.js) for JavaScript-heavy, anti-bot, or login-required sources. Last resort.

### 5.3 Caching

URL-keyed with a TTL. If a document at a URL was fetched within the last N hours, return the cached version.

**When cache fires:**
- Same document referenced across multiple work orders
- Re-investigation of a topic over time
- Multiple sources citing the same primary document

Cache applies to fetched document content, not to source API query results (those vary by parameters).

### 5.4 Browser Automation

Browser automation runs as a **separate Node.js + Playwright sidecar** container, not inside the Rust Fetch service. Fetch's Rust core handles source adapters, caching, rate limiting, and raw HTTP. When browser rendering or interaction is needed, Fetch delegates to the sidecar.

**Sidecar architecture:**

- **Always-running** Docker container managed by Docker Compose. Headless Chromium idles at ~100-200MB. No cold start penalty when browser automation is needed.
- **Concurrent browser contexts.** Multiple requests handled simultaneously via isolated Playwright browser contexts (separate cookies, sessions, storage) within a single browser instance. Concurrency cap (e.g., 4-6 contexts) to manage memory pressure. Requests beyond the cap queue until a context frees up.
- **Two interaction modes:**
  - **Simple render (HTTP):** Navigate to URL, wait for JavaScript, return rendered DOM. Covers the common case — pages that need JS to render but don't need interaction. Single request/response via Fetch's `/browse` endpoint.
  - **Interactive session (WebSocket):** Bidirectional, continuous transaction between the Processor and the sidecar (proxied through Fetch). The Processor controls navigation step-by-step — it sees the rendered page, reasons about it, sends the next command (click, fill, scroll), sees the result, reasons again. The Processor doesn't know what's on the page until it sees it, so interaction cannot be pre-programmed. The Engine's tool layer exposes this as browser tools (`browser_open`, `browser_click`, `browser_fill`, `browser_close`) and manages the WebSocket connection underneath.

**Security isolation:** Browser sidecar runs in its own container with limited network access. No direct graph access. No access to Engine internals.

### 5.5 API

```
GET  /health                   → service health

GET  /sources                  → catalog of available source adapters
                                 Returns: [{id, name, description, capabilities}]

POST /sources/{id}/query       → query a specific source adapter
                                 Input: { query parameters specific to source }
                                 Returns: { results in normalized format }

POST /fetch                    → raw HTTP fetch
                                 Input: { url, options }
                                 Returns: { content, metadata (status, content_type, etc.) }

POST /browse                   → simple browser-automated fetch (render only)
                                 Input: { url, options (wait_for, timeout) }
                                 Returns: { content, metadata }

WS   /browse/session           → interactive browser session (WebSocket)
                                 Bidirectional: Processor sends commands
                                 (navigate, click, fill, scroll, close),
                                 sidecar returns rendered content after each action.
                                 Session persists until explicitly closed.
```

All responses return content in a normalized format that the Processor can extract claims from.

---

## 6. AutOSINT Geo

### 6.1 Problem It Solves

The Analyst needs to reason about physical geography — terrain, borders, elevation, rivers, chokepoints, infrastructure, resource deposits. Research shows LLMs cannot do spatial reasoning from parametric memory reliably (max 67% accuracy across 30 models benchmarked). They can't look at a map. Their geographic knowledge is patchy and unevenly distributed.

AutOSINT Geo is a **geographic oracle** that translates physical geography into structured text the LLM can reason over. It answers specific geographic questions — it does not provide events, analysis, or anything temporal.

### 6.2 What Geo Knows About

- **Borders**: national, regional, maritime boundaries, disputed territories
- **Terrain**: mountains, deserts, plains, forests, wetlands, traversability
- **Elevation**: topographic data
- **Water**: rivers, lakes, bodies of water, coastlines
- **Climate/weather**: patterns, seasonal variations
- **Cities and populated places**: major and minor
- **Infrastructure**: ports, airports, transportation networks, pipelines
- **Resource deposits**: oil, minerals, arable land, water resources
- **Chokepoints**: straits, canals, mountain passes
- **Shipping lanes**: major maritime routes
- **Military installations**: from OSINT data sources

### 6.3 What Geo Does NOT Do

- Provide events, news, or anything temporal
- Query the Engine's knowledge graph
- Make analytical judgments
- Answer vague queries (queries must be specific geographic questions)

### 6.4 Technology

- **PostGIS** spatial database as the backend
- Geographic data loaded from external datasets (to be researched during module development)
- **Perception scripts**: pre-computed spatial context summaries that present geographic reality in LLM-readable text
- Hot-swappable datasets — geographic data can be updated without code changes

### 6.5 How the Analyst Uses Geo

The Analyst queries Geo with specific geographic questions during investigations:

- Investigating a military situation: "Describe the terrain between Sanaa and Aden, including elevation changes, mountain passes, and river crossings."
- Investigating trade disruption: "What chokepoints does a shipping route from Shanghai to Rotterdam pass through? What are the strait widths and alternative routes?"
- Investigating resource conflict: "Where are known lithium deposits in the Lithium Triangle and what national borders do they cross?"
- Investigating a strike: "Describe the terrain and infrastructure within 50km of [location]. What cities, ports, and military installations are nearby?"

Geo returns **structured text** — named places, descriptive language, relative positions. Not coordinate dumps. Not map images.

**Example perception script response:**
```
Bab el-Mandeb Strait. Connects Red Sea to Gulf of Aden.
Width: 26km at narrowest point.
Bordering nations: Yemen (east), Djibouti (west), Eritrea (northwest).
Major ports within 100km: Port of Djibouti (28km W), Aden (150km E).
Foreign military installations within 200km: Camp Lemonnier (US, Djibouti),
  PLA Support Base (China, Djibouti), Japanese Self-Defense Force base (Djibouti).
Shipping traffic: ~6.2M barrels/day oil transit, ~10% of global seaborne trade.
Active territorial disputes: Yemen-Eritrea (Hanish Islands).
```

### 6.6 Data Sources (To Be Researched)

Specific geographic data sources will be researched during module development. Known candidates:
- OpenStreetMap / Wikidata (open source geographic entities, boundaries, infrastructure)
- Natural Earth (public domain map data — country boundaries, physical features)
- UN LOCODE (ports and logistics locations)
- OSINT military installation databases
- Maritime/shipping lane datasets
- Elevation/terrain datasets (SRTM, etc.)

Note: CIA World Factbook is being discontinued — will need alternative sources for country-level data.

### 6.7 API

```
GET  /health                   → service health

GET  /capabilities             → what types of queries Geo supports

POST /context                  → perception script for a location/feature
                                 Input: { location (name or description) }
                                 Returns: { structured text summary — terrain, borders,
                                           nearby features, infrastructure, climate }

POST /spatial/nearby           → what's within X km of a location
                                 Input: { location, radius_km, feature_types[] }
                                 Returns: { features with distances and descriptions }

POST /spatial/distance         → distance and terrain between two locations
                                 Input: { from, to }
                                 Returns: { distance, terrain description, features between }

POST /spatial/route            → what a path between two points crosses
                                 Input: { origin, destination }
                                 Returns: { terrain, borders crossed, chokepoints,
                                           bodies of water, infrastructure along route }

POST /terrain                  → terrain description for an area
                                 Input: { location/region }
                                 Returns: { elevation, terrain type, traversability,
                                           natural features }

POST /borders                  → border information
                                 Input: { country/region }
                                 Returns: { shared borders, lengths, terrain at borders,
                                           disputed boundaries }

POST /features                 → what exists in a region
                                 Input: { region, feature_types[] (rivers, cities, ports, etc.) }
                                 Returns: { features with descriptions and relative locations }
```

All responses are **structured text** designed for LLM consumption.

---

## 7. AutOSINT Scribe

### 7.1 Problem It Solves

A massive amount of primary source material in geopolitics is spoken, not written: press conferences, UN speeches, diplomatic meetings, congressional hearings, central bank announcements, podcasts featuring key figures, White House briefings. If the Engine can only process text, it's blind to these primary sources.

Scribe converts multimedia into timestamped, speaker-diarized transcripts that Processors can extract claims from.

### 7.2 Platform Support

Scribe natively supports specific platforms and knows how to download/access media from them. It exposes a catalog of supported platforms so the Processor knows what it can handle.

**Initial platforms:**
- YouTube
- Spotify (podcasts)
- Direct audio/video URLs (MP3, MP4, WAV, etc.)

(Platform support grows over time by adding new platform adapters.)

The Processor can specify the platform explicitly or let Scribe auto-detect from the URL. Pre-specifying is preferred.

### 7.3 Speaker Diarization

Scribe performs **mechanical speaker diarization** — identifying distinct speakers and labeling them (Speaker 1, Speaker 2, Speaker 3). It uses diarization technology (e.g., pyannote.audio) to separate speakers.

**Scribe does NOT name speakers.** It can't know that Speaker 1 is "Joe Rogan" from audio alone (that would require voice print matching against a database of known voices, which is impractical at our scope).

**How speakers get named:** The Processor sends optional context metadata with the transcription request (title, description, known participants). Scribe passes this through in the response. The Processor (LLM) infers who is who based on context: "This is the Joe Rogan Experience with Rand Paul — Speaker 1 who speaks most and introduces the show is likely Rogan, Speaker 2 is likely Paul."

### 7.4 Transcription Details

**Original language only.** Scribe does NOT translate. It transcribes in whatever language the audio is in. The Processor/LLM handles translation during claim extraction if needed. This preserves the ability to include direct quotes in the original language — critical for accurate attribution.

**Per-segment confidence.** Whisper produces confidence at the segment level. A 3-hour video will have varying confidence — clear studio audio might be 0.96, a section with crosstalk might be 0.72. Each segment carries its own confidence score. The Processor can weight claims based on the confidence of the segment they came from.

**Timecoded segments.** Every segment has start and end timestamps. Claims extracted from transcripts can reference exact moments: "At 1:23:45 of [source], [speaker] stated [X]." This enables verification — someone can go to the exact timestamp to check.

### 7.5 Async Processing

All transcription is job-based. A 3-hour podcast takes minutes to tens of minutes to transcribe. The Processor submits a job and blocks via long-poll until completion.

**Long-poll pattern:** `GET /transcribe/{id}?block=true&timeout=300` — the connection stays open until the job completes or the timeout hits. The Processor makes one call and waits. Simple from the Processor's perspective.

Scribe downloads the media itself (given a URL) — the Processor does not need to download and upload media files.

### 7.6 Technology

- **Python** service (Whisper and audio processing libraries are Python-native)
- **Whisper** (OpenAI open source) for speech-to-text
- **pyannote.audio** or similar for speaker diarization
- Standard audio/video processing libraries (ffmpeg for format handling)

### 7.7 Output Format

```json
{
  "status": "complete",
  "language_detected": "en",
  "duration": "02:58:33",
  "speaker_count": 2,
  "context": {
    "title": "The Joe Rogan Experience #2247",
    "description": "Joe sits down with Senator Rand Paul...",
    "known_participants": ["Joe Rogan", "Rand Paul"]
  },
  "segments": [
    {
      "start": "00:00:00",
      "end": "00:01:23",
      "speaker": "Speaker 1",
      "content": "Welcome back to the podcast. Today I'm sitting down with...",
      "confidence": 0.96
    },
    {
      "start": "00:01:24",
      "end": "00:02:15",
      "speaker": "Speaker 2",
      "content": "Thanks for having me, Joe. I wanted to talk about...",
      "confidence": 0.94
    }
  ]
}
```

### 7.8 API

```
GET  /health                   → service health

GET  /platforms                → list of supported platforms
                                 Returns: [{name, description, accepts}]

POST /transcribe               → submit transcription job
                                 Input: {
                                   url: "https://...",
                                   platform: "youtube" (optional, auto-detected if omitted),
                                   context: {
                                     title: "...",
                                     description: "...",
                                     known_participants: ["...", "..."]
                                   } (optional),
                                   diarization: true/false
                                 }
                                 Returns: { job_id }

GET  /transcribe/{id}          → get job status/results
                                 Supports long-poll: ?block=true&timeout=300
                                 Returns: {
                                   status: "queued" | "processing" | "complete" | "failed",
                                   result: { ... } (on complete, see output format above)
                                 }

DELETE /transcribe/{id}        → cancel a pending job
```

---

## 8. AutOSINT Triage

**Design TBD.** This module monitors raw incoming signals (breaking news, major events) and prioritizes things for Analysts to investigate.

What we know so far:
- It's a plug-and-play external module, separate from the Engine
- It feeds investigation prompts into the Engine
- It may take the form of a timeline the Analyst can observe
- It needs to detect significance in raw information streams (volume spikes, breaking patterns, major entity mentions)
- It could be lightweight (heuristic-based) or LLM-assisted

This will be designed after the Engine is functional. The Engine doesn't depend on Triage — investigations can be triggered manually or through other means.

---

## 9. External Module Pattern

All external modules (Fetch, Geo, Scribe, Triage) follow the same architectural pattern:

- **Rigid interop schema**: HTTP API (REST/JSON). Well-defined request/response contracts.
- **Hot-swappable at runtime**: modules can be updated, restarted, or replaced without stopping the Engine.
- **Independently deployable**: each module is a Docker container. Can be developed, tested, and deployed independently.
- **Language-agnostic**: each module uses whatever language is best for its job. The Engine doesn't know or care what language runs behind the HTTP interface.
- **Dynamic capability discovery**: modules expose their capabilities (supported sources, platforms, query types) so the Engine/LLMs can discover what's available at runtime.

**Module languages:**
| Module | Language | Why |
|--------|----------|-----|
| Engine | Rust | Performance, concurrency, long-term scalability |
| Fetch (core) | Rust | Shares types with Engine via common crate, HTTP client work is natural Rust |
| Fetch (browser sidecar) | Node.js + Playwright | Playwright's native environment; small, focused container |
| Geo | Rust | Shares types with Engine via common crate, PostGIS queries via sqlx |
| Scribe | Python + Whisper | Whisper and audio libraries are Python-native |
| Triage | TBD | TBD |

**Monorepo with shared types:** Engine, Fetch (core), and Geo are all Rust crates in a single Cargo workspace. They share API contract types via a common crate (`autosint-common`), providing compile-time guarantees that services agree on request/response schemas. When a Fetch response type changes, the Engine won't compile until it handles the change.

---

## 10. Tech Stack

### Core Language: Rust

Chosen for long-term scalability over development speed. The system needs to be reliable, concurrent, and long-lived. Rust provides:
- Memory safety without garbage collection
- Excellent async/concurrent performance (tokio)
- Small binary sizes, fast startup (good for microservices)
- No GIL, no GC pauses in long-running services
- Strong type system that catches errors at compile time

LLM integration is straightforward HTTP + JSON (reqwest + serde). No LLM framework needed.

**Prompts and tool schemas are loaded from config files at runtime**, NOT compiled in. This enables rapid iteration on prompt engineering without recompiling.

### Project Structure

Single monorepo. Rust workspace for all Rust crates, separate directories for non-Rust services.

```
autosint/
├── Cargo.toml                  # workspace root
├── docker-compose.yml
├── docs/
│   └── PLAN.md
├── config/                     # runtime config (prompts, tool schemas, limits)
│   ├── prompts/                # Analyst/Processor system prompts
│   ├── tools/                  # tool definitions as JSON schemas
│   └── system.toml             # numeric parameters, model config, timeouts
├── crates/
│   ├── common/                 # autosint-common — shared API contract types,
│   │                           #   config structures, identifiers, error types
│   ├── engine/                 # autosint-engine — core system (single binary)
│   ├── fetch/                  # autosint-fetch — data retrieval service
│   └── geo/                    # autosint-geo — geographic oracle service
├── services/
│   ├── fetch-browser/          # Node.js Playwright sidecar
│   └── scribe/                 # Python transcription service
├── observability/
│   ├── grafana/                # dashboard definitions, datasource config
│   ├── prometheus/             # prometheus.yml (scrape targets)
│   └── loki/                   # loki config
├── CLAUDE.md
└── .gitignore
```

**Engine internal modules** (within `crates/engine/src/`):

```
├── main.rs
├── orchestrator/       # investigation lifecycle, work dispatch
├── analyst/            # Analyst agentic session management
├── processor/          # Processor session management
├── tools/              # tool definitions and execution layer
├── llm/                # thin LLM client wrapper (API calls, parsing)
├── graph/              # Neo4j client and query functions
├── store/              # PostgreSQL client (assessments, investigations)
├── queue/              # Redis client (work orders)
└── config/             # config file loading
```

**`config/` directory** is mounted into the Engine container at runtime. Edit a prompt or tool schema, restart the container — no recompile needed.

### Databases & Infrastructure

| Component | Technology | Why |
|-----------|-----------|-----|
| Knowledge Graph | Neo4j Community Edition (GPLv3) | 3 search modes native (vector, traversal, full-text), best LLM tooling ecosystem |
| Assessment Store + Orchestrator State | PostgreSQL + pgvector | Battle-tested, vector search capability, dual-use |
| Work Order Queue | Redis | Simple, fast, native queue primitives (streams/lists) |
| Embeddings | OpenAI API initially → open source (nomic-embed-text, BGE) later | Simplicity now, cost reduction later |
| Containerization | Docker Compose (dev), single VPS/cloud (prod) | Microservice development, simple deployment |

### Key Rust Crates

| Need | Crate | Maturity |
|------|-------|----------|
| HTTP client (LLM APIs, web fetching) | reqwest | Excellent, production-proven |
| JSON serialization | serde / serde_json | Best-in-class |
| Async runtime | tokio | Industry standard |
| HTTP framework (service APIs) | axum | Excellent, built on tokio |
| Neo4j driver | neo4rs | Usable, less mature than Python equivalent |
| PostgreSQL | sqlx | Mature, async, compile-time checked queries |
| Redis | redis-rs | Mature |
| HTML parsing | scraper | Good |

### LLM Integration

**Direct API calls to Anthropic/OpenAI.** No LangChain, no LlamaIndex. The system is too custom for framework abstractions — our own tool definitions, our own agentic loop patterns, our own orchestration logic. A framework would give us abstractions we'd constantly work around.

Custom thin wrapper handling:
- Tool calling / function calling
- Structured response parsing
- Agentic session management (tool call → execute → return result → next LLM call)
- Error handling and retries

### Infrastructure Estimates

Starting scale: a single well-speced VPS (32-64GB RAM) or small cloud deployment.

- Knowledge Graph (Neo4j): 4-8GB RAM
- Assessment Store + Orchestrator State (PostgreSQL): minimal
- Work Order Queue (Redis): minimal
- Processor instances: 1-2GB each, scale count as needed
- Orchestrator: minimal (512MB)

**The dominant cost is LLM API calls, not infrastructure.**

---

## 11. Error Handling

Cross-cutting concern that touches every module. Errors are expected, not exceptional.

### Dependency Classification

**Hard dependencies** — system cannot function without them:
- Neo4j, PostgreSQL, Redis, LLM API
- Failure → investigation SUSPENDED, critical alert

**Soft dependencies** — system degrades but continues:
- Fetch (Processors can't retrieve new data; Analyst reasons over existing graph)
- Geo (Analyst loses geographic context, notes the gap)
- Scribe (Processors skip multimedia sources, note limitation)

### Errors as Tool Results

When a tool call fails due to a soft dependency, the error is returned as a tool result — NOT a session crash:

```json
{
  "type": "tool_result",
  "content": "Error: AutOSINT Geo is currently unavailable.",
  "is_error": true
}
```

The LLM adapts: works around the limitation, documents the gap. Consistent with the design philosophy — the LLM makes the judgment about how to handle degraded conditions.

Hard dependency failures during a tool call escalate to session failure (the session genuinely can't continue).

### Retry Strategy

Consistent retry pattern across the system via a shared utility:

```rust
struct RetryConfig {
    max_attempts: u32,
    initial_backoff_ms: u64,
    max_backoff_ms: u64,
    backoff_multiplier: f64,
    jitter: bool,
}
```

| Target | Attempts | Initial | Max | Rationale |
|---|---|---|---|---|
| LLM API | 3 | 1s | 30s | Rate limits need longer backoff; respect Retry-After |
| Databases | 3 | 500ms | 10s | Usually recovers quickly or is truly down |
| External modules | 2 | 1s | 5s | Degrade gracefully, don't wait long |

All values runtime-configurable. Exponential backoff with jitter on all retries.

**What does NOT get retried:** authentication failures (alert immediately), constraint violations (log, likely a bug), invalid tool call parameters (return error to LLM for self-correction), context window exceeded (structural, not transient).

### Circuit Breakers

Prevents hammering a dead service:

- **Closed** (normal): requests flow. Failures exceeding threshold → Open.
- **Open**: requests fail immediately. After cooldown → Half-Open.
- **Half-Open**: one probe request. Success → Closed. Failure → re-Open.

Circuit breaker on every external dependency. Thresholds and cooldowns runtime-configurable.

When a hard dependency circuit opens → active investigations transition to SUSPENDED. When circuit closes → Orchestrator resumes SUSPENDED investigations.

### LLM Self-Correction

Malformed tool calls (invalid parameters, missing fields) are returned as error tool results. The LLM self-corrects on its next turn. If 3 consecutive malformed calls occur (configurable), the session ends — the LLM is confused, not self-correcting.

### Error Propagation Layers

1. **Tool handler**: catches error, retries, checks circuit breaker. Returns success or error as tool_result. Most errors handled here.
2. **Agentic session**: hard dependency failure makes session impossible → `SessionResult::Failed(error)`.
3. **Orchestrator**: follows state machine. Session failures → retry or FAILED. Work order failures → retry or permanent failure.
4. **System-level**: all hard dependency circuits open → system degraded. Alerting fires. Investigations SUSPENDED. Auto-recovery when dependencies return.

### Processor Crash Safety

A Processor crash mid-write leaves partial committed writes in Neo4j. When the work order is reclaimed and re-processed:
- Entity dedup catches already-created entities
- Duplicate claims are harmless (append-only, same source/content)
- Partial writes are non-corrupting by design

No special rollback or cleanup needed.

### Embedding Pipeline

Every write to Neo4j (entity, claim, relationship) requires an embedding. The pipeline is **batched, synchronous, with fallback to backfill**.

**Normal flow:**
1. Processor extracts multiple entities/claims/relationships from a document.
2. Collects all texts that need embedding (entity name+summary, claim content, relationship description).
3. One batched embedding API call for all texts.
4. Writes everything to Neo4j in a single transaction — data + embeddings together.

**On embedding API failure** (retries exhausted):
1. Write to Neo4j without embeddings. Flag records with `embedding_pending: true`.
2. Graph stays current — no data loss from embedding outage.
3. Records appear in keyword/full-text search and graph traversal (2 of 3 search modes). Missing from semantic/vector search only.

**Backfill process:**
Background task in the Engine that periodically queries Neo4j for `embedding_pending: true` records, computes embeddings in batch, updates the records. Runs every N minutes (configurable). When the embedding API recovers, the backlog clears automatically.

**Embedding model configuration** (in system.toml):
```toml
[embeddings]
provider = "openai"
model = "text-embedding-3-small"
dimensions = 1536
batch_size = 100
backfill_interval_minutes = 5
```

Provider-swappable (OpenAI initially → open source later) via the same config mechanism as LLM providers.

---

## 12. CI & Testing

### Path-Filtered CI

Monorepo CI runs path-filtered checks — changes to specific directories trigger only the relevant test groups. Exception: the full Rust workspace always builds together (a change to `common` can break downstream crates, and Cargo's incremental compilation makes full-workspace builds fast).

**Rust workspace** (triggers on `crates/**` or `Cargo.*` changes):
- `cargo fmt --check` — formatting gate
- `cargo clippy -- -D warnings` — lint gate
- `cargo test --workspace` — unit tests across all crates
- `cargo build --release` — release build verification

**Node.js sidecar** (triggers on `services/fetch-browser/**` changes):
- Lint + type check
- Unit tests

**Python Scribe** (triggers on `services/scribe/**` changes):
- Linter (ruff)
- pytest unit tests

### Integration Tests

Scoped to database interactions — the riskiest boundaries in the system. Not a general "spin up everything" suite.

**Why databases, specifically:**

- **Neo4j (neo4rs):** Least mature driver in the stack. Vector index searches, full-text index searches, relationship property indexes (5.18+ dependency), multi-hop traversals, entity dedup cascade — subtle Cypher bugs silently degrade the system without crashing. A mocked Neo4j client tells you Rust compiles; it doesn't tell you queries return what you expect.
- **Redis Streams:** Consumer groups, XREADGROUP blocking, XACK, pending entry reclamation — stateful interactions where order and timing matter. Mocks that return "success" don't validate behavioral semantics.
- **PostgreSQL (pgvector):** Lowest risk due to sqlx compile-time query checking, but vector similarity search behavior (cosine distance thresholds, index scan vs sequential scan) is worth validating against real data.

Integration tests run in CI with Neo4j, PostgreSQL, and Redis as service containers (GitHub Actions supports this natively). Run on PR and merge to main, not on every push to a feature branch.

**What is NOT integration-tested in CI:**

- **Module-to-module HTTP APIs.** Well-defined contracts, axum handler testing is straightforward with unit tests, and shared types in `autosint-common` catch contract drift at compile time.
- **LLM API calls.** Expensive, non-deterministic, slow. LLM integration tested with mocked responses in CI. Real API validation is manual or scheduled, not gating.
- **Docker image builds.** Images built on merge to main, not on every PR. PRs validate code, not containers.

### Merge Gate

All relevant path-filtered checks must pass. Integration tests must pass. No merge to main with failures.

### Environment Consistency

CI pins the same Neo4j, PostgreSQL, and Redis versions used in `docker-compose.yml`. Drift between CI and dev databases is a subtle bug source, especially given the Neo4j 5.18+ dependency for relationship vector indexes.

---

## 13. Deployment

Three deployment stages. The key constraint: LLM API calls are the dominant cost and bottleneck, not compute. A single well-specced VPS handles far more load than expected.

### Stage 1: Local Development (Docker Compose)

Full service topology in Docker Compose:

**Core services** (always running):
- Engine, Fetch, Geo
- Neo4j, PostgreSQL, Redis

**Auxiliary services** (run when needed):
- fetch-browser (Playwright sidecar)
- Scribe
- Grafana, Prometheus, Loki

Docker Compose profiles control what runs: `docker compose up` for core, `docker compose --profile full up` for everything, `docker compose --profile observability up` for monitoring stack.

**Config hot-reload:** `config/` directory volume-mounted into Engine container. Edit a prompt or tool schema, restart the container — no rebuild. Code changes require container rebuild (`docker compose up --build engine`), but Cargo caches make incremental rebuilds fast.

**Database persistence:** Named volumes for Neo4j, PostgreSQL, Redis. Graph and state survive container restarts.

### Stage 2: Single VPS Production (Docker Compose)

Same Docker Compose with production overrides via `docker-compose.prod.yml`:
- Resource limits per container
- Restart policies (`unless-stopped`)
- Production logging drivers (JSON to Loki)
- Built images only (no source mounts)
- Proper volume management with backup

**CI → Production pipeline:** CI builds container images on merge to main, pushes to container registry (GitHub Container Registry). Deploy = pull new images + `docker compose up -d` on the VPS. Simple shell script or GitHub Actions workflow. No complex orchestration tooling — the system is simple enough that those add overhead without benefit at this scale.

**Database backups:** Cron job on VPS. `neo4j-admin dump` for Neo4j, `pg_dump` for PostgreSQL. Redis is ephemeral (work orders also persist in PostgreSQL; Redis loss = re-queue from PG state, not data loss). Backups pushed to object storage.

**Identical images across environments.** The same container images run in dev and prod. Only the orchestration layer (Compose config) and runtime config (config files, environment variables) differ.

### Stage 3: Kubernetes

For when horizontal Processor scaling or high availability becomes necessary.

#### Processor Atomization (Future)

In stages 1-2, Processors are tokio tasks inside the Engine binary. For Kubernetes scaling, Processors need to be independently scalable. The extraction is straightforward: Processors share no in-memory state with the Orchestrator. They read from Redis, call external modules, write to Neo4j, and heartbeat to Redis. When the time comes, factor into a separate `autosint-processor` binary. The Orchestrator doesn't care whether Processors are local tasks or remote containers — it dispatches to Redis, monitors heartbeats, and tracks completion. Same interface either way.

**Do NOT build this separation prematurely.** Build Processors inside the Engine. Extract when scaling demands it.

#### Kubernetes Topology

- **Engine (Orchestrator):** Deployment, 1 replica initially
- **Processors:** Separate Deployment, HPA (Horizontal Pod Autoscaler) scaled on Redis queue depth
- **Fetch, Geo:** Deployments, 1-2 replicas
- **Scribe:** Deployment, scaled based on transcription load
- **fetch-browser:** Deployment, scaled with Fetch
- **Databases:** Managed services (Cloud SQL, Neo4j Aura, managed Redis) preferred over self-hosted StatefulSets
- **Config:** ConfigMaps for `config/` directory, Secrets for API keys

**GitOps:** Manifests in repo, ArgoCD syncing to cluster. Appropriate at this scale.

**Observability stack:** Same Grafana/Prometheus/Loki, deployed via Helm charts instead of Docker Compose. All three are Kubernetes-native.

---

## 14. Research Findings

### Geography + LLMs

Research conducted on current (2024-2026) approaches to LLM geographic reasoning:

- LLMs max 67% accuracy on geospatial tasks from parametric memory (across 30 models benchmarked)
- Connecting to a geographic knowledge graph improved F1 from 37% to 81%
- Best pattern: structured geographic data (PostGIS + knowledge graph) with LLM reasoning on top
- **Perception scripts** (pre-computed spatial context summaries) are the best interface between geo data and LLMs — compact, structured, factual text
- LLMs respond to named places and contextual descriptions, NOT coordinates
- Spatial-RAG combining spatial filtering with semantic matching shows strong results
- LLMs have uneven geographic knowledge — some regions far better represented than others
- Multi-step spatial reasoning degrades — break into explicit sub-queries against the spatial database

Key sources: GeoLLM (ICLR 2024), Spatial-RAG, GraphRAG for geospatial knowledge, GAL framework, Google's Geospatial Reasoning framework.

### Graph + LLM Search/Retrieval

Research conducted on LLM integration with graph databases and search:

- **Three-mode hybrid search** (vector + graph traversal + keyword) is the consensus best practice
- **Function calling** is more reliable than Text2Cypher for LLM-to-graph interface — LLMs struggle with complex multi-hop Cypher queries
- Entity resolution below 85% accuracy degrades the entire system — this is the critical quality gate
- Community detection (Leiden algorithm) on dense graph clusters can provide fast orientation summaries
- Incremental graph building (our approach) sidesteps the expensive full-reindex problem of batch GraphRAG
- Neo4j supports all three search modes natively (vector indexes, graph traversal, full-text indexes)
- **Microsoft GraphRAG** is the defining system — hierarchical community detection + summarization. But expensive for indexing (10-50x tokens vs source text).
- **LightRAG** is the pragmatic alternative — dual-level retrieval, ~100 tokens per query vs GraphRAG's 610K, 80ms latency
- Self-healing query generation (retry with error context) improves Text2Cypher reliability
- Production systems reporting 300-320% ROI in finance, healthcare, manufacturing

Key sources: Microsoft GraphRAG, LightRAG (EMNLP 2025), Neo4j GraphRAG Python package, NL2GeoSQL, CSIS/Scale AI Foreign Policy Benchmark.

### LLM Geopolitical Bias

- LLMs exhibit marked bias toward escalation in crisis scenarios
- US-developed vs China-developed models show distinct ideological perspectives
- Geographic knowledge is unevenly distributed — English-language training data overrepresents some regions
- System design mitigates: factual claims separated from analytical layer, source evaluation emergent not predetermined, geographic data from structured external sources not LLM memory

---

## 15. Stress Test Results

### Concurrency & Scale
- **Multiple Analysts**: concurrent-safe. Graph reads concurrent, Analysts don't write to graph directly, assessments append-only. Redundant work across overlapping investigations solvable via work order deduplication.
- **Microservice fit**: design is already microservice architecture. Each piece independently deployable and scalable.
- **Scaling**: Processors scale horizontally (main lever). Assessment Store, Queue, Orchestrator scale trivially. Knowledge Graph is the eventual bottleneck — graph DBs scale vertically. Organic growth delays this. Design doesn't paint into a corner.
- **Resource footprint**: modest starting scale (~32GB). LLM API calls are dominant cost.

### Data Handling
- **Context windows**: claims are ~50-80 tokens each. Analyst queries iteratively via tools, doesn't need all claims simultaneously. Real constraint is claims needed for a single reasoning step (usually 50-200). Multi-Analyst decomposition handles larger investigations.
- **Scope creep**: naturally bounded by diminishing granularity with distance from core topic, accumulated graph knowledge, and Analyst's own judgment. No hard budget — empirical guardrails if needed.
- **Stale data**: dual timestamps (published + ingested). Analyst guided to consider temporal relevance per-claim, per-topic. Query tools support temporal filtering/sorting.
- **Conflicting claims**: contradictions are information, not errors. Published timestamps distinguish "situation changed" from "sources disagree." Analyst weighs and reasons — this is its core job.
- **Investigation failure**: Analyst always produces an assessment with honest confidence and gap reporting. No special failure path needed.

### Source Capabilities
- **Open source stack**: fully achievable except LLM API calls. Neo4j Community, PostgreSQL, Redis, Whisper, Playwright, open source embeddings.
- **Multimedia** (podcasts, video): fits cleanly via AutOSINT Scribe. Processing time consideration, not architectural.
- **Foreign languages**: LLMs handle natively. Significant advantage over human teams — simultaneous processing of dozens of languages.
- **Closed/anti-automation sources**: Playwright via AutOSINT Fetch browser automation. Graceful degradation when sources are inaccessible. Arms race with some platforms (Twitter/X).

---

## 16. Key Decisions Log

| Decision | Rationale |
|----------|-----------|
| Project name: AutOSINT | Replaces Cronkite. OSINT (Open Source Intelligence) accurately describes what the system does. |
| Rust for Engine | Long-term scalability over development speed. Concurrent operations, memory safety, no GC. |
| Assessments NOT in knowledge graph | Graph = world knowledge. Assessments = analytical products. One-directional dependency keeps graph clean. |
| Source evaluation is emergent, not tiered | Sources are graph entities. Reliability assessed from ownership, track record, incentives — not predefined labels. |
| Claims carry purpose guidance, not format prescriptions | Processor told what claims are FOR; it decides how to structure them. |
| Entity schema minimal with loose "kind" label | Kind is descriptive, not structural. No predefined type-specific schemas. LLM decides properties. |
| Relationships use freeform descriptions | No enumerated types. Searched semantically. More expressive, avoids normalization problems. |
| Entity resolution is critical quality gate | Below 85% accuracy degrades everything. Cascading approach: string → embedding → LLM. |
| LLM-to-graph via function calling | Not raw Cypher. Predefined retrieval functions with structured parameters. More reliable. |
| All data retrieval through AutOSINT Fetch | Centralizes caching, rate limiting, monitoring. Engine never fetches directly. |
| Processor hierarchy: hardcoded → raw HTTP → browser | Default to most reliable/structured source. Browser automation is last resort. |
| Prompts/tool schemas loaded from config | Not compiled into Rust binary. Enables rapid prompt iteration without recompile. |
| Direct LLM API calls, no frameworks | LangChain/LlamaIndex would fight our custom patterns. Thin custom wrapper. |
| Geography as external module (AutOSINT Geo) | LLMs can't do spatial reasoning from memory. Structured geographic data needed. Hot-swappable datasets. |
| Scribe transcribes original language only | No translation in Scribe. LLM handles translation to preserve direct quotes. |
| Scribe uses mechanical diarization only | Speaker 1/2/3 labeling. Processor (LLM) assigns names from context metadata. |
| Multi-Analyst built second, not first | Decomposition guidance needs empirical data. Single-Analyst first, observe, then add. |
| Demand-driven, not continuous | System activates on investigation prompts. Graph grows as byproduct, not as goal. |
| UX layer designed later | Engine is the core challenge. User interfaces and Monitor module come after Engine works. |
| Triage designed later | Needs the Engine to exist first. Manual investigation triggers sufficient to start. |
| Monorepo for all services | Single repo. Coordinated changes, shared Docker Compose, one place to clone. |
| Fetch and Geo in Rust | Shared types with Engine via `autosint-common` crate. Compile-time API contract enforcement. |
| Playwright sidecar (Node.js) | Only browser rendering needs Node.js. Fetch core is Rust. Sidecar is small, focused. |
| Browser sessions via WebSocket | Processor controls navigation step-by-step. Bidirectional — can't pre-program interaction because Processor doesn't know what's on the page until it sees it. |
| No browser platform profiles | Avoided pre-programming site-specific interaction patterns. Will observe what's needed in practice. |
| Browser sidecar always-running | Docker container, not launched on demand. ~100-200MB idle. Eliminates cold start latency. |
| Concurrent browser contexts | Multiple isolated Playwright contexts in one browser instance. Cap at 4-6 to manage memory. |
| Processor heartbeats over fixed timeouts | Processors may block for 30+ minutes on legitimate work (e.g., waiting on Scribe for long transcriptions). Heartbeat TTL detects dead Processors without killing slow ones. |
| Investigation history tool over algorithmic dedup | Analyst sees its own investigation history (prior work orders, results, claim counts). Avoids redundant requests via judgment, not fuzzy matching. Consistent with LLM-decides-analysis philosophy. |
| Failed investigations still produce assessments | Intelligence is never complete — it's as complete as possible. Partial assessment with honest gaps is a valid product. |
| Orchestrator state machine formalized | Explicit states (PENDING, ANALYST_RUNNING, PROCESSING, COMPLETED, FAILED), transitions, and triggers. Deterministic, no ambiguity. |
| Relationships as native Neo4j edges | Neo4j 5.18+ supports vector/full-text indexes on relationship properties. Native edges give better traversal performance than intermediate nodes. |
| Fresh Analyst sessions per cycle | No conversation continuity across cycles. Graph is the Analyst's memory. Prevents anchoring on prior reasoning. |
| `create_work_order` and `produce_assessment` mutually exclusive | Analyst either requests more data or produces assessment in a given session. Aligned via prompt, not enforced as a rule. |
| All numeric parameters runtime-configurable | Limits, thresholds, timeouts, concurrency caps, result sizes — all in config files. No recompile to test a different value. Empirical tuning requires fast iteration. |
| LLM provider abstraction | Thin trait over Anthropic/OpenAI (extensible to others). Provider and model per role configured in config. Switching models is a config change. |
| Anthropic Claude for both roles initially | Opus for Analyst (best reasoning), Sonnet for Processor (good extraction, lower cost). OpenAI for embeddings initially. |
| Non-streaming LLM calls initially | No real-time user watching. Streaming adds complexity for no backend benefit. Add later if UX requires. |
| Grafana + Prometheus + Loki observability stack | Open source, Docker-native, lightweight. Single pane of glass for logs, metrics, traces. Built from first line of code. |
| `tracing` crate for all Rust observability | Structured logging, async-aware spans, OpenTelemetry integration. Consistent across Engine, Fetch, Geo. |
| `investigation_id` as universal correlation key | Every log line, metric label, and trace span carries it. Filter by investigation_id to see full end-to-end story. |
| Tool result size limits in handler_config | JSON tool configs have LLM-facing schema + handler_config section for limits. Both iterable without recompile. |
| Hard vs soft dependency classification | Neo4j/PG/Redis/LLM are hard (SUSPEND on failure). Fetch/Geo/Scribe are soft (degrade gracefully, LLM adapts). |
| Errors as tool results for soft failures | LLM sees the error and adapts — works around it, documents the gap. Session continues. |
| SUSPENDED investigation state | Hard dependency down → investigation persisted for future retry. Survives Engine restarts. Auto-resumes on recovery. |
| Circuit breakers on all dependencies | Prevents hammering dead services. Open/Closed/Half-Open pattern. Thresholds configurable. |
| Shared retry utility | Consistent retry pattern (exponential backoff + jitter) across all services. Per-target defaults, all configurable. |
| Processor writes are crash-safe | Entity dedup + append-only claims make partial writes non-corrupting. No rollback needed on Processor crash. |
| Batched embeddings with backfill fallback | Processor batches all texts, one API call, write with embeddings. On failure: write without, flag `embedding_pending`, background backfill. Graph stays current; 2 of 3 search modes work without embeddings. |
| Path-filtered CI with full Rust workspace builds | Monorepo triggers CI per changed path, but Rust always builds full workspace. Cross-crate breaks caught by shared `common` crate dependency. |
| Integration tests scoped to database boundaries | Neo4j (least mature driver, non-trivial queries), Redis Streams (stateful behavioral semantics), pgvector (similarity search). Not module-to-module HTTP. Not LLM API calls. |
| CI pins database versions to match docker-compose.yml | Prevents drift between CI and dev, especially for Neo4j 5.18+ relationship vector index dependency. |
| Three-stage deployment: Docker Compose → VPS → Kubernetes | LLM API calls are the bottleneck, not compute. Single VPS carries far more load than expected. Kubernetes only when horizontal Processor scaling or HA needed. |
| Processor atomization deferred to Kubernetes stage | Processors currently tokio tasks inside Engine. No in-memory state shared with Orchestrator — extraction to separate binary is straightforward when scaling demands it. Do not build prematurely. |
| Same container images across all environments | Dev, VPS, Kubernetes run identical images. Only orchestration layer and runtime config differ. |

---

## 17. Not Yet Designed

### AutOSINT Triage
- How raw signals are monitored
- How significance is detected (heuristic vs LLM-assisted)
- How it feeds investigation prompts to the Engine
- Interface between Triage and the Analyst

### UX Layer
- User interfaces (web app, email digests, API, etc.)
- Monitor module (matching assessments to user interests, delivering intelligence)
- User preference/interest modeling
- How users interact with the system's understanding (potentially conversational — "ask questions about current understanding")
- Monetization model (subscription, API access, tiered features)

### Implementation Details
- Prompt engineering (Analyst system prompt, Processor system prompt, assessment JSONB schema)
- Geographic data sourcing and loading for Geo module
- Grafana dashboard definitions

---

*This document was produced during the initial AutOSINT design session and represents the complete architectural vision as of that discussion. It should be updated as design decisions are refined during implementation.*