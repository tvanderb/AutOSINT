use super::loader::{ConfigError, EngineConfig};

/// Validate the complete engine configuration.
///
/// Checks sane ranges on numeric parameters and cross-validates
/// that referenced files/schemas exist. Engine refuses to start
/// on validation failure (PLAN.md §4.9).
pub fn validate(config: &EngineConfig) -> Result<(), ConfigError> {
    let mut errors: Vec<String> = Vec::new();

    validate_safety_limits(config, &mut errors);
    validate_concurrency(config, &mut errors);
    validate_llm(config, &mut errors);
    validate_embeddings(config, &mut errors);
    validate_dedup(config, &mut errors);
    validate_retry(config, &mut errors);

    if errors.is_empty() {
        Ok(())
    } else {
        Err(ConfigError::Validation(errors.join("; ")))
    }
}

fn validate_safety_limits(config: &EngineConfig, errors: &mut Vec<String>) {
    let s = &config.system.safety;

    if s.max_cycles_per_investigation == 0 {
        errors.push("safety.max_cycles_per_investigation must be > 0".into());
    }
    if s.max_turns_per_analyst_session == 0 {
        errors.push("safety.max_turns_per_analyst_session must be > 0".into());
    }
    if s.max_work_orders_per_cycle == 0 {
        errors.push("safety.max_work_orders_per_cycle must be > 0".into());
    }
    if s.heartbeat_ttl_seconds < 5 {
        errors.push("safety.heartbeat_ttl_seconds must be >= 5".into());
    }
    if s.consecutive_all_fail_limit == 0 {
        errors.push("safety.consecutive_all_fail_limit must be > 0".into());
    }
    if s.max_consecutive_malformed_tool_calls == 0 {
        errors.push("safety.max_consecutive_malformed_tool_calls must be > 0".into());
    }
}

fn validate_concurrency(config: &EngineConfig, errors: &mut Vec<String>) {
    let c = &config.system.concurrency;

    if c.processor_pool_size == 0 {
        errors.push("concurrency.processor_pool_size must be > 0".into());
    }
    if c.browser_context_cap == 0 {
        errors.push("concurrency.browser_context_cap must be > 0".into());
    }
}

fn validate_llm(config: &EngineConfig, errors: &mut Vec<String>) {
    let validate_role =
        |role: &autosint_common::config::LlmRoleConfig, name: &str, errors: &mut Vec<String>| {
            if role.provider.is_empty() {
                errors.push(format!("llm.{}.provider must not be empty", name));
            }
            if role.model.is_empty() {
                errors.push(format!("llm.{}.model must not be empty", name));
            }
            if role.max_tokens == 0 {
                errors.push(format!("llm.{}.max_tokens must be > 0", name));
            }
            if let Some(temp) = role.temperature {
                if !(0.0..=2.0).contains(&temp) {
                    errors.push(format!(
                        "llm.{}.temperature must be between 0.0 and 2.0",
                        name
                    ));
                }
            }
        };

    validate_role(&config.system.llm.analyst, "analyst", errors);
    validate_role(&config.system.llm.processor, "processor", errors);
}

fn validate_embeddings(config: &EngineConfig, errors: &mut Vec<String>) {
    let e = &config.system.embeddings;

    if e.provider.is_empty() {
        errors.push("embeddings.provider must not be empty".into());
    }
    if e.model.is_empty() {
        errors.push("embeddings.model must not be empty".into());
    }
    if e.dimensions == 0 {
        errors.push("embeddings.dimensions must be > 0".into());
    }
    if e.batch_size == 0 {
        errors.push("embeddings.batch_size must be > 0".into());
    }
}

fn validate_dedup(config: &EngineConfig, errors: &mut Vec<String>) {
    let d = &config.system.dedup;

    if !(0.0..=1.0).contains(&d.fuzzy_threshold) {
        errors.push("dedup.fuzzy_threshold must be between 0.0 and 1.0".into());
    }
    if !(0.0..=1.0).contains(&d.embedding_threshold) {
        errors.push("dedup.embedding_threshold must be between 0.0 and 1.0".into());
    }
}

fn validate_retry(config: &EngineConfig, errors: &mut Vec<String>) {
    let validate_one =
        |rc: &autosint_common::config::RetryConfig, name: &str, errors: &mut Vec<String>| {
            if rc.max_attempts == 0 {
                errors.push(format!("retry.{}.max_attempts must be > 0", name));
            }
            if rc.initial_backoff_ms == 0 {
                errors.push(format!("retry.{}.initial_backoff_ms must be > 0", name));
            }
            if rc.max_backoff_ms < rc.initial_backoff_ms {
                errors.push(format!(
                    "retry.{}.max_backoff_ms must be >= initial_backoff_ms",
                    name
                ));
            }
            if rc.backoff_multiplier < 1.0 {
                errors.push(format!("retry.{}.backoff_multiplier must be >= 1.0", name));
            }
        };

    validate_one(&config.system.retry.llm_api, "llm_api", errors);
    validate_one(&config.system.retry.databases, "databases", errors);
    validate_one(
        &config.system.retry.external_modules,
        "external_modules",
        errors,
    );
}
