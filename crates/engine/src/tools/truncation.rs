use autosint_common::config::ToolResultLimits;
use serde_json::Value;

/// Truncate search result arrays to the configured max, adding a note about omitted results.
pub fn truncate_search_results(results: &mut Value, limits: &ToolResultLimits) {
    let max = limits.max_search_results as usize;

    if let Some(arr) = results.get_mut("results").and_then(|v| v.as_array_mut()) {
        if arr.len() > max {
            let total = arr.len();
            arr.truncate(max);
            if let Some(obj) = results.as_object_mut() {
                obj.insert("total_results".into(), Value::from(total));
                obj.insert(
                    "truncated".into(),
                    Value::String(format!("[{} more results omitted]", total - max)),
                );
            }
        }
    }
}

/// Truncate entity detail responses — freeform properties before core fields.
pub fn truncate_entity_detail(entity: &mut Value, limits: &ToolResultLimits) {
    let max_chars = limits.max_entity_detail_chars as usize;

    // Truncate the properties sub-object first (least important).
    if let Some(props) = entity.get_mut("properties").and_then(|v| v.as_object_mut()) {
        let serialized_len: usize = props
            .iter()
            .map(|(k, v)| k.len() + v.to_string().len())
            .sum();

        if serialized_len > max_chars / 2 {
            let mut kept = serde_json::Map::new();
            let mut budget = max_chars / 2;
            for (k, v) in props.iter() {
                let entry_len = k.len() + v.to_string().len();
                if budget >= entry_len {
                    budget -= entry_len;
                    kept.insert(k.clone(), v.clone());
                } else {
                    break;
                }
            }
            let omitted = props.len() - kept.len();
            if omitted > 0 {
                kept.insert(
                    "_truncated".into(),
                    Value::String(format!("[{} properties omitted]", omitted)),
                );
            }
            *props = kept;
        }
    }

    // Truncate summary if still too long.
    if let Some(summary) = entity
        .get_mut("summary")
        .and_then(|v| v.as_str().map(String::from))
    {
        if summary.len() > max_chars {
            if let Some(obj) = entity.as_object_mut() {
                obj.insert(
                    "summary".into(),
                    Value::String(format!("{}...[truncated]", &summary[..max_chars])),
                );
            }
        }
    }
}

/// Truncate claim content previews before dropping results entirely.
pub fn truncate_claim_previews(claims: &mut Value, limits: &ToolResultLimits) {
    let max_preview = limits.max_claim_preview_chars as usize;

    if let Some(arr) = claims.get_mut("results").and_then(|v| v.as_array_mut()) {
        for item in arr.iter_mut() {
            if let Some(content) = item
                .get_mut("content")
                .and_then(|v| v.as_str().map(String::from))
            {
                if content.len() > max_preview {
                    if let Some(obj) = item.as_object_mut() {
                        obj.insert(
                            "content".into(),
                            Value::String(format!(
                                "{}...[truncated, {} chars total]",
                                &content[..max_preview],
                                content.len()
                            )),
                        );
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_truncate_search_results_under_limit() {
        let mut results = json!({"results": [{"id": "1"}, {"id": "2"}]});
        let limits = ToolResultLimits {
            max_search_results: 10,
            max_entity_detail_chars: 10000,
            max_claim_preview_chars: 500,
        };
        truncate_search_results(&mut results, &limits);
        assert_eq!(results["results"].as_array().unwrap().len(), 2);
        assert!(results.get("truncated").is_none());
    }

    #[test]
    fn test_truncate_search_results_over_limit() {
        let items: Vec<Value> = (0..25).map(|i| json!({"id": i.to_string()})).collect();
        let mut results = json!({"results": items});
        let limits = ToolResultLimits {
            max_search_results: 10,
            max_entity_detail_chars: 10000,
            max_claim_preview_chars: 500,
        };
        truncate_search_results(&mut results, &limits);
        assert_eq!(results["results"].as_array().unwrap().len(), 10);
        assert_eq!(results["total_results"], 25);
        assert!(results["truncated"].as_str().unwrap().contains("15 more"));
    }

    #[test]
    fn test_truncate_claim_previews() {
        let long_content = "a".repeat(1000);
        let mut claims = json!({"results": [{"content": long_content}]});
        let limits = ToolResultLimits {
            max_search_results: 20,
            max_entity_detail_chars: 10000,
            max_claim_preview_chars: 100,
        };
        truncate_claim_previews(&mut claims, &limits);
        let preview = claims["results"][0]["content"].as_str().unwrap();
        assert!(preview.contains("[truncated"));
        assert!(preview.len() < 200);
    }
}
