///! Integration tests for Neo4j graph operations.
///! All tests are `#[ignore]` — run with `cargo test -- --ignored` against a live Neo4j.
///!
///! Setup: Connect to Neo4j from env vars (or localhost defaults).
///! Each test cleans all data before running via `MATCH (n) DETACH DELETE n`.
use autosint_common::types::{AttributionDepth, Claim, Entity, Relationship};
use chrono::Utc;
use neo4rs::query;
use serde_json::json;

use autosint_engine::graph::{
    EntitySearchParams, EntityUpdate, GraphClient, RelationshipUpdate, SearchMode,
    TraversalDirection, TraversalParams,
};

async fn setup() -> GraphClient {
    let uri = std::env::var("NEO4J_URI").unwrap_or_else(|_| "bolt://localhost:7687".into());
    let user = std::env::var("NEO4J_USER").unwrap_or_else(|_| "neo4j".into());
    let password = std::env::var("NEO4J_PASSWORD").unwrap_or_else(|_| "autosint_dev".into());

    let client = GraphClient::connect(&uri, &user, &password)
        .await
        .expect("Failed to connect to Neo4j");

    // Clean all data.
    client
        .inner()
        .run(query("MATCH (n) DETACH DELETE n"))
        .await
        .expect("Failed to clean database");

    // Initialize schema.
    client
        .initialize_schema()
        .await
        .expect("Failed to initialize schema");

    client
}

// -----------------------------------------------------------------------
// 1. Entity CRUD
// -----------------------------------------------------------------------

#[tokio::test]
#[ignore]
async fn test_entity_crud() {
    let graph = setup().await;

    let entity = Entity::new("United States".into(), "country".into());
    let created = graph.create_entity(&entity, None).await.unwrap();

    assert_eq!(created.canonical_name, "United States");
    assert_eq!(created.kind, "country");
    assert!(created.embedding_pending);
    assert!(created.embedding.is_none());

    // Get.
    let fetched = graph.get_entity(created.id).await.unwrap();
    assert_eq!(fetched.id, created.id);
    assert_eq!(fetched.canonical_name, "United States");

    // Update.
    let update = EntityUpdate {
        canonical_name: None,
        aliases: Some(vec!["USA".into(), "US".into()]),
        kind: None,
        summary: Some("A country in North America.".into()),
        is_stub: None,
        properties: None,
    };
    let updated = graph
        .update_entity(created.id, &update, None)
        .await
        .unwrap();
    assert_eq!(updated.aliases, vec!["USA".to_string(), "US".to_string()]);
    assert_eq!(
        updated.summary.as_deref(),
        Some("A country in North America.")
    );
}

// -----------------------------------------------------------------------
// 2. Entity stub
// -----------------------------------------------------------------------

#[tokio::test]
#[ignore]
async fn test_entity_stub() {
    let graph = setup().await;

    let entity = Entity::new_stub("Unknown Corp".into(), "organization".into());
    let created = graph.create_entity(&entity, None).await.unwrap();

    assert!(created.is_stub);
}

// -----------------------------------------------------------------------
// 3. Entity freeform properties
// -----------------------------------------------------------------------

#[tokio::test]
#[ignore]
async fn test_entity_freeform_properties() {
    let graph = setup().await;

    let mut entity = Entity::new("Apple Inc.".into(), "organization".into());
    entity
        .properties
        .insert("stock_ticker".into(), json!("AAPL"));
    entity.properties.insert("founded_year".into(), json!(1976));
    entity
        .properties
        .insert("publicly_traded".into(), json!(true));

    let created = graph.create_entity(&entity, None).await.unwrap();

    let fetched = graph.get_entity(created.id).await.unwrap();
    assert_eq!(fetched.properties.get("stock_ticker"), Some(&json!("AAPL")));
    // Numeric values stored as strings, parsed back as JSON.
    assert_eq!(fetched.properties.get("founded_year"), Some(&json!(1976)));
    assert_eq!(
        fetched.properties.get("publicly_traded"),
        Some(&json!(true))
    );
}

// -----------------------------------------------------------------------
// 4. Claim CRUD
// -----------------------------------------------------------------------

