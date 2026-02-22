mod batch_extract;
mod create_claim;
mod create_entity;
mod create_relationship;
mod create_work_order;
mod fetch_source_catalog;
mod fetch_source_query;
mod fetch_url;
mod get_assessment;
mod get_entity;
mod get_investigation_history;
mod list_fetch_sources;
mod merge_entities;
mod produce_assessment;
mod query_geo;
mod search_assessments;
mod search_claims;
mod search_entities;
mod search_relationships;
mod traverse_relationships;
mod update_entity;
mod update_entity_with_change_claim;
mod update_relationship;
mod web_search;

use super::registry::ToolRegistry;

/// Register all Processor tool handlers with the registry.
pub fn register_processor_tools(registry: &mut ToolRegistry) {
    registry.register("search_entities", search_entities::handler());
    registry.register("create_entity", create_entity::handler());
    registry.register("update_entity", update_entity::handler());
    registry.register("create_claim", create_claim::handler());
    registry.register("create_relationship", create_relationship::handler());
    registry.register("update_relationship", update_relationship::handler());
    registry.register("fetch_url", fetch_url::handler());
    registry.register(
        "update_entity_with_change_claim",
        update_entity_with_change_claim::handler(),
    );
    registry.register("fetch_source_catalog", fetch_source_catalog::handler());
    registry.register("fetch_source_query", fetch_source_query::handler());
    registry.register("web_search", web_search::handler());
    registry.register("batch_extract", batch_extract::handler());
}

/// Register all Analyst tool handlers with the registry.
pub fn register_analyst_tools(registry: &mut ToolRegistry) {
    // Graph read tools (shared with Processor where applicable).
    registry.register("search_entities", search_entities::handler());
    registry.register("get_entity", get_entity::handler());
    registry.register("traverse_relationships", traverse_relationships::handler());
    registry.register("search_relationships", search_relationships::handler());
    registry.register("search_claims", search_claims::handler());

    // Assessment store tools.
    registry.register("search_assessments", search_assessments::handler());
    registry.register("get_assessment", get_assessment::handler());

    // Investigation action tools.
    registry.register("create_work_order", create_work_order::handler());
    registry.register("produce_assessment", produce_assessment::handler());

    // Graph maintenance tools.
    registry.register("merge_entities", merge_entities::handler());

    // Investigation context tools.
    registry.register(
        "get_investigation_history",
        get_investigation_history::handler(),
    );
    registry.register("list_fetch_sources", list_fetch_sources::handler());

    // Geographic intelligence (stub until M5).
    registry.register("query_geo", query_geo::handler());
}
