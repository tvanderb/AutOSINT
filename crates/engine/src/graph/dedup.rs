#![allow(dead_code)]

use std::future::Future;
use std::pin::Pin;

use neo4rs::query;

use autosint_common::config::DedupConfig;
use autosint_common::types::Entity;
use autosint_common::EntityId;

use super::conversions::node_to_entity;
use super::escape_lucene_query;
use super::GraphClient;
use super::GraphError;

/// Result of a deduplication check.
#[allow(clippy::enum_variant_names)]
pub enum DedupResult {
    ExactMatch(EntityId),
    ProbableMatch {
        entity_id: EntityId,
        confidence: f64,
        stage: DedupStage,
    },
    NoMatch,
}

/// Which dedup pipeline stage produced the match.
pub enum DedupStage {
    ExactString,
    FuzzyString,
    EmbeddingSimilarity,
    LlmJudgment,
}

/// Trait for LLM-based deduplication judgment (interface only — M3 implements).
/// Uses boxed future return for object safety (dyn dispatch).
pub trait LlmDedupJudge: Send + Sync {
    fn judge(
        &self,
        candidate: &Entity,
        existing: &Entity,
    ) -> Pin<Box<dyn Future<Output = Result<Option<f64>, GraphError>> + Send + '_>>;
}

/// Entity deduplication pipeline.
pub struct EntityDedup<'a> {
    graph: &'a GraphClient,
    config: &'a DedupConfig,
    llm_judge: Option<&'a dyn LlmDedupJudge>,
}

impl<'a> EntityDedup<'a> {
    pub fn new(
        graph: &'a GraphClient,
        config: &'a DedupConfig,
        llm_judge: Option<&'a dyn LlmDedupJudge>,
    ) -> Self {
        Self {
            graph,
            config,
            llm_judge,
        }
    }

    /// Run the cascading deduplication pipeline for a candidate entity.
    /// Stages: exact string → fuzzy string → embedding similarity → LLM judgment.
    pub async fn find_duplicate(
        &self,
        name: &str,
        kind: &str,
        embedding: Option<&[f32]>,
    ) -> Result<DedupResult, GraphError> {
        let start = std::time::Instant::now();

        // Stage 1: Exact string match on canonical_name or aliases.
        if let Some(entity_id) = self.exact_string_match(name).await? {
            metrics::counter!("graph.dedup.stage_hit", "stage" => "exact_string").increment(1);
            metrics::histogram!("graph.dedup.latency").record(start.elapsed().as_secs_f64());
            return Ok(DedupResult::ExactMatch(entity_id));
        }

        // Stage 2: Fuzzy string match using fulltext search + Jaro-Winkler.
        if let Some((entity_id, confidence)) = self.fuzzy_string_match(name, kind).await? {
            metrics::counter!("graph.dedup.stage_hit", "stage" => "fuzzy_string").increment(1);
            metrics::histogram!("graph.dedup.latency").record(start.elapsed().as_secs_f64());
            return Ok(DedupResult::ProbableMatch {
                entity_id,
                confidence,
                stage: DedupStage::FuzzyString,
            });
        }

        // Stage 3: Embedding similarity via vector search.
        if let Some(embedding) = embedding {
            if let Some((entity_id, confidence)) =
                self.embedding_similarity_match(embedding).await?
            {
                metrics::counter!("graph.dedup.stage_hit", "stage" => "embedding_similarity")
                    .increment(1);
                metrics::histogram!("graph.dedup.latency").record(start.elapsed().as_secs_f64());
                return Ok(DedupResult::ProbableMatch {
                    entity_id,
                    confidence,
                    stage: DedupStage::EmbeddingSimilarity,
                });
            }
        }

        // Stage 4: LLM judgment (deferred to M3 — only runs if judge is provided).
        // Currently no-op since llm_judge will always be None in M2.
        if self.llm_judge.is_some() {
            // LLM judgment would go here in M3.
        }

        metrics::counter!("graph.dedup.no_match").increment(1);
        metrics::histogram!("graph.dedup.latency").record(start.elapsed().as_secs_f64());
        Ok(DedupResult::NoMatch)
    }

