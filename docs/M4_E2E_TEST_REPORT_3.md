# M4 End-to-End Test Report #3

**Date:** 2026-02-20 (20:05 EST / 2026-02-21 01:05 UTC)
**Investigation ID:** `0d12d422-ee6e-4da5-a411-dfb40da65d25`
**Prompt:** "What is the current state of Australia's submarine acquisition program under AUKUS?"
**Result:** COMPLETED — full loop validated (first successful end-to-end investigation)

---

## Timeline

| Event | UTC Timestamp | Elapsed |
|---|---|---|
| Investigation created | 01:05:47 | 0:00 |
| Analyst cycle 1 complete (4 WOs created) | 01:06:16 | 0:29 |
| WO1 completed (9 claims) | 01:09:33 | 3:46 |
| WO2 completed (12 claims) | 01:13:36 | 7:49 |
| WO3 completed (18 claims, MaxTurnsReached) | 01:19:21 | 13:34 |
| WO4 completed (17 claims) | 01:23:17 | 17:30 |
| Analyst cycle 2 complete (assessment produced) | 01:24:48 | 19:01 |
| Investigation completed | 01:24:48 | 19:01 |

**Total wall time: ~19 minutes** (pool_size=1, sequential processing)

---

## Analyst Cycle 1

- **Duration:** 29.3s
- **Turns:** 12
- **Tool calls:** 11
- **Outcome:** Created 4 work orders

The Analyst correctly identified the graph was empty, searched for existing entities/claims/assessments (all empty), checked investigation history, then created 4 targeted work orders:

1. Recent reporting on AUKUS submarine timeline, milestones, status updates (2024-2025)
2. Official government statements from Australia, US, UK on program developments
3. Industrial base development, workforce training, infrastructure projects
4. Challenges, delays, cost overruns, criticism from analysts and media

---

## Processor Results

| WO | Claims | Turns | Duration | Outcome | Notes |
|----|--------|-------|----------|---------|-------|
| WO1 | 9 | — | 3:29 | Completed | Clean run |
| WO2 | 12 | — | 4:03 | Completed | Clean run |
| WO3 | 18 | 50 | 5:45 | Completed (MaxTurnsReached) | Hit turn limit, still produced most claims |
| WO4 | 17 | — | 3:56 | Completed | Clean run |
| **Total** | **56** | — | **17:13** | **4/4 completed** | |

### Warnings During Processing

- **5× fetch_url 502 errors** — government sites (.gov.au, .asa.gov.au) blocking automated requests. Processors gracefully recovered and fetched alternative sources.
- **1× MaxTurnsReached** — WO3 hit 50-turn limit. Correctly treated as Completed with 18 claims (the most of any WO).
- **0 panics, 0 errors, 0 malformed tool calls.**

---

## Knowledge Graph Contents

### Entities: 29

| Entity | Kind |
|--------|------|
| ABC News Australia | publication |
| AUKUS | defense pact |
| Anthony Albanese | person |
| Australian Government | government |
| Australian Naval Infrastructure | government enterprise |
| BAE Systems | organization |
| Barrow-in-Furness shipyard | infrastructure |
| Congressional Research Service | research institution |
| David Johnston | person |
| GOV.UK | publication |
| General Dynamics Electric Boat | organization |
| HMAS Stirling | military facility |
| Infrastructure and Major Projects Authority | government agency |
| John Healey | person |
| Jonathan Mead | person |
| Lloyd J. Austin III | person |
| Navy Lookout | publication |
| Osborne Submarine Construction Yard | infrastructure |
| Pentagon | government agency |
| Rex Patrick | person |
| Richard Marles | person |
| Rolls-Royce Submarines Ltd | organization |
| SSN-AUKUS submarine | military platform |
| South Australia Skills and Training Academy | educational institution |
| The Guardian | publication |
| UK Ministry of Defence | organization |
| United Kingdom | country |
| United States Studies Centre | research institution |
| Virginia-class submarine | military platform |

### Claims: 56

All claims sourced from 9 unique URLs across 6 distinct publications:

| Source | Claims | Type |
|--------|--------|------|
| The Guardian (3 articles) | 16 | Investigative journalism |
| GOV.UK (2 pages) | 10 | Official government statements |
| Navy Lookout (2 articles) | 13 | Defense analysis |
| USSC (1 report) | 10 | Academic research |
| ABC News Australia (1 article) | 5 | News reporting |
| minister.defence.gov.au | 2 | Official government |

