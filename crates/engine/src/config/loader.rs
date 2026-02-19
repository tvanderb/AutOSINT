use std::collections::HashMap;
use std::path::{Path, PathBuf};

use autosint_common::config::SystemConfig;
use serde_json::Value;

use super::validation;

/// Complete engine configuration loaded from the config directory.
#[derive(Clone, Debug)]
pub struct EngineConfig {
    /// Parsed system.toml.
    pub system: SystemConfig,
    /// Tool schemas keyed by "{role}/{tool_name}" (e.g. "analyst/search_entities").
    pub tool_schemas: HashMap<String, Value>,
    /// Prompt templates keyed by filename stem (e.g. "analyst", "processor").
    pub prompts: HashMap<String, String>,
    /// Base config directory path (used for future config reload).
    #[allow(dead_code)]
    pub config_dir: PathBuf,
}

/// Load all configuration from the given config directory.
///
/// Fails loudly with clear error messages if anything is misconfigured.
/// The Engine refuses to start on validation failure (PLAN.md §4.9).
pub fn load_config(config_dir: &Path) -> Result<EngineConfig, ConfigError> {
    tracing::info!(config_dir = %config_dir.display(), "Loading configuration");

    // 1. Load and parse system.toml
    let system_path = config_dir.join("system.toml");
    let system = load_system_config(&system_path)?;

    // 2. Load tool schemas from config/tools/{role}/*.json
    let tool_schemas = load_tool_schemas(&config_dir.join("tools"))?;

    // 3. Load prompt templates from config/prompts/*.md
    let prompts = load_prompts(&config_dir.join("prompts"))?;

    let config = EngineConfig {
        system,
        tool_schemas,
        prompts,
        config_dir: config_dir.to_path_buf(),
    };

    // 4. Validate everything
    validation::validate(&config)?;

    tracing::info!(
        tool_schemas = config.tool_schemas.len(),
        prompts = config.prompts.len(),
        "Configuration loaded successfully"
    );

    Ok(config)
}

fn load_system_config(path: &Path) -> Result<SystemConfig, ConfigError> {
    let content = std::fs::read_to_string(path).map_err(|e| ConfigError::FileRead {
        path: path.to_path_buf(),
        source: e,
    })?;

    toml::from_str(&content).map_err(|e| ConfigError::Parse {
        path: path.to_path_buf(),
        detail: e.to_string(),
    })
}

fn load_tool_schemas(tools_dir: &Path) -> Result<HashMap<String, Value>, ConfigError> {
    let mut schemas = HashMap::new();

    if !tools_dir.exists() {
        tracing::warn!(
            path = %tools_dir.display(),
            "Tools directory does not exist, no tool schemas loaded"
        );
        return Ok(schemas);
    }

    // Iterate over role directories (analyst/, processor/)
    let entries = std::fs::read_dir(tools_dir).map_err(|e| ConfigError::FileRead {
        path: tools_dir.to_path_buf(),
        source: e,
    })?;

    for entry in entries {
        let entry = entry.map_err(|e| ConfigError::FileRead {
            path: tools_dir.to_path_buf(),
            source: e,
        })?;

        let role_path = entry.path();
        if !role_path.is_dir() {
            continue;
        }

        let role_name = role_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");

        let role_entries = std::fs::read_dir(&role_path).map_err(|e| ConfigError::FileRead {
            path: role_path.clone(),
            source: e,
        })?;

        for file_entry in role_entries {
            let file_entry = file_entry.map_err(|e| ConfigError::FileRead {
                path: role_path.clone(),
                source: e,
            })?;

            let file_path = file_entry.path();
            if file_path.extension().is_some_and(|ext| ext == "json") {
                let tool_name = file_path
                    .file_stem()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown");

                let content =
                    std::fs::read_to_string(&file_path).map_err(|e| ConfigError::FileRead {
                        path: file_path.clone(),
                        source: e,
                    })?;

                let schema: Value =
                    serde_json::from_str(&content).map_err(|e| ConfigError::Parse {
                        path: file_path.clone(),
                        detail: e.to_string(),
                    })?;

                let key = format!("{}/{}", role_name, tool_name);
                tracing::debug!(tool = %key, "Loaded tool schema");
                schemas.insert(key, schema);
            }
        }
    }

    Ok(schemas)
}

fn load_prompts(prompts_dir: &Path) -> Result<HashMap<String, String>, ConfigError> {
    let mut prompts = HashMap::new();

    if !prompts_dir.exists() {
        tracing::warn!(
            path = %prompts_dir.display(),
            "Prompts directory does not exist, no prompts loaded"
        );
        return Ok(prompts);
    }

    let entries = std::fs::read_dir(prompts_dir).map_err(|e| ConfigError::FileRead {
        path: prompts_dir.to_path_buf(),
        source: e,
    })?;

    for entry in entries {
        let entry = entry.map_err(|e| ConfigError::FileRead {
            path: prompts_dir.to_path_buf(),
            source: e,
        })?;

        let path = entry.path();
        if path
            .extension()
            .is_some_and(|ext| ext == "md" || ext == "txt")
        {
            let name = path
                .file_stem()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();

            let content = std::fs::read_to_string(&path).map_err(|e| ConfigError::FileRead {
                path: path.clone(),
                source: e,
            })?;

            tracing::debug!(prompt = %name, "Loaded prompt template");
            prompts.insert(name, content);
        }
    }

    Ok(prompts)
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Failed to read {path}: {source}")]
    FileRead {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Failed to parse {path}: {detail}")]
    Parse { path: PathBuf, detail: String },

    #[error("Validation failed: {0}")]
    Validation(String),
}
