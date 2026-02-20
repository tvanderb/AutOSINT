use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::types::{
    ContentBlock, LlmResponse, Message, Role, StopReason, TokenUsage, ToolDefinition,
};
use super::LlmError;

const ANTHROPIC_MESSAGES_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";

// ---------------------------------------------------------------------------
// Request wire types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct AnthropicRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    system: &'a str,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<AnthropicTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
}

#[derive(Serialize)]
struct AnthropicMessage {
    role: String,
    content: Vec<AnthropicContentBlock>,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicContentBlock {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
}

#[derive(Serialize)]
struct AnthropicTool {
    name: String,
    description: String,
    input_schema: Value,
}

// ---------------------------------------------------------------------------
// Response wire types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicResponseBlock>,
    stop_reason: String,
    usage: AnthropicUsage,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicResponseBlock {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
}

#[derive(Deserialize)]
struct AnthropicUsage {
    input_tokens: u64,
    output_tokens: u64,
}

#[derive(Deserialize)]
struct AnthropicError {
    error: AnthropicErrorDetail,
}

#[derive(Deserialize)]
struct AnthropicErrorDetail {
    message: String,
    #[serde(default)]
    r#type: String,
}

// ---------------------------------------------------------------------------
// Conversion helpers
// ---------------------------------------------------------------------------

fn to_wire_message(msg: &Message) -> AnthropicMessage {
    let role = match msg.role {
        Role::User => "user",
        Role::Assistant => "assistant",
    };

    let content = msg
        .content
        .iter()
        .map(|block| match block {
            ContentBlock::Text { text } => AnthropicContentBlock::Text { text: text.clone() },
            ContentBlock::ToolUse { id, name, input } => AnthropicContentBlock::ToolUse {
                id: id.clone(),
                name: name.clone(),
                input: input.clone(),
            },
            ContentBlock::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => AnthropicContentBlock::ToolResult {
                tool_use_id: tool_use_id.clone(),
                content: content.clone(),
                is_error: *is_error,
            },
        })
        .collect();

    AnthropicMessage {
        role: role.to_string(),
        content,
    }
}

fn from_wire_response(resp: AnthropicResponse) -> LlmResponse {
    let content = resp
        .content
        .into_iter()
        .map(|block| match block {
            AnthropicResponseBlock::Text { text } => ContentBlock::Text { text },
            AnthropicResponseBlock::ToolUse { id, name, input } => {
                ContentBlock::ToolUse { id, name, input }
            }
        })
        .collect();

    let stop_reason = match resp.stop_reason.as_str() {
        "end_turn" => StopReason::EndTurn,
        "tool_use" => StopReason::ToolUse,
        "max_tokens" => StopReason::MaxTokens,
        "stop_sequence" => StopReason::StopSequence,
        _ => StopReason::EndTurn,
    };

    LlmResponse {
        content,
        stop_reason,
        usage: TokenUsage {
            input_tokens: resp.usage.input_tokens,
            output_tokens: resp.usage.output_tokens,
        },
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
/// Send a messages request to the Anthropic API.
pub async fn send_messages(
    http: &reqwest::Client,
    api_key: &str,
    model: &str,
    max_tokens: u32,
    temperature: Option<f64>,
    system: &str,
    messages: &[Message],
    tools: &[ToolDefinition],
) -> Result<LlmResponse, LlmError> {
    let start = std::time::Instant::now();

    let wire_messages: Vec<AnthropicMessage> = messages.iter().map(to_wire_message).collect();

    let wire_tools: Vec<AnthropicTool> = tools
        .iter()
        .map(|t| AnthropicTool {
            name: t.name.clone(),
            description: t.description.clone(),
            input_schema: t.input_schema.clone(),
        })
        .collect();

    let request = AnthropicRequest {
        model,
        max_tokens,
        system,
        messages: wire_messages,
        tools: wire_tools,
        temperature,
    };

    let response = http
        .post(ANTHROPIC_MESSAGES_URL)
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .header("content-type", "application/json")
        .json(&request)
        .send()
        .await
        .map_err(|e| LlmError::Http(e.to_string()))?;

    let status = response.status();
    let latency = start.elapsed().as_secs_f64();
    metrics::histogram!("llm.api.latency", "provider" => "anthropic", "model" => model.to_string())
        .record(latency);

    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        let body = response.text().await.unwrap_or_default();
        return Err(LlmError::Auth(format!("{}: {}", status, body)));
    }

    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        let retry_after = response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok());
        return Err(LlmError::RateLimited { retry_after });
    }

    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        let parsed = serde_json::from_str::<AnthropicError>(&body);
        let msg = match parsed {
            Ok(e) => {
                if e.error.r#type == "invalid_request_error"
                    && e.error.message.contains("context window")
                {
                    return Err(LlmError::ContextWindowExceeded(e.error.message));
                }
                e.error.message
            }
            Err(_) => body,
        };
        return Err(LlmError::Api(format!("{}: {}", status, msg)));
    }

    let body: AnthropicResponse = response
        .json()
        .await
        .map_err(|e| LlmError::Parse(format!("Failed to parse Anthropic response: {}", e)))?;

    let llm_response = from_wire_response(body);

    metrics::counter!("llm.api.input_tokens", "provider" => "anthropic")
        .increment(llm_response.usage.input_tokens);
    metrics::counter!("llm.api.output_tokens", "provider" => "anthropic")
        .increment(llm_response.usage.output_tokens);

    Ok(llm_response)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_anthropic_text_response() {
        let json = r#"{
            "content": [{"type": "text", "text": "Hello world"}],
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 10, "output_tokens": 5}
        }"#;

        let resp: AnthropicResponse = serde_json::from_str(json).unwrap();
        let parsed = from_wire_response(resp);

        assert_eq!(parsed.stop_reason, StopReason::EndTurn);
        assert_eq!(parsed.usage.input_tokens, 10);
        assert_eq!(parsed.usage.output_tokens, 5);
        assert_eq!(parsed.content.len(), 1);
        match &parsed.content[0] {
            ContentBlock::Text { text } => assert_eq!(text, "Hello world"),
            _ => panic!("Expected text block"),
        }
    }

    #[test]
    fn test_parse_anthropic_tool_use_response() {
        let json = r#"{
            "content": [
                {"type": "text", "text": "I'll search for that."},
                {"type": "tool_use", "id": "toolu_abc123", "name": "search_entities", "input": {"query": "TSMC"}}
            ],
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 100, "output_tokens": 50}
        }"#;

        let resp: AnthropicResponse = serde_json::from_str(json).unwrap();
        let parsed = from_wire_response(resp);

        assert_eq!(parsed.stop_reason, StopReason::ToolUse);
        assert_eq!(parsed.content.len(), 2);
        match &parsed.content[1] {
            ContentBlock::ToolUse { id, name, input } => {
                assert_eq!(id, "toolu_abc123");
                assert_eq!(name, "search_entities");
                assert_eq!(input["query"], "TSMC");
            }
            _ => panic!("Expected tool_use block"),
        }
    }

    #[test]
    fn test_message_wire_roundtrip() {
        let msg = Message {
            role: Role::User,
            content: vec![ContentBlock::ToolResult {
                tool_use_id: "toolu_123".into(),
                content: r#"{"id": "abc"}"#.into(),
                is_error: Some(false),
            }],
        };

        let wire = to_wire_message(&msg);
        assert_eq!(wire.role, "user");
        assert_eq!(wire.content.len(), 1);
    }
}