All claims attributed as "secondhand" (appropriate — sourced from published reporting, not primary interviews).

**Sample claims (showing range of content):**

- "Australia will receive three Virginia-class nuclear submarines from the United States starting from 2032, 2035, and 2038"
- "The AUKUS submarine deal is valued at $368 billion according to Pentagon review reports"
- "The United States has missed its production target of 2.3-2.5 SSNs per year, constructing only 1.2 boats per year"
- "The UK government's own major projects agency has described the UK's plan to build nuclear reactor cores as 'unachievable'"
- "The AUKUS submarine program requires a direct workforce of 20,000 over the next 30 years"
- "The Barrow shipyard workforce fell from 13,000 workers to 3,000 during the 17-year gap between the Vanguard and Astute"

### Relationships: 21

All relationships include descriptive labels and confidence scores (0.8–1.0).

| From | Relationship | To |
|------|-------------|-----|
| Anthony Albanese | serves as Prime Minister | Australian Government |
| Australian Government | will acquire Virginia-class submarines | Virginia-class submarine |
| Australian Government | member of trilateral security partnership | AUKUS |
| BAE Systems | operates Barrow shipyard for nuclear submarines | Barrow-in-Furness shipyard |
| BAE Systems | builds nuclear submarines for AUKUS program | AUKUS |
| Congressional Research Service | warns of elevated cost risk | AUKUS |
| General Dynamics Electric Boat | primary contractor facing workforce shortages | Virginia-class submarine |
| Infrastructure & Major Projects Authority | assesses reactor core project as 'unachievable' | SSN-AUKUS submarine |
| Osborne Submarine Construction Yard | will replicate Barrow processes | Barrow-in-Furness shipyard |
| Pentagon | reviewed AUKUS agreement in 2025 | AUKUS |
| Rex Patrick | criticizes UK shipbuilding, warns of gaps | AUKUS |
| Richard Marles | describes as 'too big to fail' | AUKUS |
| Richard Marles | serves as Defence Minister | Australian Government |
| Rolls-Royce Submarines Ltd | provides nuclear reactor technology | SSN-AUKUS submarine |
| United Kingdom | member of trilateral partnership | AUKUS |
| *...and 6 more* | | |

---

## Assessment Produced

**Confidence:** Moderate

### Summary

> Australia's AUKUS submarine acquisition program remains on track according to official statements as of September 2025, with key milestones being met despite facing significant workforce and industrial capacity challenges. The program involves acquiring three Virginia-class submarines from the US starting in 2032, followed by building SSN-AUKUS submarines domestically from the late 2030s at a total cost of $368 billion.

### Analysis (excerpt)

The assessment covers three program phases:

1. **Immediate capability building** — $1.6B paid to US, £2.4B invested in UK infrastructure, first US submarine maintenance completed in Australia (August 2024)
2. **Interim acquisition** — Three Virginia-class submarines from US (2032, 2035, 2038)
3. **Domestic production** — $30B Osborne Submarine Construction Yard in Adelaide, SSN-AUKUS design with UK, workforce of 20,000 over 30 years

### Competing Hypotheses

| Hypothesis | Supporting Evidence | Weaknesses |
|-----------|-------------------|------------|
| Program proceeding smoothly | Official confidence from Defence Minister Marles, milestone completion, financial commitments met | Limited independent verification, potential political messaging |
| Serious risk of delays/capability gaps | No RN SSNs at sea, Collins-class extended beyond life, massive workforce needs with limited expertise | Government contingency planning, workforce initiatives underway |
| Pentagon review signals US concerns | Review led by official with "mixed history" on AUKUS, formal review in 2025 | May be routine oversight, Australian officials remain confident |

### Gaps Identified

- Detailed SSN-AUKUS domestic production timeline beyond "late 2030s"
- Specific Pentagon review findings and recommendations
- Regulatory frameworks for nuclear submarine operations in Australia
- Progress on workforce recruitment vs. targets
- Detailed cost breakdown beyond announced commitments
- Collins-class life extension feasibility

### Forward Indicators

