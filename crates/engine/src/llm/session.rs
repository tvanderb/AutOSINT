use std::future::Future;
use std::pin::Pin;

use super::types::{ContentBlock, Message, Role, ToolDefinition};
use super::LlmCaller;

/// Result of a completed agentic session.
pub enum SessionResult {
    /// LLM responded with text only (no tool calls) — session complete.
    Completed {
        final_text: String,
        stats: SessionStats,
    },
    /// Hit the configured max turns limit.
    MaxTurnsReached { stats: SessionStats },
    /// Too many consecutive malformed tool calls.
    MalformedToolCallLimit { stats: SessionStats },
    /// An unrecoverable error occurred.
    Failed { error: String, stats: SessionStats },
}

impl SessionResult {
    pub fn stats(&self) -> &SessionStats {
        match self {
            Self::Completed { stats, .. }
            | Self::MaxTurnsReached { stats }
            | Self::MalformedToolCallLimit { stats }
            | Self::Failed { stats, .. } => stats,
        }
    }
}

/// Accumulated statistics for a session.
#[derive(Clone, Debug, Default)]
pub struct SessionStats {
    pub turns: u32,
    pub tool_calls: u32,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub malformed_tool_calls: u32,
}

/// Configuration for the agentic loop.
pub struct SessionConfig {
    pub max_turns: u32,
    pub max_consecutive_malformed: u32,
}

/// Result from executing a single tool call.
pub struct ToolExecutionResult {
    pub content: String,
    pub is_error: bool,
    /// Only malformed calls count toward the consecutive limit.
    pub is_malformed: bool,
}

/// Closure type for the tool executor passed to `run_session`.
pub type ToolExecutor = Box<
    dyn Fn(String, serde_json::Value) -> Pin<Box<dyn Future<Output = ToolExecutionResult> + Send>>
        + Send
        + Sync,
>;