#[tokio::test]
#[ignore]
async fn test_claim_crud() {
    let graph = setup().await;

    // Create source and referenced entities first.
    let source = Entity::new("Reuters".into(), "publication".into());
    let source = graph.create_entity(&source, None).await.unwrap();

    let ref_entity = Entity::new("China".into(), "country".into());
    let ref_entity = graph.create_entity(&ref_entity, None).await.unwrap();

    let mut claim = Claim::new(
        "China announced new trade tariffs on US goods.".into(),
        Utc::now(),
        AttributionDepth::Primary,
        source.id,
    );
    claim.referenced_entity_ids = vec![ref_entity.id];

    let created = graph.create_claim(&claim, None).await.unwrap();
    assert_eq!(created.content, claim.content);
    assert_eq!(created.source_entity_id, source.id);
    assert_eq!(created.referenced_entity_ids, vec![ref_entity.id]);

    // Get.
    let fetched = graph.get_claim(created.id).await.unwrap();
    assert_eq!(fetched.id, created.id);
    assert_eq!(fetched.source_entity_id, source.id);
    assert!(fetched.referenced_entity_ids.contains(&ref_entity.id));
}

// -----------------------------------------------------------------------
// 5. Relationship CRUD
// -----------------------------------------------------------------------

#[tokio::test]
#[ignore]
async fn test_relationship_crud() {
    let graph = setup().await;

    let e1 = graph
        .create_entity(&Entity::new("TSMC".into(), "organization".into()), None)
        .await
        .unwrap();
    let e2 = graph
        .create_entity(&Entity::new("Apple".into(), "organization".into()), None)
        .await
        .unwrap();

    let mut rel = Relationship::new(e1.id, e2.id, "Supplies A-series chips to Apple.".into());
    rel.weight = Some(0.8);
    rel.confidence = Some(0.95);

    let created = graph.create_relationship(&rel, None).await.unwrap();
    assert_eq!(created.description, "Supplies A-series chips to Apple.");
    assert_eq!(created.weight, Some(0.8));
    assert_eq!(created.source_entity_id, e1.id);
    assert_eq!(created.target_entity_id, e2.id);

    // Update.
    let update = RelationshipUpdate {
        description: Some("Primary chip supplier for Apple's A-series.".into()),
        weight: Some(0.9),
        confidence: None,
        bidirectional: None,
        timestamp: None,
    };
    let updated = graph
        .update_relationship(created.id, &update, None)
        .await
        .unwrap();
    assert_eq!(
        updated.description,
        "Primary chip supplier for Apple's A-series."
    );
    assert_eq!(updated.weight, Some(0.9));
}

// -----------------------------------------------------------------------
// 6. Relationship bidirectional
// -----------------------------------------------------------------------

#[tokio::test]
#[ignore]
async fn test_relationship_bidirectional() {
    let graph = setup().await;

    let e1 = graph
        .create_entity(&Entity::new("France".into(), "country".into()), None)
        .await
        .unwrap();
    let e2 = graph
        .create_entity(&Entity::new("Germany".into(), "country".into()), None)
        .await
        .unwrap();

    let mut rel = Relationship::new(e1.id, e2.id, "Share a border in western Europe.".into());
    rel.bidirectional = true;

    let created = graph.create_relationship(&rel, None).await.unwrap();
    assert!(created.bidirectional);

    // Traverse from e2 (incoming direction should also find it via Both).
    let from_e2 = graph
        .traverse_relationships(
            e2.id,
            &TraversalParams {
                direction: Some(TraversalDirection::Both),
                min_weight: None,
                limit: None,
            },
        )
        .await
        .unwrap();

    assert!(!from_e2.is_empty());
}

// -----------------------------------------------------------------------
// 7. Fulltext search entity name
// -----------------------------------------------------------------------

#[tokio::test]
#[ignore]
async fn test_fulltext_search_entity_name() {
    let graph = setup().await;

    graph
        .create_entity(
            &Entity::new("European Central Bank".into(), "organization".into()),
            None,
        )
        .await
        .unwrap();
    graph
        .create_entity(
            &Entity::new("Bank of Japan".into(), "organization".into()),
            None,
        )
        .await
        .unwrap();

    // Wait briefly for fulltext index to update.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let results = graph
        .search_entities(
            &EntitySearchParams {
                query: "European Central Bank".into(),
                mode: SearchMode::Keyword,
                kind_filter: None,
                updated_after: None,
                updated_before: None,
                limit: Some(5),
            },
            None,
        )
        .await
        .unwrap();

    assert!(!results.is_empty());
    assert_eq!(results[0].item.canonical_name, "European Central Bank");
}

