# System Improvements

Issues and design changes identified during M4 end-to-end testing and subsequent analysis. This document captures both the problems observed and the design decisions made to address them. Implementation details are in PLAN.md; this document explains the reasoning.

---

## Core Principle Added: Graph Integrity Over Completeness

**Added to PLAN.md §2 as a foundational design principle.**

The knowledge graph is the system's most valuable long-term asset. LLM hallucination is the primary existential threat. During testing, we observed the Processor contaminating entity names and properties from training data patterns (e.g., "Honorable Lloyd J. Austin III" — wrong title, wrong current role, derived from Australian honorific patterns in surrounding context, not from the source document).

**The principle:** Every fact in the graph must trace to a fetched, verifiable source. The LLM reasons freely but never introduces facts from its own parametric memory. An incomplete graph is correct. Fabricated completeness is corruption.

**This principle affects every other improvement below.** It governs how Processors write to the graph, how source entities are built, how the Analyst evaluates evidence, and what the assessment output looks like.

See PLAN.md §2 "Graph Integrity Over Completeness" for full details.

---

## 1. Assessment Output Quality

**Priority: Critical — this is the product. If the assessment isn't intelligence-grade, nothing else matters.**

### Problems Observed

- Assessment analysis contained no citations or source traceability. Every statement was unverifiable without manually querying the graph.
- Competing hypotheses presented at equal weight despite clearly different levels of evidence support.
- Confidence level ("moderate") asserted without explaining why — no named factors.
- Source evaluation completely absent. The Analyst didn't investigate or characterize its sources. The design intent (PLAN.md §2: "Source Evaluation is Emergent, Not Tiered") was not met.
- Forward indicators were loose strings, not linked to graph entities.
- Sources span a 2-year range with no temporal freshness discussion in the assessment.

### Design Decisions (documented in PLAN.md)

**Assessment JSONB schema defined** (PLAN.md §4.3 Assessments):
- `analysis` text carries inline source evaluation — not footnotes but analytical prose weaving source characterization into every substantive statement. "According to Source X, which is [structural characterization], Y — corroborated by Source Z via independent reporting, though both ultimately trace to [primary source]."
- `competing_hypotheses` now carry `probability` (0.0-1.0), `reasoning` (why this probability), and `claim_refs` (specific evidence).
- `confidence_reasoning` names specific factors: source diversity, primary vs secondhand ratio, temporal freshness, corroboration patterns, source access limitations.
- `citations` array serves as reference index — claim_id, source_entity_id, source_url, date, attribution_depth.
- `sources_evaluated` carries structured source profiles: structural profile (from fetched data, never LLM memory), profile_basis (what was fetched to determine the profile), sourcing chain notes.
- `gaps` now carry `impact` and `suggested_resolution`.
- `forward_indicators` now carry `entity_refs`, `claim_refs`, and `trigger_implication`.

**Source evaluation is integral, not separate.** The analysis text IS the source evaluation. The Analyst must query source entities, examine structural profiles built by Processors from fetched material, and evaluate corroboration patterns. Source structural profiles come from fetched about pages and public records — never from LLM memory assertions about reputation.

**Corroboration is the primary trust mechanism.** Rather than asserting "Source X is reliable" (which requires evaluating the evaluator — an infinite regression), the system evaluates whether independent sources agree. Independent meaning: different ownership, different geographic base, different editorial incentive, different sourcing chain. Three sources citing the same press release is not corroboration.

### Implementation — DONE

- [x] Updated `config/tools/analyst/produce_assessment.json` schema to match PLAN.md JSONB definition.
- [x] Updated `config/prompts/analyst.md` to instruct inline source evaluation, probability assignment, confidence reasoning, and citation usage.
- [x] Analyst prompt includes source evaluation phase: query source entities before producing assessment.
- No Rust code changes needed for assessment schema — `content` is JSONB, handler passes through.

---

## 2. Claim Classification (Two-Dimensional)

**Priority: High — forces structured thinking at extraction time, enables powerful Analyst filtering.**

### Problem Observed

All 56 claims from the test were tagged only as "secondhand" with no further classification. The Analyst had no structured way to distinguish government assertions from expert analysis from public sentiment. It treated all claims as equivalent evidence.

### Design Decision (documented in PLAN.md §4.3 Claims)

Claims are now classified on two independent dimensions:

**Attribution depth** (chain of custody):
- `primary` — direct from the entity (official documents, filings, official social media)
- `secondhand` — named intermediary reporting (journalism, named expert analysis)
- `indirect` — anonymous sources, unnamed officials, thirdhand, unverified identities