    /// Stage 1: Exact match on canonical_name or alias.
    async fn exact_string_match(&self, name: &str) -> Result<Option<EntityId>, GraphError> {
        let name_lower = name.to_lowercase();

        let q = query(
            "MATCH (e:Entity) \
             WHERE toLower(e.canonical_name) = $name \
             RETURN e.id AS id \
             LIMIT 1",
        )
        .param("name", name_lower.as_str());

        let mut result = self
            .graph
            .inner()
            .execute(q)
            .await
            .map_err(|e| GraphError::Query(e.to_string()))?;

        if let Some(row) = result
            .next()
            .await
            .map_err(|e| GraphError::Query(e.to_string()))?
        {
            let id_str: String = row
                .get("id")
                .map_err(|e| GraphError::Query(format!("Missing 'id': {}", e)))?;
            return Ok(Some(super::conversions::parse_entity_id(&id_str)?));
        }

        // Also check aliases (stored as JSON arrays).
        // Use fulltext index for efficiency, then verify exact match in Rust.
        let escaped_name = escape_lucene_query(name);
        let q2 = query(
            "CALL db.index.fulltext.queryNodes('entity_name_fulltext', $name) \
             YIELD node, score \
             RETURN node \
             LIMIT 10",
        )
        .param("name", escaped_name.as_str());

        let mut result2 = self
            .graph
            .inner()
            .execute(q2)
            .await
            .map_err(|e| GraphError::Query(e.to_string()))?;

        while let Some(row) = result2
            .next()
            .await
            .map_err(|e| GraphError::Query(e.to_string()))?
        {
            let node: neo4rs::Node = row
                .get("node")
                .map_err(|e| GraphError::Query(format!("Missing 'node': {}", e)))?;
            let entity = node_to_entity(&node)?;

            // Check exact match in aliases (case-insensitive).
            for alias in &entity.aliases {
                if alias.to_lowercase() == name_lower {
                    return Ok(Some(entity.id));
                }
            }
            // Also check canonical name again (the fulltext might have returned it).
            if entity.canonical_name.to_lowercase() == name_lower {
                return Ok(Some(entity.id));
            }
        }

        Ok(None)
    }

    /// Stage 2: Fuzzy string match using fulltext search + Jaro-Winkler similarity.
    async fn fuzzy_string_match(
        &self,
        name: &str,
        _kind: &str,
    ) -> Result<Option<(EntityId, f64)>, GraphError> {
        let escaped_name = escape_lucene_query(name);
        let q = query(
            "CALL db.index.fulltext.queryNodes('entity_name_fulltext', $name) \
             YIELD node, score \
             RETURN node \
             LIMIT 10",
        )
        .param("name", escaped_name.as_str());

        let mut result = self
            .graph
            .inner()
            .execute(q)
            .await
            .map_err(|e| GraphError::Query(e.to_string()))?;

        let mut best_match: Option<(EntityId, f64)> = None;

        while let Some(row) = result
            .next()
            .await
            .map_err(|e| GraphError::Query(e.to_string()))?
        {
            let node: neo4rs::Node = row
                .get("node")
                .map_err(|e| GraphError::Query(format!("Missing 'node': {}", e)))?;
            let entity = node_to_entity(&node)?;

            // Compute Jaro-Winkler against canonical name.
            let jw_score =
                strsim::jaro_winkler(&name.to_lowercase(), &entity.canonical_name.to_lowercase());

            // Also check against aliases and take the best score.
            let best_alias_score = entity
                .aliases
                .iter()
                .map(|a| strsim::jaro_winkler(&name.to_lowercase(), &a.to_lowercase()))
                .fold(0.0_f64, f64::max);

            let score = jw_score.max(best_alias_score);

            if score >= self.config.fuzzy_threshold {
                match best_match {
                    Some((_, existing_score)) if score > existing_score => {
                        best_match = Some((entity.id, score));
                    }
                    None => {
                        best_match = Some((entity.id, score));
                    }
                    _ => {}
                }
            }
        }

        Ok(best_match)
    }

    /// Stage 3: Embedding similarity via vector search.
    async fn embedding_similarity_match(
        &self,
        embedding: &[f32],
    ) -> Result<Option<(EntityId, f64)>, GraphError> {
        let emb_f64: Vec<f64> = embedding.iter().map(|&f| f as f64).collect();

        let q = query(
            "CALL db.index.vector.queryNodes('entity_embedding', 5, $embedding) \
             YIELD node, score \
             RETURN node, score \
             ORDER BY score DESC \
             LIMIT 1",
        )
        .param("embedding", emb_f64);

        let mut result = self
            .graph
            .inner()
            .execute(q)
            .await
            .map_err(|e| GraphError::Query(e.to_string()))?;

        if let Some(row) = result
            .next()
            .await
            .map_err(|e| GraphError::Query(e.to_string()))?
        {
            let score: f64 = row
                .get("score")
                .map_err(|e| GraphError::Query(format!("Missing 'score': {}", e)))?;
            let node: neo4rs::Node = row
                .get("node")
                .map_err(|e| GraphError::Query(format!("Missing 'node': {}", e)))?;
            let entity = node_to_entity(&node)?;

            if score >= self.config.embedding_threshold {
                return Ok(Some((entity.id, score)));
            }
        }

        Ok(None)
    }
}
