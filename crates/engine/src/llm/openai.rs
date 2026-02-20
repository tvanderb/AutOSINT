use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::types::{
    ContentBlock, LlmResponse, Message, Role, StopReason, TokenUsage, ToolDefinition,
};
use super::LlmError;

const OPENAI_CHAT_URL: &str = "https://api.openai.com/v1/chat/completions";

// ---------------------------------------------------------------------------
// Request wire types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<ChatTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
}

#[derive(Serialize)]
struct ChatMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<ChatToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Serialize)]
struct ChatTool {
    r#type: String,
    function: ChatFunction,
}

#[derive(Serialize)]
struct ChatFunction {
    name: String,
    description: String,
    parameters: Value,
}

#[derive(Serialize, Deserialize)]
struct ChatToolCall {
    id: String,
    r#type: String,
    function: ChatToolCallFunction,
}

#[derive(Serialize, Deserialize)]
struct ChatToolCallFunction {
    name: String,
    arguments: String,
}

// ---------------------------------------------------------------------------
// Response wire types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
    usage: ChatUsage,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatResponseMessage,
    finish_reason: String,
}

#[derive(Deserialize)]
struct ChatResponseMessage {
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<ChatToolCall>,
}

#[derive(Deserialize)]
struct ChatUsage {
    prompt_tokens: u64,
    completion_tokens: u64,
}

#[derive(Deserialize)]
struct OpenAiError {
    error: OpenAiErrorDetail,
}

#[derive(Deserialize)]
struct OpenAiErrorDetail {
    message: String,
}

// ---------------------------------------------------------------------------
// Conversion helpers
// ---------------------------------------------------------------------------

fn to_wire_messages(system: &str, messages: &[Message]) -> Vec<ChatMessage> {
    let mut wire = vec![ChatMessage {
        role: "system".into(),
        content: Some(system.to_string()),
        tool_calls: None,
        tool_call_id: None,
    }];

    for msg in messages {
        match msg.role {
            Role::User => {
                // User messages may contain text or tool results.
                for block in &msg.content {
                    match block {
                        ContentBlock::Text { text } => {
                            wire.push(ChatMessage {
                                role: "user".into(),
                                content: Some(text.clone()),
                                tool_calls: None,
                                tool_call_id: None,
                            });
                        }
                        ContentBlock::ToolResult {
                            tool_use_id,
                            content,
                            ..
                        } => {
                            wire.push(ChatMessage {
                                role: "tool".into(),
                                content: Some(content.clone()),
                                tool_calls: None,
                                tool_call_id: Some(tool_use_id.clone()),
                            });
                        }
                        _ => {}
                    }
                }
            }
            Role::Assistant => {
                // Collect text and tool calls from assistant content blocks.
                let mut text_parts = Vec::new();
                let mut tool_calls = Vec::new();

                for block in &msg.content {
                    match block {
                        ContentBlock::Text { text } => text_parts.push(text.clone()),
                        ContentBlock::ToolUse { id, name, input } => {
                            tool_calls.push(ChatToolCall {
                                id: id.clone(),
                                r#type: "function".into(),
                                function: ChatToolCallFunction {
                                    name: name.clone(),
                                    arguments: serde_json::to_string(input).unwrap_or_default(),
                                },
                            });
                        }
                        _ => {}
                    }
                }

                let content = if text_parts.is_empty() {
                    None
                } else {
                    Some(text_parts.join("\n"))
                };

                let tool_calls_opt = if tool_calls.is_empty() {
                    None
                } else {
                    Some(tool_calls)
                };

                wire.push(ChatMessage {
                    role: "assistant".into(),
                    content,
                    tool_calls: tool_calls_opt,
                    tool_call_id: None,
                });
            }
        }
    }

    wire
}

