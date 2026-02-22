use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde::Deserialize;
use serde_json::{json, Value};

use autosint_common::types::{AttributionDepth, Claim, Entity, InformationType, Relationship};
use autosint_common::EntityId;

use crate::graph::conversions::{
    embedding_text_for_claim, embedding_text_for_entity, embedding_text_for_relationship,
};
use crate::graph::dedup::{DedupResult, EntityDedup};
use crate::tools::registry::{ToolHandler, ToolHandlerContext};

#[derive(Deserialize)]
struct Args {
    source_entity_id: String,
    source_url: String,
    published_timestamp: String,
    #[serde(default)]
    entities: Vec<EntityArg>,
    #[serde(default)]
    claims: Vec<ClaimArg>,
    #[serde(default)]
    relationships: Vec<RelationshipArg>,
}

#[derive(Deserialize)]
struct EntityArg {
    canonical_name: String,
    kind: String,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    properties: Option<HashMap<String, Value>>,
}

#[derive(Deserialize)]
struct ClaimArg {
    content: String,
    #[serde(default = "default_attribution")]
    attribution_depth: String,
    #[serde(default = "default_information_type")]
    information_type: String,
    #[serde(default)]
    referenced_entity_names: Vec<String>,
}

#[derive(Deserialize)]
struct RelationshipArg {
    source_entity_name: String,
    target_entity_name: String,
    description: String,
    #[serde(default)]
    confidence: Option<f64>,
}

fn default_attribution() -> String {
    "secondhand".to_string()
}

fn default_information_type() -> String {
    "assertion".to_string()
}

