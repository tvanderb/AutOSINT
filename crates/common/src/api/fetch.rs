use serde::{Deserialize, Serialize};
use serde_json::Value;

/// POST /fetch request — raw HTTP fetch.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FetchRequest {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<FetchOptions>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FetchOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
    /// Additional headers as key-value pairs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headers: Option<std::collections::HashMap<String, String>>,
}

/// POST /fetch response.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FetchResponse {
    pub content: String,
    pub metadata: FetchMetadata,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FetchMetadata {
    pub status_code: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    pub url: String,
    /// Whether the response was served from cache.
    #[serde(default)]
    pub cached: bool,
}

/// POST /browse request — simple browser-automated render.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BrowseRequest {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<BrowseOptions>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BrowseOptions {
    /// CSS selector to wait for before returning content.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait_for: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

/// POST /browse response.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BrowseResponse {
    pub content: String,
    pub metadata: FetchMetadata,
}

/// Source adapter catalog entry from GET /sources.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

/// POST /sources/{id}/query request.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceQueryRequest {
    /// Source-specific query parameters.
    #[serde(flatten)]
    pub params: serde_json::Map<String, Value>,
}

/// POST /sources/{id}/query response.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceQueryResponse {
    pub results: Vec<SourceQueryResult>,
    pub metadata: SourceQueryMetadata,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceQueryResult {
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Source-specific additional fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra: Option<serde_json::Map<String, Value>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceQueryMetadata {
    pub source_id: String,
    pub total_results: usize,
    pub returned_results: usize,
}