- Pentagon review findings and resulting program modifications
- Osborne shipyard construction milestones and workforce recruitment
- First Virginia-class delivery timeline adherence (2032)
- UK SSN-AUKUS production schedule and any delays
- Collins-class availability and emergency life extensions
- Congressional approval for US submarine industrial base expansion

---

## Fixes Applied (Since Test #2)

These three fixes were applied before this test:

1. **`wait_for_work_orders` timeout handling** — On timeout, now fails stuck work orders and returns Ok so orchestrator can proceed (prevents permanently frozen investigations).
2. **`reclaim_pending` wired up** — Worker loop periodically reclaims stale messages from dead consumers via XCLAIM. Complements the existing dequeue(ID=0) pending check.
3. **`MalformedToolCallLimit` claims reporting** — Now reports actual `claims_created` count (not hardcoded 0) and treats as Completed (graph writes are non-transactional).

---

## Comparison to Previous Tests

| Metric | Test #1 | Test #2 | Test #3 |
|--------|---------|---------|---------|
| WOs completed | 0/5 | 4/5 | **4/4** |
| Entities | 0 | 24 | **29** |
| Claims | 0 | 34 | **56** |
| Relationships | 0 | 14 | **21** |
| Unique sources | 0 | ~6 | **9** |
| Analyst cycle 2 | N/A | Stuck | **Completed** |
| Assessment produced | No | No | **Yes** |
| Panics | 1 | 0 | **0** |
| Total time | Timed out | Timed out | **19 min** |

---

## Repeat Investigation: Graph as Memory

Immediately after Test #3 completed, the same prompt was submitted again to test whether the Analyst would leverage existing graph knowledge or redundantly create new work orders.

**Investigation ID:** `95b51e45-02ef-4b75-bb28-7c6ad3017d52`
**Result:** Assessment produced directly from graph — zero work orders.

| Metric | Investigation #1 (empty graph) | Investigation #2 (populated graph) |
|--------|-------------------------------|-----------------------------------|
| Cycle count | 1 (WOs + assessment) | 0 (assessment only) |
| Work orders | 4 | **0** |
| Analyst turns | 12 + 11 = 23 | **9** |
| Analyst tool calls | 11 + 10 = 21 | **8** |
| Total time | ~19 min | **~2.5 min** |
| Assessment confidence | Moderate | Moderate |

### What the Analyst Did

1. `search_entities` — returned 6,231 chars (vs 14 on empty graph)
2. `search_claims` — returned 9,972 chars of existing claims
3. `search_assessments` — returned 14 (previous assessment is in PG, not the graph)
4. `get_investigation_history` — no prior WOs for this investigation ID
5. `search_claims` (second query) — 4,621 chars, different search terms
6. `traverse_relationships` — 4,566 chars of relationship data
7. `search_claims` (third query) — 14 chars (exhausted relevant claims)
8. `produce_assessment` — produced assessment directly

The Analyst correctly determined that the graph already contained sufficient, diverse, recent evidence to answer the question. It explored entities, claims, and relationships across multiple queries, then produced the assessment without creating any work orders.

This validates the core architectural premise: **the knowledge graph is the Analyst's memory across investigations.** Repeated or related questions benefit from prior research without redundant web fetching.

---

## Known Issues / Observations

1. **Government site blocking** — 5 Australian government URLs returned 502. These sites likely block non-browser user agents. The Fetch service's browser sidecar (M5) would address this.
2. **WO3 hit max turns** — 50 turns wasn't enough for WO3 (infrastructure/workforce topic had deep content). Still produced the most claims (18) so the MaxTurnsReached-as-Completed fix worked correctly.
3. **pool_size=1** — Sequential processing means 4 WOs took ~17 minutes. With pool_size=2-3, this would be ~6-8 minutes.
4. **All claims "secondhand"** — Appropriate for web-sourced published reporting. Primary attribution would require direct interviews or official documents fetched from original sources.
5. **Assessment quality** — The competing hypotheses analysis is genuinely useful, with specific evidence cited for each hypothesis. The "gaps" and "forward indicators" sections demonstrate proper analytical tradecraft.
6. **Assessment search gap** — The Analyst's `search_assessments` returned empty (14 chars) despite a prior assessment existing. Assessments live in PostgreSQL, not Neo4j — the search tool may need to also query the store for prior assessments on related topics.