// -----------------------------------------------------------------------
// 8. Fulltext search entity alias
// -----------------------------------------------------------------------

#[tokio::test]
#[ignore]
async fn test_fulltext_search_entity_alias() {
    let graph = setup().await;

    let mut entity = Entity::new("United States of America".into(), "country".into());
    entity.aliases = vec!["USA".into(), "United States".into()];
    graph.create_entity(&entity, None).await.unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let results = graph
        .search_entities(
            &EntitySearchParams {
                query: "USA".into(),
                mode: SearchMode::Keyword,
                kind_filter: None,
                updated_after: None,
                updated_before: None,
                limit: Some(5),
            },
            None,
        )
        .await
        .unwrap();

    assert!(!results.is_empty());
    assert_eq!(results[0].item.canonical_name, "United States of America");
}

// -----------------------------------------------------------------------
// 9. Vector search entities (injected embeddings, no OpenAI)
// -----------------------------------------------------------------------

#[tokio::test]
#[ignore]
async fn test_vector_search_entities() {
    let graph = setup().await;

    // Create entities with known embeddings (1536-dim, mostly zeros with distinct signals).
    let mut emb_a = vec![0.0f32; 1536];
    emb_a[0] = 1.0;
    emb_a[1] = 0.5;

    let mut emb_b = vec![0.0f32; 1536];
    emb_b[0] = 0.9;
    emb_b[1] = 0.6;

    let mut emb_c = vec![0.0f32; 1536];
    emb_c[100] = 1.0; // very different

    let e1 = Entity::new("Similar A".into(), "test".into());
    graph.create_entity(&e1, Some(emb_a.clone())).await.unwrap();

    let e2 = Entity::new("Similar B".into(), "test".into());
    graph.create_entity(&e2, Some(emb_b.clone())).await.unwrap();

    let e3 = Entity::new("Different C".into(), "test".into());
    graph.create_entity(&e3, Some(emb_c)).await.unwrap();

    // Wait for vector index to update.
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Search with a query vector close to emb_a.
    let results = graph
        .search_entities(
            &EntitySearchParams {
                query: String::new(), // not used for semantic search
                mode: SearchMode::Semantic,
                kind_filter: None,
                updated_after: None,
                updated_before: None,
                limit: Some(3),
            },
            Some(emb_a),
        )
        .await
        .unwrap();

    assert!(results.len() >= 2);
    // "Similar A" should be the closest match (exact vector match).
    assert_eq!(results[0].item.canonical_name, "Similar A");
}

// -----------------------------------------------------------------------
// 10. Fulltext search claims
// -----------------------------------------------------------------------

#[tokio::test]
#[ignore]
async fn test_fulltext_search_claims() {
    let graph = setup().await;

    let source = graph
        .create_entity(&Entity::new("AP News".into(), "publication".into()), None)
        .await
        .unwrap();

    let claim = Claim::new(
        "Oil prices surged amid Middle East tensions.".into(),
        Utc::now(),
        AttributionDepth::Primary,
        source.id,
    );
    graph.create_claim(&claim, None).await.unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let results = graph
        .search_claims(
            &autosint_engine::graph::ClaimSearchParams {
                query: Some("oil prices".into()),
                mode: Some(SearchMode::Keyword),
                published_after: None,
                published_before: None,
                source_entity_id: None,
                referenced_entity_id: None,
                attribution_depth: None,
                limit: Some(5),
            },
            None,
        )
        .await
        .unwrap();

    assert!(!results.is_empty());
    assert!(results[0].item.content.contains("Oil prices"));
}

// -----------------------------------------------------------------------
// 11. Claim search by source entity
// -----------------------------------------------------------------------

