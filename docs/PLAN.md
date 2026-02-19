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
11. [Research Findings](#11-research-findings)
12. [Stress Test Results](#12-stress-test-results)
13. [Key Decisions Log](#13-key-decisions-log)
14. [Not Yet Designed](#14-not-yet-designed)

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

### Depth = Graph Density, Not Record Complexity

The LLM goes deeper by creating more entities and relationships, not by adding more properties to existing ones. Understanding emerges from the structure of the graph. A densely connected area of the graph represents deep understanding. A sparse area represents shallow or no understanding. The LLM's editorial judgment about depth manifests as how far it expands the graph, not how detailed individual records are.

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

### 4.4 LLM Roles

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

### 4.5 Work Order System

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

### 4.6 Orchestration

The **orchestrator** is deterministic Rust code — not an LLM. It is a **session manager and work dispatcher**. It does not make analytical decisions, assemble context, or decide what's relevant.

**Investigation lifecycle:**

1. Investigation prompt arrives → orchestrator creates investigation record in PostgreSQL
2. Starts an **Analyst agentic session** with: the investigation prompt + tool access (graph queries, Assessment Store queries, AutOSINT Geo queries, work order creation, AutOSINT Fetch source catalog)
3. Analyst self-serves retrieval, reasons, outputs: work orders and/or an assessment
4. If work orders: orchestrator queues them in Redis, dispatches to available Processors
5. Processors execute in parallel: call AutOSINT Fetch for data retrieval, extract claims, write to graph, mark work orders complete
6. On completion of all work orders for this cycle → orchestrator starts a new Analyst agentic session with the same investigation prompt + same tool access (graph now has new data from Processors)
7. Repeat until Analyst produces an assessment → orchestrator writes to Assessment Store, marks investigation complete

**Orchestrator responsibilities:**
- Investigation lifecycle management (create, track cycles, complete)
- Work order dispatch and completion tracking
- Processor pool management (available, busy, assignments)
- Assessment routing to Assessment Store
- Providing tool access to Analyst sessions

**What the orchestrator does NOT do:**
- Make analytical decisions
- Pre-query the graph or assemble context
- Decide when an investigation is complete (the Analyst decides)
- Decide what's relevant (the Analyst decides)

**Multi-Analyst investigations (future):**
For large investigations, a planning Analyst call can decompose the investigation into parallel sub-investigations. Multiple Analysts run concurrently, all sharing the same graph. A synthesis Analyst produces a final assessment built from the child assessments. Child Analysts produce normal assessments — no special types. The orchestrator manages parent-child investigation records and triggers synthesis on completion.

This is NOT built first. Build single-Analyst investigations first. Add multi-Analyst decomposition when empirical data shows where single Analysts struggle. The decomposition guidance (how big is "too big," what are right-sized sub-investigations) can only be determined from experience.

### 4.7 Search & Retrieval

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

### 4.8 Tool Layer

LLMs interact with the system through **predefined functions** (not raw queries). Interfaces are hardcoded; usage is LLM-decided.

**Tools available to the Analyst:**
- Query entities (by name, kind, properties, semantic search)
- Traverse relationships (from entity outward, filtered by description/direction/weight)
- Query claims (by entity, by time range, by source, semantic search)
- Query assessments (semantic search over Assessment Store)
- Create work orders (write to Redis queue)
- Query AutOSINT Geo (geographic context, spatial queries, terrain, borders, features)
- Query AutOSINT Fetch source catalog (see available data sources)

**Tools available to Processors:**
- Query entities (for deduplication — name/alias matching + embedding similarity)
- Store entities (create new or update existing)
- Store claims (append to graph)
- Store relationships (create new or update existing)
- Query AutOSINT Fetch (source catalog, data retrieval, raw HTTP, browser automation)
- Query AutOSINT Scribe (submit transcription jobs, poll for results)

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

Node.js + Playwright service within Fetch. Capabilities:

- Render JavaScript-heavy pages
- Manage sessions and cookies across requests
- Handle authentication flows
- Navigate multi-page content (pagination, load-more buttons)
- Support platform-specific browsing profiles
- Graceful degradation — returns "could not access source" rather than crashing

**Security isolation:** Browser automation runs in its own container with limited network access. No direct graph access.

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

POST /browse                   → browser-automated fetch
                                 Input: { url, options (wait_for, interact, auth_profile) }
                                 Returns: { content, metadata }
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
| Fetch (browser automation) | Node.js + Playwright | Playwright's native environment |
| Scribe | Python + Whisper | Whisper and audio libraries are Python-native |
| Geo | TBD (likely Python or Go) | PostGIS integration |
| Triage | TBD | TBD |

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

## 11. Research Findings

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

## 12. Stress Test Results

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

## 13. Key Decisions Log

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

---

## 14. Not Yet Designed

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
- Exact Rust project structure and module organization
- Neo4j schema design (node labels, indexes, constraints)
- PostgreSQL schema design (assessment table, investigation records, work order state)
- Redis queue structure (streams vs lists, consumer groups)
- Prompt engineering (Analyst system prompt, Processor system prompt, tool definitions)
- LLM provider selection (Anthropic Claude vs OpenAI GPT vs mix)
- Embedding model integration details
- Error handling and retry strategies across services
- Logging, monitoring, and observability
- Testing strategy
- Deployment pipeline
- Geographic data sourcing and loading for Geo module

---

*This document was produced during the initial AutOSINT design session and represents the complete architectural vision as of that discussion. It should be updated as design decisions are refined during implementation.*