/// Run the generic agentic loop.
///
/// Same loop drives both Processor (M3) and Analyst (M4).
/// The difference is which tools are registered and which system prompt is used.
pub async fn run_session(
    llm: &dyn LlmCaller,
    system_prompt: &str,
    initial_user_message: &str,
    tools: &[ToolDefinition],
    tool_executor: &ToolExecutor,
    config: &SessionConfig,
) -> SessionResult {
    let mut history = vec![Message {
        role: Role::User,
        content: vec![ContentBlock::Text {
            text: initial_user_message.to_string(),
        }],
    }];

    let mut stats = SessionStats::default();
    let mut consecutive_malformed: u32 = 0;

    loop {
        // Check turn limit.
        if stats.turns >= config.max_turns {
            tracing::warn!(turns = stats.turns, "Session hit max turns limit");
            return SessionResult::MaxTurnsReached { stats };
        }

        stats.turns += 1;

        // Call LLM.
        let response = match llm.chat(system_prompt, &history, tools).await {
            Ok(r) => r,
            Err(e) => {
                tracing::error!(error = %e, "LLM API error during session");
                return SessionResult::Failed {
                    error: e.to_string(),
                    stats,
                };
            }
        };

        // Accumulate token usage.
        stats.total_input_tokens += response.usage.input_tokens;
        stats.total_output_tokens += response.usage.output_tokens;

        // Add assistant response to history.
        history.push(Message {
            role: Role::Assistant,
            content: response.content.clone(),
        });

        // Extract tool use blocks.
        let tool_uses: Vec<_> = response
            .content
            .iter()
            .filter_map(|block| match block {
                ContentBlock::ToolUse { id, name, input } => {
                    Some((id.clone(), name.clone(), input.clone()))
                }
                _ => None,
            })
            .collect();

        // No tool calls → session complete.
        if tool_uses.is_empty() {
            let final_text = response
                .content
                .iter()
                .filter_map(|block| match block {
                    ContentBlock::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");

            return SessionResult::Completed { final_text, stats };
        }

        // Execute each tool call and collect results.
        let mut tool_results = Vec::new();

        for (id, name, input) in tool_uses {
            stats.tool_calls += 1;
            let result = tool_executor(name, input).await;

            if result.is_malformed {
                consecutive_malformed += 1;
                stats.malformed_tool_calls += 1;
            } else {
                consecutive_malformed = 0;
            }

            tool_results.push(ContentBlock::ToolResult {
                tool_use_id: id,
                content: result.content,
                is_error: if result.is_error { Some(true) } else { None },
            });
        }

        // Check consecutive malformed limit.
        if consecutive_malformed >= config.max_consecutive_malformed {
            tracing::warn!(
                consecutive = consecutive_malformed,
                "Session hit malformed tool call limit"
            );
            return SessionResult::MalformedToolCallLimit { stats };
        }

        // Add tool results as a user message.
        history.push(Message {
            role: Role::User,
            content: tool_results,
        });
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{LlmError, LlmResponse, StopReason, TokenUsage};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    /// Mock LLM that returns pre-configured responses in sequence.
    struct MockLlm {
        responses: std::sync::Mutex<Vec<Result<LlmResponse, LlmError>>>,
    }

    impl MockLlm {
        fn new(responses: Vec<Result<LlmResponse, LlmError>>) -> Self {
            // Reverse so we can pop from the back.
            let mut responses = responses;
            responses.reverse();
            Self {
                responses: std::sync::Mutex::new(responses),
            }
        }
    }

    impl LlmCaller for MockLlm {
        fn chat<'a>(
            &'a self,
            _system: &'a str,
            _messages: &'a [Message],
            _tools: &'a [ToolDefinition],
        ) -> Pin<Box<dyn Future<Output = Result<LlmResponse, LlmError>> + Send + 'a>> {
            let result = self.responses.lock().unwrap().pop().unwrap_or_else(|| {
                Ok(LlmResponse {
                    content: vec![ContentBlock::Text {
                        text: "No more responses".into(),
                    }],
                    stop_reason: StopReason::EndTurn,
                    usage: TokenUsage::default(),
                })
            });
            Box::pin(async move { result })
        }
    }

    fn noop_executor() -> ToolExecutor {
        Box::new(|_name, _input| {
            Box::pin(async {
                ToolExecutionResult {
                    content: "ok".into(),
                    is_error: false,
                    is_malformed: false,
                }
            })
        })
    }

    #[tokio::test]
    async fn test_simple_text_response() {
        let llm = MockLlm::new(vec![Ok(LlmResponse {
            content: vec![ContentBlock::Text {
                text: "Done.".into(),
            }],
            stop_reason: StopReason::EndTurn,
            usage: TokenUsage {
                input_tokens: 10,
                output_tokens: 5,
            },
        })]);

        let config = SessionConfig {
            max_turns: 10,
            max_consecutive_malformed: 3,
        };

        let result = run_session(&llm, "system", "hello", &[], &noop_executor(), &config).await;

        match result {
            SessionResult::Completed { final_text, stats } => {
                assert_eq!(final_text, "Done.");
                assert_eq!(stats.turns, 1);
                assert_eq!(stats.tool_calls, 0);
                assert_eq!(stats.total_input_tokens, 10);
                assert_eq!(stats.total_output_tokens, 5);
            }
            _ => panic!("Expected Completed"),
        }
    }

    #[tokio::test]
    async fn test_tool_call_roundtrip() {
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = Arc::clone(&call_count);

        let llm = MockLlm::new(vec![
            // Turn 1: tool call
            Ok(LlmResponse {
                content: vec![ContentBlock::ToolUse {
                    id: "toolu_1".into(),
                    name: "search".into(),
                    input: serde_json::json!({"q": "test"}),
                }],
                stop_reason: StopReason::ToolUse,
                usage: TokenUsage {
                    input_tokens: 50,
                    output_tokens: 30,
                },
            }),
            // Turn 2: final text
            Ok(LlmResponse {
                content: vec![ContentBlock::Text {
                    text: "Found results.".into(),
                }],
                stop_reason: StopReason::EndTurn,
                usage: TokenUsage {
                    input_tokens: 80,
                    output_tokens: 20,
                },
            }),
        ]);

        let executor: ToolExecutor = Box::new(move |_name, _input| {
            let counter = Arc::clone(&call_count_clone);
            Box::pin(async move {
                counter.fetch_add(1, Ordering::SeqCst);
                ToolExecutionResult {
                    content: r#"{"results": ["item1"]}"#.into(),
                    is_error: false,
                    is_malformed: false,
                }
            })
        });

        let config = SessionConfig {
            max_turns: 10,
            max_consecutive_malformed: 3,
        };

        let result = run_session(&llm, "system", "search for test", &[], &executor, &config).await;

        match result {
            SessionResult::Completed { final_text, stats } => {
                assert_eq!(final_text, "Found results.");
                assert_eq!(stats.turns, 2);
                assert_eq!(stats.tool_calls, 1);
                assert_eq!(stats.total_input_tokens, 130);
                assert_eq!(stats.total_output_tokens, 50);
            }
            _ => panic!("Expected Completed"),
        }

        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_max_turns_enforcement() {
        // LLM always returns a tool call — should hit max turns.
        let responses: Vec<_> = (0..5)
            .map(|i| {
                Ok(LlmResponse {
                    content: vec![ContentBlock::ToolUse {
                        id: format!("toolu_{}", i),
                        name: "search".into(),
                        input: serde_json::json!({}),
                    }],
                    stop_reason: StopReason::ToolUse,
                    usage: TokenUsage::default(),
                })
            })
            .collect();

        let llm = MockLlm::new(responses);

        let config = SessionConfig {
            max_turns: 3,
            max_consecutive_malformed: 3,
        };

        let result = run_session(&llm, "system", "go", &[], &noop_executor(), &config).await;

        match result {
            SessionResult::MaxTurnsReached { stats } => {
                assert_eq!(stats.turns, 3);
            }
            _ => panic!("Expected MaxTurnsReached"),
        }
    }

    #[tokio::test]
    async fn test_malformed_tool_call_limit() {
        let responses: Vec<_> = (0..5)
            .map(|i| {
                Ok(LlmResponse {
                    content: vec![ContentBlock::ToolUse {
                        id: format!("toolu_{}", i),
                        name: "unknown_tool".into(),
                        input: serde_json::json!({}),
                    }],
                    stop_reason: StopReason::ToolUse,
                    usage: TokenUsage::default(),
                })
            })
            .collect();

        let llm = MockLlm::new(responses);

        // Executor marks everything as malformed.
        let executor: ToolExecutor = Box::new(|_name, _input| {
            Box::pin(async {
                ToolExecutionResult {
                    content: "Unknown tool".into(),
                    is_error: true,
                    is_malformed: true,
                }
            })
        });

        let config = SessionConfig {
            max_turns: 10,
            max_consecutive_malformed: 2,
        };

        let result = run_session(&llm, "system", "go", &[], &executor, &config).await;

        match result {
            SessionResult::MalformedToolCallLimit { stats } => {
                assert_eq!(stats.malformed_tool_calls, 2);
            }
            _ => panic!("Expected MalformedToolCallLimit"),
        }
    }

    #[tokio::test]
    async fn test_llm_error_propagation() {
        let llm = MockLlm::new(vec![Err(LlmError::Auth("invalid key".into()))]);

        let config = SessionConfig {
            max_turns: 10,
            max_consecutive_malformed: 3,
        };

        let result = run_session(&llm, "system", "go", &[], &noop_executor(), &config).await;

        match result {
            SessionResult::Failed { error, .. } => {
                assert!(error.contains("invalid key"));
            }
            _ => panic!("Expected Failed"),
        }
    }
}