**Information type** (how the source presents the information — form, not truth value):
- `assertion` — source presents as factual claim. The label means "source asserts this" not "this is true." The Analyst must still evaluate given source incentives.
- `analysis` — source presents as judgment, assessment, prediction, opinion.
- `discourse` — collective reaction, public discussion, opinion trends.
- `testimony` — personal accounts from individuals claiming direct experience.

The forced double classification prevents the Processor's path-of-least-resistance tendency to treat all information as equivalent. The neutral wording (especially `assertion` instead of `factual`) avoids pre-judging reliability — the Analyst determines trust.

### Implementation — DONE

- [x] Added `InformationType` enum and `Indirect` variant to `AttributionDepth` in `autosint_common::types::claim`.
- [x] Added `information_type` to Neo4j Claim node schema + range index.
- [x] Updated `create_claim` graph method, conversions (backward-compatible), and search filters.
- [x] Updated `create_claim`, `search_claims`, and `update_entity_with_change_claim` handlers.
- [x] Updated `create_claim.json`, `search_claims.json`, and `update_entity_with_change_claim.json` schemas.
- [x] Processor prompt includes classification guide with definitions and examples for both dimensions.

---

## 3. Processor Workflow Restructure (Plan → Research → Extract)

**Priority: Critical — addresses source diversity (the #1 quality gap) and turn efficiency simultaneously.**

### Problems Observed

- Only 9 URLs across 6 publications for a major geopolitical topic. Enterprise/government-grade intelligence requires far more diverse sourcing.
- Processors interleaved research and extraction: find one source → exhaustively extract → find another. This finds 2-3 sources per work order.
- 50 turns for 18 claims from 2-3 sources. Each claim creation takes ~4 turns (search_entities + create_entity + create_claim + create_relationship). Extraction consumes the entire turn budget, leaving almost nothing for research.
- Fetched document content sits in conversation history for all subsequent turns, wasting context window on stale content.

### Root Cause

Not a tooling problem. Web search + fetch is sufficient for a skilled researcher — query tuning, site-specific searches, following citation chains, and source-type awareness can reach diverse sources including government documents, think tank reports, parliamentary records, and academic papers. The Processor simply isn't researching well.

### Design Decision (documented in PLAN.md §4.5 Processor)

**Three-phase workflow:**

1. **Plan** (2-3 turns): Explicitly plan 6-10 diverse search queries targeting different source types before making any tool calls. Structure forces thinking.
2. **Research** (15-20 turns): Execute planned searches, evaluate results for source diversity, follow citation chains, fetch source about-pages for structural profiles. Accumulate 8-15 documents from diverse sources.
3. **Extract** (remaining turns): Process each document with batch extraction — all claims from one document in a single tool call.

**Batch extraction tool:** New tool that accepts a source entity and array of claims with referenced entities and relationships. Handler does dedup, entity creation, claim creation, and relationship creation internally. Reduces extraction from ~4 turns per claim to ~1-2 turns per document.

**Combined effect:** Same 50-turn budget produces 10-15 diverse sources instead of 2-3, with source structural profiles and citation-chain-traced primary documents.

### Implementation — DONE

- [x] Updated `config/prompts/processor.md` with three-phase workflow (Plan → Research → Extract), diverse search query planning, citation chain following.
- [x] Created `batch_extract` tool: schema in `config/tools/processor/batch_extract.json`, handler in `crates/engine/src/tools/handlers/batch_extract.rs`.
- [x] Handler internally: dedup entities via `EntityDedup`, create entities, create claims (with both classification dimensions), create relationships, compute embeddings. Collect-warnings pattern for partial failures.
- Individual create_entity/create_claim tools retained alongside batch_extract for one-off additions.

---

## 4. Processor Grounding Discipline

**Priority: Critical — prevents graph poisoning.**

### Problem Observed

"Honorable Lloyd J. Austin III" — the Processor applied a title and formality from contextual pattern matching (Australian honorific patterns in surrounding documents), not from the source document. Entity summaries and properties were enriched from LLM training data, not from fetched content.

### Design Decision (documented in PLAN.md §4.5 Processor, §4.3 Entities)

- Processor records ONLY information explicitly stated in fetched source documents.
- Entity summaries built exclusively from fetched sources. If a document only mentions a name in passing, the summary reflects only that.
- Entity properties require grounding in fetched documents. No document mention = no property.
- Source entities built from fetched about/info pages for structural profiling.
- Stubs are preferred over fabricated completeness.

### Comprehension vs. Evaluation: The Critical Distinction

The grounding discipline does NOT mean the Processor is a dumb extractor. It must comprehend documents to extract structured data accurately. The line is:

- **Comprehension (Processor's job):** Understanding what a document is saying, who it refers to, what claims are being made. Includes recognizing rhetoric, political framing, and contextual references. Example: reading "Governor Trudeau" in a Trump quote and understanding this refers to the Canadian Prime Minister with a deliberately wrong title.
- **Evaluation (Analyst's job):** Judging whether claims are true, whether sources are reliable, whether information is current. The Processor classifies claims (attribution_depth + information_type) to give the Analyst structured handles; it does not judge them.

### Entity Resolution Under Grounding

Entity resolution — matching references in documents to existing graph entities — is a **reasoning** task, not a memory task. It operates on graph context + document context, not training data.

**Matching to existing entities (allowed, encouraged):** The Processor searches existing entities, finds "Anthony Albanese" with relationships to "Australian Government" and claims about being Prime Minister. A new article about Australian defense mentions "Prime Minister Albanese." The Processor reasons from graph context that these are the same person.

**Creating entities beyond what the document states (prohibited):** A document mentions "Prime Minister Albanese" with no existing entity in the graph. The Processor creates an entity with the name and attributes the document provides — not "Anthony Peter Albanese, born March 2, 1963." The first name gets added when a document actually states it.

**Specific edge cases the Processor must handle:**

1. **Ambiguous references.** "The Prime Minister" in a document discussing both Australia and the UK. The Processor checks graph relationships for both PMs and uses surrounding document context to disambiguate. If genuinely uncertain, create a minimal stub with what the document says rather than force a match.

2. **Deliberately wrong references.** Trump calls the Canadian PM "Governor Trudeau." The Processor understands this refers to the existing Trudeau entity, records the claim about what Trump said (classified as `primary` + `discourse` or `assertion`), and does NOT create a new "Governor Trudeau" entity or update Trudeau's role to "Governor."

3. **Outdated information.** A 2023 article says "Prime Minister Albanese" but Albanese is no longer PM. The Processor records the claim with `published_timestamp: 2023` — this is correct. The claim IS that this source reported this at that time. Entity summaries may lag behind reality; claims with timestamps are the authoritative temporal record. The Analyst handles temporal reasoning.

4. **Cross-investigation confusion.** A later investigation about UK politics mentions "Prime Minister Albanese met with..." The existing entity has Australian context (relationships, claims). The Processor uses those relationships to correctly resolve, not just name matching.

**The safety net:** Even if a Processor misresolves or misclassifies, the Analyst sees the full graph — multiple claims from multiple sources. One bad extraction doesn't poison the assessment if the graph has sufficient source diversity. This is another reason the three-phase workflow (§3) matters.

### Implementation — DONE

- [x] Complete Processor prompt rewrite (~170 lines) with Graph Integrity principle, comprehension/evaluation distinction, entity resolution guidance with four named edge cases, source entity enrichment instructions.
- Entity-level `source_url` provenance deferred — `batch_extract` links claims to source URLs via `raw_source_link`, which provides document-level traceability. Entity-level provenance would require schema changes; revisit if needed.

---

## 5. Assessment Search (ivfflat Cold Start)

**Priority: High — causes silent data loss on repeat investigations.**

`search_assessments` returns empty despite assessments existing with valid embeddings. Root cause: pgvector ivfflat index with `lists=10` and default `probes=1` misses rows when the table has very few entries. Confirmed: setting `probes=10` finds both assessments correctly.

### Fix

Switch from ivfflat to HNSW index. One-line migration:

```sql
DROP INDEX idx_assessments_embedding;
CREATE INDEX idx_assessments_embedding ON assessments USING hnsw (embedding vector_cosine_ops);
```

**Implementation — DONE.** Migration at `crates/engine/src/store/migrations/20260222000001_hnsw_assessment_index.sql`. PLAN.md §4.4 updated to HNSW.

---

## 6. Government Site Access (Browser Sidecar)

**Priority: Medium — blocks primary source attribution.**

5 Australian government URLs returned 502 during testing. These sites block non-browser user agents. Processors fell back to journalism citing those sources, degrading attribution from "primary" to "secondhand."

### Approach

- **Short term:** Processor prompt instructs noting which URLs failed and why. Analyst sees this in claims and flags in assessment source evaluation. Failed primary source access is itself a named factor in confidence reasoning.
- **Medium term:** M5 browser sidecar (Playwright) is the planned solution. The Fetch service already has the architecture for it (`services/fetch-browser`). Browser sidecar should be prioritized in M5 alongside source adapter work since it directly addresses primary source access.

---

## 7. Contradictory Claims and Temporal Reasoning

**Priority: Medium — matters as the graph grows over time.**

### What exists
- Claims have `ingested_timestamp` and `published_timestamp`
- Multiple contradictory claims can coexist (graph doesn't enforce consistency)
- Analyst prompt instructs consideration of recency and competing evidence

### What's missing
- Confidence decay on older claims when newer contradictory ones arrive
- Contradiction edges between claims
- Supersession linkage
- Staleness detection

### Approach

Handled at the Analyst layer, not graph schema. The Analyst sees timestamps and weighs recency. Temporal freshness is now a named factor in `confidence_reasoning` (see assessment schema). The assessment must explicitly state the age range of its evidence and evaluate whether older claims remain valid for the topic type.

**Outdated claims are not wrong claims.** A 2023 claim that "Albanese is Prime Minister" correctly records what was reported at that time. The claim's `published_timestamp` is what makes it useful — it's evidence about the state of the world at that date. Entity summaries may lag behind reality as a result, but claims (with timestamps) are the authoritative temporal record. The Analyst reasons about which claims reflect current reality based on recency and corroboration.

Leave rigid `CONTRADICTS` edges and confidence decay for when the graph is large enough to need them. The Analyst's contextual judgment is sufficient at current scale.

---

## 8. Source Evaluation by Analyst

**Priority: High — core design intent not yet implemented.**

### Problem Observed

The Analyst produced an assessment without investigating any of its sources as entities. The design intent (PLAN.md §2: "Source Evaluation is Emergent, Not Tiered") was not met. Sources should be entities in the graph with ownership, track record, and incentive structures that the Analyst traverses.

### Design Decision

**Source evaluation is addressed through three mechanisms:**

1. **Processor source entity enrichment** (§4.5): When a Processor encounters a new source, it fetches the source's about/info page and creates the source entity from that fetched content. Structural facts (ownership, funding model, geographic base, editorial focus) are recorded from the source's self-description.

2. **Analyst source evaluation phase** (§4.5): Before producing an assessment, the Analyst queries source entities for structural profiles. Where profiles are thin, it notes this as a limitation.

3. **Corroboration as primary trust mechanism** (§4.3 Assessments): Instead of asserting source reliability (which requires evaluating the evaluator), the system evaluates whether independent sources agree. This sidesteps the epistemological regression.

Source reliability cannot be asserted from LLM memory. It can only be built from: fetched structural facts about the source, observed corroboration patterns within the graph, and accumulated track record over time as the graph grows.

---

## 9. Forward Indicators and Future Triage

**Priority: Medium — structural investment for future monitoring capability.**

Forward indicators are now linked to graph entities and claims in the assessment schema, with `trigger_implication` describing what it means if the indicator fires. This makes them machine-actionable for the future Triage module.

When Triage is built, it can match incoming claims against forward indicators from previous assessments: new claim references Pentagon + AUKUS + "review" → flag that this forward indicator may be triggered → alert that a previous assessment may need updating.

The schema is ready. The monitoring capability is future work (Triage module, not yet designed).

---

## 10. Pool Parallelism

**Priority: Low — blocked by external constraints, not a design issue.**

`pool_size=1` means 4 work orders process sequentially (~17 min). The infrastructure supports `pool_size=2-3`, but parallel Processors hit API rate limits when making simultaneous LLM calls through OpenRouter. Investigation wall time is not a priority concern — a human analyst would spend 10-100x longer producing equivalent research. Quality is the bottleneck, not speed.

Potential future solutions (out of scope): multiple API keys, higher-tier API plans, request queuing with rate-limit-aware scheduling. Revisit when quality improvements are validated and throughput becomes the constraint.

---

## Implementation Order

Based on impact and dependencies:

1. ~~**Processor grounding discipline** (prompt) — prevents further graph poisoning. Do first.~~ **DONE**
2. ~~**Processor workflow restructure** (prompt) — plan → research → extract. Addresses source diversity.~~ **DONE**
3. ~~**Batch extraction tool** (Rust + schema) — enables the extraction phase to be efficient.~~ **DONE**
4. ~~**Claim two-dimensional classification** (Rust + schema + prompt) — enables structured Analyst filtering.~~ **DONE**
5. ~~**Assessment schema and Analyst prompt** (schema + prompt) — produces the improved assessment output.~~ **DONE**
6. ~~**HNSW index migration** (SQL) — one-line fix, do anytime.~~ **DONE**
7. **Browser sidecar** (M5) — for government site access. **Not started.**
8. **Pool parallelism** (config) — when iteration speed matters. **Not started.**

Items 1-6 implemented. 18 files changed (15 modified, 3 new). Items 7-8 are M5/production work.