pub fn handler() -> ToolHandler {
    Arc::new(|args: Value, ctx: Arc<ToolHandlerContext>| {
        Box::pin(async move {
            let args: Args =
                serde_json::from_value(args).map_err(|e| format!("Invalid arguments: {}", e))?;

            let source_entity_id: EntityId = args
                .source_entity_id
                .parse::<uuid::Uuid>()
                .map(EntityId::from_uuid)
                .map_err(|e| format!("Invalid source_entity_id: {}", e))?;

            let published_timestamp =
                chrono::DateTime::parse_from_rfc3339(&args.published_timestamp)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .map_err(|e| {
                        format!("Invalid published_timestamp (expected RFC3339): {}", e)
                    })?;

            let mut warnings: Vec<String> = Vec::new();
            let mut entities_created: u32 = 0;
            let mut entities_matched: u32 = 0;
            let mut claims_created: u32 = 0;
            let mut relationships_created: u32 = 0;

            // ---------------------------------------------------------------
            // Phase 1: Entity resolution — dedup each entity, build name→id map
            // ---------------------------------------------------------------
            let mut name_to_id: HashMap<String, EntityId> = HashMap::new();

            let dedup = EntityDedup::new(&ctx.graph, &ctx.dedup_config, None);

            for entity_arg in &args.entities {
                // Compute embedding for dedup + storage.
                let embed_text = embedding_text_for_entity(
                    &entity_arg.canonical_name,
                    entity_arg.summary.as_deref(),
                );
                let embedding = if let Some(ref emb_client) = ctx.embedding_client {
                    match emb_client.embed_single(&embed_text).await {
                        Ok(emb) => Some(emb),
                        Err(e) => {
                            tracing::warn!(
                                entity = %entity_arg.canonical_name,
                                error = %e,
                                "Failed to compute entity embedding"
                            );
                            None
                        }
                    }
                } else {
                    None
                };

                let dedup_result = match dedup
                    .find_duplicate(
                        &entity_arg.canonical_name,
                        &entity_arg.kind,
                        embedding.as_deref(),
                    )
                    .await
                {
                    Ok(r) => r,
                    Err(e) => {
                        warnings.push(format!(
                            "Dedup failed for '{}': {}",
                            entity_arg.canonical_name, e
                        ));
                        continue;
                    }
                };

                match dedup_result {
                    DedupResult::ExactMatch(entity_id)
                    | DedupResult::ProbableMatch { entity_id, .. } => {
                        name_to_id.insert(entity_arg.canonical_name.to_lowercase(), entity_id);
                        entities_matched += 1;
                    }
                    DedupResult::NoMatch => {
                        let mut entity =
                            Entity::new(entity_arg.canonical_name.clone(), entity_arg.kind.clone());
                        entity.summary = entity_arg.summary.clone();
                        if let Some(ref props) = entity_arg.properties {
                            entity.properties = props.clone();
                        }

                        match ctx.graph.create_entity(&entity, embedding).await {
                            Ok(created) => {
                                name_to_id
                                    .insert(entity_arg.canonical_name.to_lowercase(), created.id);
                                entities_created += 1;
                                ctx.session_counters
                                    .entities_created
                                    .fetch_add(1, Ordering::Relaxed);
                            }
                            Err(e) => {
                                warnings.push(format!(
                                    "Failed to create entity '{}': {}",
                                    entity_arg.canonical_name, e
                                ));
                            }
                        }
                    }
                }
            }

            // ---------------------------------------------------------------
            // Phase 2: Claims
            // ---------------------------------------------------------------
            for claim_arg in &args.claims {
                let attribution_depth = match claim_arg.attribution_depth.as_str() {
                    "primary" => AttributionDepth::Primary,
                    "secondhand" | "secondary" => AttributionDepth::Secondhand,
                    "indirect" | "tertiary" => AttributionDepth::Indirect,
                    other => {
                        warnings.push(format!(
                            "Invalid attribution_depth '{}' for claim, skipping",
                            other
                        ));
                        continue;
                    }
                };

                let information_type = match claim_arg.information_type.as_str() {
                    "assertion" => InformationType::Assertion,
                    "analysis" => InformationType::Analysis,
                    "discourse" => InformationType::Discourse,
                    "testimony" => InformationType::Testimony,
                    other => {
                        warnings.push(format!(
                            "Invalid information_type '{}' for claim, skipping",
                            other
                        ));
                        continue;
                    }
                };

                // Resolve referenced entity names to IDs.
                let mut referenced_ids = Vec::new();
                for name in &claim_arg.referenced_entity_names {
                    if let Some(id) = name_to_id.get(&name.to_lowercase()) {
                        referenced_ids.push(*id);
                    } else {
                        // Fallback: search graph by name.
                        match resolve_entity_by_name(&ctx, name).await {
                            Some(id) => {
                                name_to_id.insert(name.to_lowercase(), id);
                                referenced_ids.push(id);
                            }
                            None => {
                                warnings.push(format!(
                                    "Could not resolve entity '{}' for claim reference",
                                    name
                                ));
                            }
                        }
                    }
                }

                // Compute embedding.
                let embed_text = embedding_text_for_claim(&claim_arg.content);
                let embedding = if let Some(ref emb_client) = ctx.embedding_client {
                    match emb_client.embed_single(&embed_text).await {
                        Ok(emb) => Some(emb),
                        Err(e) => {
                            tracing::warn!(error = %e, "Failed to compute claim embedding");
                            None
                        }
                    }
                } else {
                    None
                };

                let mut claim = Claim::new(
                    claim_arg.content.clone(),
                    published_timestamp,
                    attribution_depth,
                    information_type,
                    source_entity_id,
                );
                claim.referenced_entity_ids = referenced_ids;
                claim.raw_source_link = Some(args.source_url.clone());

                match ctx.graph.create_claim(&claim, embedding).await {
                    Ok(_) => {
                        claims_created += 1;
                        ctx.session_counters
                            .claims_created
                            .fetch_add(1, Ordering::Relaxed);
                    }
                    Err(e) => {
                        warnings.push(format!("Failed to create claim: {}", e));
                    }
                }
            }

            // ---------------------------------------------------------------
            // Phase 3: Relationships
            // ---------------------------------------------------------------
            for rel_arg in args.relationships.iter() {
                let source_id = match resolve_name_to_id(
                    &ctx,
                    &name_to_id,
                    &rel_arg.source_entity_name,
                )
                .await
                {
                    Some(id) => id,
                    None => {
                        warnings.push(format!(
                            "Could not resolve source entity '{}' for relationship",
                            rel_arg.source_entity_name
                        ));
                        continue;
                    }
                };

                let target_id = match resolve_name_to_id(
                    &ctx,
                    &name_to_id,
                    &rel_arg.target_entity_name,
                )
                .await
                {
                    Some(id) => id,
                    None => {
                        warnings.push(format!(
                            "Could not resolve target entity '{}' for relationship",
                            rel_arg.target_entity_name
                        ));
                        continue;
                    }
                };

                let embed_text = embedding_text_for_relationship(&rel_arg.description);
                let embedding = if let Some(ref emb_client) = ctx.embedding_client {
                    match emb_client.embed_single(&embed_text).await {
                        Ok(emb) => Some(emb),
                        Err(e) => {
                            tracing::warn!(error = %e, "Failed to compute relationship embedding");
                            None
                        }
                    }
                } else {
                    None
                };

                let mut relationship =
                    Relationship::new(source_id, target_id, rel_arg.description.clone());
                relationship.confidence = rel_arg.confidence;

                match ctx
                    .graph
                    .create_relationship(&relationship, embedding)
                    .await
                {
                    Ok(_) => {
                        relationships_created += 1;
                        ctx.session_counters
                            .relationships_created
                            .fetch_add(1, Ordering::Relaxed);
                    }
                    Err(e) => {
                        warnings.push(format!("Failed to create relationship: {}", e));
                    }
                }
            }

            let mut result = json!({
                "entities_created": entities_created,
                "entities_matched": entities_matched,
                "claims_created": claims_created,
                "relationships_created": relationships_created,
                "message": format!(
                    "Batch extraction complete: {} entities created, {} matched, {} claims, {} relationships.",
                    entities_created, entities_matched, claims_created, relationships_created
                )
            });

            if !warnings.is_empty() {
                result["warnings"] = json!(warnings);
            }

            Ok(result)
        })
    })
}

/// Resolve an entity name to an ID, first checking the local map, then searching the graph.
async fn resolve_name_to_id(
    ctx: &Arc<ToolHandlerContext>,
    name_to_id: &HashMap<String, EntityId>,
    name: &str,
) -> Option<EntityId> {
    if let Some(id) = name_to_id.get(&name.to_lowercase()) {
        return Some(*id);
    }
    resolve_entity_by_name(ctx, name).await
}

/// Search the graph for an entity by name (exact match via dedup stage 1).
async fn resolve_entity_by_name(ctx: &Arc<ToolHandlerContext>, name: &str) -> Option<EntityId> {
    let dedup = EntityDedup::new(&ctx.graph, &ctx.dedup_config, None);
    match dedup.find_duplicate(name, "", None).await {
        Ok(DedupResult::ExactMatch(id)) | Ok(DedupResult::ProbableMatch { entity_id: id, .. }) => {
            Some(id)
        }
        _ => None,
    }
}