fn from_wire_response(resp: ChatResponse) -> Result<LlmResponse, LlmError> {
    let choice = resp
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| LlmError::Parse("Empty choices array".into()))?;

    let mut content = Vec::new();

    if let Some(text) = choice.message.content {
        if !text.is_empty() {
            content.push(ContentBlock::Text { text });
        }
    }

    for tc in choice.message.tool_calls {
        let input: Value = serde_json::from_str(&tc.function.arguments)
            .unwrap_or(Value::Object(serde_json::Map::new()));
        content.push(ContentBlock::ToolUse {
            id: tc.id,
            name: tc.function.name,
            input,
        });
    }

    let stop_reason = match choice.finish_reason.as_str() {
        "stop" => StopReason::EndTurn,
        "tool_calls" => StopReason::ToolUse,
        "length" => StopReason::MaxTokens,
        _ => StopReason::EndTurn,
    };

    Ok(LlmResponse {
        content,
        stop_reason,
        usage: TokenUsage {
            input_tokens: resp.usage.prompt_tokens,
            output_tokens: resp.usage.completion_tokens,
        },
    })
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
/// Send a chat completion request to the OpenAI API.
pub async fn send_chat_completion(
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

    let wire_messages = to_wire_messages(system, messages);

    let wire_tools: Vec<ChatTool> = tools
        .iter()
        .map(|t| ChatTool {
            r#type: "function".into(),
            function: ChatFunction {
                name: t.name.clone(),
                description: t.description.clone(),
                parameters: t.input_schema.clone(),
            },
        })
        .collect();

    let request = ChatRequest {
        model,
        max_tokens,
        messages: wire_messages,
        tools: wire_tools,
        temperature,
    };

    let response = http
        .post(OPENAI_CHAT_URL)
        .bearer_auth(api_key)
        .json(&request)
        .send()
        .await
        .map_err(|e| LlmError::Http(e.to_string()))?;

    let status = response.status();
    let latency = start.elapsed().as_secs_f64();
    metrics::histogram!("llm.api.latency", "provider" => "openai", "model" => model.to_string())
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
        let parsed = serde_json::from_str::<OpenAiError>(&body);
        let msg = match parsed {
            Ok(e) => {
                if e.error.message.contains("context_length_exceeded") {
                    return Err(LlmError::ContextWindowExceeded(e.error.message));
                }
                e.error.message
            }
            Err(_) => body,
        };
        return Err(LlmError::Api(format!("{}: {}", status, msg)));
    }

    let body: ChatResponse = response
        .json()
        .await
        .map_err(|e| LlmError::Parse(format!("Failed to parse OpenAI response: {}", e)))?;

    let llm_response = from_wire_response(body)?;

    metrics::counter!("llm.api.input_tokens", "provider" => "openai")
        .increment(llm_response.usage.input_tokens);
    metrics::counter!("llm.api.output_tokens", "provider" => "openai")
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
    fn test_parse_openai_text_response() {
        let json = r#"{
            "choices": [{
                "message": {"content": "Hello world", "tool_calls": []},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5}
        }"#;

        let resp: ChatResponse = serde_json::from_str(json).unwrap();
        let parsed = from_wire_response(resp).unwrap();

        assert_eq!(parsed.stop_reason, StopReason::EndTurn);
        assert_eq!(parsed.usage.input_tokens, 10);
        assert_eq!(parsed.usage.output_tokens, 5);
        assert_eq!(parsed.content.len(), 1);
    }

    #[test]
    fn test_parse_openai_tool_call_response() {
        let json = r#"{
            "choices": [{
                "message": {
                    "content": null,
                    "tool_calls": [{
                        "id": "call_abc123",
                        "type": "function",
                        "function": {
                            "name": "search_entities",
                            "arguments": "{\"query\": \"TSMC\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {"prompt_tokens": 100, "completion_tokens": 50}
        }"#;

        let resp: ChatResponse = serde_json::from_str(json).unwrap();
        let parsed = from_wire_response(resp).unwrap();

        assert_eq!(parsed.stop_reason, StopReason::ToolUse);
        assert_eq!(parsed.content.len(), 1);
        match &parsed.content[0] {
            ContentBlock::ToolUse { name, input, .. } => {
                assert_eq!(name, "search_entities");
                assert_eq!(input["query"], "TSMC");
            }
            _ => panic!("Expected tool_use block"),
        }
    }

    #[test]
    fn test_system_message_in_wire_format() {
        let messages = vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: "Hello".into(),
            }],
        }];

        let wire = to_wire_messages("You are helpful.", &messages);
        assert_eq!(wire.len(), 2);
        assert_eq!(wire[0].role, "system");
        assert_eq!(wire[0].content.as_deref(), Some("You are helpful."));
        assert_eq!(wire[1].role, "user");
    }
}
