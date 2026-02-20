mod create_claim;
mod create_entity;
mod create_relationship;
mod fetch_source_catalog;
mod fetch_source_query;
mod fetch_url;
mod search_entities;
mod update_entity;
mod update_entity_with_change_claim;
mod update_relationship;

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
}