#[tokio::test]
#[ignore]
async fn test_claim_search_by_source_entity() {
    let graph = setup().await;

    let reuters = graph
        .create_entity(&Entity::new("Reuters".into(), "publication".into()), None)
        .await
        .unwrap();
    let bbc = graph
        .create_entity(&Entity::new("BBC".into(), "publication".into()), None)
        .await
        .unwrap();

    let claim1 = Claim::new(
        "Claim from Reuters.".into(),
        Utc::now(),
        AttributionDepth::Primary,
        reuters.id,
    );
    graph.create_claim(&claim1, None).await.unwrap();

    let claim2 = Claim::new(
        "Claim from BBC.".into(),
        Utc::now(),
        AttributionDepth::Primary,
        bbc.id,
    );
    graph.create_claim(&claim2, None).await.unwrap();

    let results = graph
        .search_claims(
            &autosint_engine::graph::ClaimSearchParams {
                query: None,
                mode: None,
                published_after: None,
                published_before: None,
                source_entity_id: Some(reuters.id),
                referenced_entity_id: None,
                attribution_depth: None,
                limit: Some(10),
            },
            None,
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    assert!(results[0].item.content.contains("Reuters"));
}

// -----------------------------------------------------------------------
// 12. Claim search by referenced entity
// -----------------------------------------------------------------------

#[tokio::test]
#[ignore]
async fn test_claim_search_by_referenced_entity() {
    let graph = setup().await;

    let source = graph
        .create_entity(&Entity::new("AP".into(), "publication".into()), None)
        .await
        .unwrap();
    let china = graph
        .create_entity(&Entity::new("China".into(), "country".into()), None)
        .await
        .unwrap();
    let japan = graph
        .create_entity(&Entity::new("Japan".into(), "country".into()), None)
        .await
        .unwrap();

    let mut c1 = Claim::new(
        "Claim about China.".into(),
        Utc::now(),
        AttributionDepth::Primary,
        source.id,
    );
    c1.referenced_entity_ids = vec![china.id];
    graph.create_claim(&c1, None).await.unwrap();

    let mut c2 = Claim::new(
        "Claim about Japan.".into(),
        Utc::now(),
        AttributionDepth::Primary,
        source.id,
    );
    c2.referenced_entity_ids = vec![japan.id];
    graph.create_claim(&c2, None).await.unwrap();

    let results = graph
        .search_claims(
            &autosint_engine::graph::ClaimSearchParams {
                query: None,
                mode: None,
                published_after: None,
                published_before: None,
                source_entity_id: None,
                referenced_entity_id: Some(china.id),
                attribution_depth: None,
                limit: Some(10),
            },
            None,
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    assert!(results[0].item.content.contains("China"));
}

// -----------------------------------------------------------------------
// 13. Claim search temporal
// -----------------------------------------------------------------------

#[tokio::test]
#[ignore]
async fn test_claim_search_temporal() {
    let graph = setup().await;

    let source = graph
        .create_entity(&Entity::new("Source".into(), "publication".into()), None)
        .await
        .unwrap();

    let old_date = chrono::DateTime::parse_from_rfc3339("2020-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let new_date = chrono::DateTime::parse_from_rfc3339("2025-06-15T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);

    let c_old = Claim::new(
        "Old claim.".into(),
        old_date,
        AttributionDepth::Primary,
        source.id,
    );
    graph.create_claim(&c_old, None).await.unwrap();

    let c_new = Claim::new(
        "New claim.".into(),
        new_date,
        AttributionDepth::Primary,
        source.id,
    );
    graph.create_claim(&c_new, None).await.unwrap();

    let cutoff = chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);

    let results = graph
        .search_claims(
            &autosint_engine::graph::ClaimSearchParams {
                query: None,
                mode: None,
                published_after: Some(cutoff),
                published_before: None,
                source_entity_id: None,
                referenced_entity_id: None,
                attribution_depth: None,
                limit: Some(10),
            },
            None,
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    assert!(results[0].item.content.contains("New"));
}

// -----------------------------------------------------------------------
// 14. Claim search attribution depth
// -----------------------------------------------------------------------

#[tokio::test]
#[ignore]
async fn test_claim_search_attribution_depth() {
    let graph = setup().await;

    let source = graph
        .create_entity(&Entity::new("Source".into(), "publication".into()), None)
        .await
        .unwrap();

    let c_primary = Claim::new(
        "Primary claim.".into(),
        Utc::now(),
        AttributionDepth::Primary,
        source.id,
    );
    graph.create_claim(&c_primary, None).await.unwrap();

    let c_secondhand = Claim::new(
        "Secondhand claim.".into(),
        Utc::now(),
        AttributionDepth::Secondhand,
        source.id,
    );
    graph.create_claim(&c_secondhand, None).await.unwrap();

    let results = graph
        .search_claims(
            &autosint_engine::graph::ClaimSearchParams {
                query: None,
                mode: None,
                published_after: None,
                published_before: None,
                source_entity_id: None,
                referenced_entity_id: None,
                attribution_depth: Some(AttributionDepth::Primary),
                limit: Some(10),
            },
            None,
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    assert!(results[0].item.content.contains("Primary"));
}

// -----------------------------------------------------------------------
// 15. Relationship traverse
// -----------------------------------------------------------------------

#[tokio::test]
#[ignore]
async fn test_relationship_traverse() {
    let graph = setup().await;

    let a = graph
        .create_entity(&Entity::new("A".into(), "test".into()), None)
        .await
        .unwrap();
    let b = graph
        .create_entity(&Entity::new("B".into(), "test".into()), None)
        .await
        .unwrap();
    let c = graph
        .create_entity(&Entity::new("C".into(), "test".into()), None)
        .await
        .unwrap();

    let r1 = Relationship::new(a.id, b.id, "A to B".into());
    graph.create_relationship(&r1, None).await.unwrap();

    let r2 = Relationship::new(b.id, c.id, "B to C".into());
    graph.create_relationship(&r2, None).await.unwrap();

    // Traverse outgoing from A — should find B.
    let from_a = graph
        .traverse_relationships(
            a.id,
            &TraversalParams {
                direction: Some(TraversalDirection::Outgoing),
                min_weight: None,
                limit: None,
            },
        )
        .await
        .unwrap();

    assert_eq!(from_a.len(), 1);
    assert_eq!(from_a[0].1.canonical_name, "B");
}

// -----------------------------------------------------------------------
// 16. Relationship semantic search (injected embeddings)
// -----------------------------------------------------------------------

#[tokio::test]
#[ignore]
async fn test_relationship_semantic_search() {
    let graph = setup().await;

    let e1 = graph
        .create_entity(&Entity::new("X".into(), "test".into()), None)
        .await
        .unwrap();
    let e2 = graph
        .create_entity(&Entity::new("Y".into(), "test".into()), None)
        .await
        .unwrap();

    let mut emb = vec![0.0f32; 1536];
    emb[0] = 1.0;

    let rel = Relationship::new(e1.id, e2.id, "X supplies widgets to Y.".into());
    graph
        .create_relationship(&rel, Some(emb.clone()))
        .await
        .unwrap();

    // Wait for vector index.
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let results = graph
        .search_relationships(
            &autosint_engine::graph::RelationshipSearchParams {
                query: String::new(),
                limit: Some(5),
            },
            Some(emb),
        )
        .await
        .unwrap();

    assert!(!results.is_empty());
    assert!(results[0].item.description.contains("widgets"));
}

// -----------------------------------------------------------------------
// 17. Entity merge
// -----------------------------------------------------------------------

#[tokio::test]
#[ignore]
async fn test_entity_merge() {
    let graph = setup().await;

    let source_pub = graph
        .create_entity(&Entity::new("NYT".into(), "publication".into()), None)
        .await
        .unwrap();

    // Create two entities that represent the same thing.
    let mut e1 = Entity::new("United States".into(), "country".into());
    e1.aliases = vec!["USA".into()];
    let e1 = graph.create_entity(&e1, None).await.unwrap();

    let mut e2 = Entity::new("US".into(), "country".into());
    e2.aliases = vec!["America".into()];
    let e2 = graph.create_entity(&e2, None).await.unwrap();

    // Create a claim referencing e1.
    let mut claim = Claim::new(
        "US GDP grew.".into(),
        Utc::now(),
        AttributionDepth::Primary,
        source_pub.id,
    );
    claim.referenced_entity_ids = vec![e1.id];
    graph.create_claim(&claim, None).await.unwrap();

    // Create a relationship from e1 to another entity.
    let other = graph
        .create_entity(&Entity::new("Canada".into(), "country".into()), None)
        .await
        .unwrap();
    let rel = Relationship::new(e1.id, other.id, "Shares border with.".into());
    graph.create_relationship(&rel, None).await.unwrap();

    // Merge e1 (source) into e2 (target).
    let merged = graph
        .merge_entities(e1.id, e2.id, Some("Duplicate entity"))
        .await
        .unwrap();

    // Verify merged entity.
    assert_eq!(merged.id, e2.id);
    assert!(merged.aliases.contains(&"United States".to_string()));
    assert!(merged.aliases.contains(&"USA".to_string()));
    assert!(merged.aliases.contains(&"America".to_string()));

    // Verify source entity is deleted.
    let get_result = graph.get_entity(e1.id).await;
    assert!(get_result.is_err());

    // Verify claim REFERENCES edge was reassigned to e2.
    let fetched_claim = graph.get_claim(claim.id).await.unwrap();
    assert!(fetched_claim.referenced_entity_ids.contains(&e2.id));
    assert!(!fetched_claim.referenced_entity_ids.contains(&e1.id));

    // Verify relationship was reassigned to e2.
    let rels = graph
        .traverse_relationships(
            e2.id,
            &TraversalParams {
                direction: Some(TraversalDirection::Outgoing),
                min_weight: None,
                limit: None,
            },
        )
        .await
        .unwrap();

    assert!(!rels.is_empty());
    assert_eq!(rels[0].1.canonical_name, "Canada");
}

// -----------------------------------------------------------------------
// 18. Dedup exact match
// -----------------------------------------------------------------------

#[tokio::test]
#[ignore]
async fn test_dedup_exact_match() {
    let graph = setup().await;

    graph
        .create_entity(&Entity::new("United States".into(), "country".into()), None)
        .await
        .unwrap();

    let config = autosint_common::config::DedupConfig {
        fuzzy_threshold: 0.85,
        embedding_threshold: 0.90,
    };

    let dedup = autosint_engine::graph::dedup::EntityDedup::new(&graph, &config, None);
    let result = dedup
        .find_duplicate("United States", "country", None)
        .await
        .unwrap();

    match result {
        autosint_engine::graph::DedupResult::ExactMatch(id) => {
            let entity = graph.get_entity(id).await.unwrap();
            assert_eq!(entity.canonical_name, "United States");
        }
        _ => panic!("Expected ExactMatch"),
    }
}

// -----------------------------------------------------------------------
// 19. Dedup fuzzy match
// -----------------------------------------------------------------------

#[tokio::test]
#[ignore]
async fn test_dedup_fuzzy_match() {
    let graph = setup().await;

    let mut entity = Entity::new("United States of America".into(), "country".into());
    entity.aliases = vec!["United States".into(), "USA".into()];
    graph.create_entity(&entity, None).await.unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let config = autosint_common::config::DedupConfig {
        fuzzy_threshold: 0.85,
        embedding_threshold: 0.90,
    };

    let dedup = autosint_engine::graph::dedup::EntityDedup::new(&graph, &config, None);
    // "United States" should fuzzy-match "United States of America".
    let result = dedup
        .find_duplicate("United States", "country", None)
        .await
        .unwrap();

    match result {
        autosint_engine::graph::DedupResult::ExactMatch(_) => {
            // Exact match via alias is also acceptable.
        }
        autosint_engine::graph::DedupResult::ProbableMatch { confidence, .. } => {
            assert!(confidence >= 0.85);
        }
        autosint_engine::graph::DedupResult::NoMatch => {
            panic!("Expected a match for 'United States' vs 'United States of America'");
        }
    }
}

// -----------------------------------------------------------------------
// 20. Dedup no match
// -----------------------------------------------------------------------

#[tokio::test]
#[ignore]
async fn test_dedup_no_match() {
    let graph = setup().await;

    graph
        .create_entity(&Entity::new("Japan".into(), "country".into()), None)
        .await
        .unwrap();

    let config = autosint_common::config::DedupConfig {
        fuzzy_threshold: 0.85,
        embedding_threshold: 0.90,
    };

    let dedup = autosint_engine::graph::dedup::EntityDedup::new(&graph, &config, None);
    let result = dedup
        .find_duplicate("Completely Unrelated Name XYZ", "thing", None)
        .await
        .unwrap();

    match result {
        autosint_engine::graph::DedupResult::NoMatch => {}
        _ => panic!("Expected NoMatch"),
    }
}

// -----------------------------------------------------------------------
// 21. Embedding pending backfill (mock — verifies the flag flow)
// -----------------------------------------------------------------------

#[tokio::test]
#[ignore]
async fn test_embedding_pending_flag() {
    let graph = setup().await;

    // Create entity without embedding — should set embedding_pending = true.
    let entity = Entity::new("Test Entity".into(), "test".into());
    let created = graph.create_entity(&entity, None).await.unwrap();

    assert!(created.embedding_pending);
    assert!(created.embedding.is_none());

    // Create entity with embedding — should set embedding_pending = false.
    let entity2 = Entity::new("Embedded Entity".into(), "test".into());
    let emb = vec![0.1f32; 1536];
    let created2 = graph.create_entity(&entity2, Some(emb)).await.unwrap();

    assert!(!created2.embedding_pending);
    assert!(created2.embedding.is_some());
}
