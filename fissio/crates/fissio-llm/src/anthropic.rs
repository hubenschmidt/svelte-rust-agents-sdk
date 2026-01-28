//! Anthropic Claude API client with streaming and tool support.

use fissio_core::{AgentError, Message, MessageRole, ToolCall, ToolSchema};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{error, info};

use crate::client::ChatResponse;
use crate::{LlmMetrics, LlmResponse, LlmStream, StreamChunk};

const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";

#[derive(Serialize)]
struct AnthropicMessage {
    role: &'static str,
    content: String,
}

#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<AnthropicMessage>,
    stream: bool,
}

#[derive(Deserialize)]
struct ContentBlockDelta {
    text: Option<String>,
}

#[derive(Deserialize)]
struct Usage {
    input_tokens: Option<u32>,
    output_tokens: Option<u32>,
}

#[derive(Deserialize)]
struct StreamEvent {
    #[serde(rename = "type")]
    event_type: String,
    delta: Option<ContentBlockDelta>,
    usage: Option<Usage>,
    message: Option<MessageEvent>,
}

#[derive(Deserialize)]
struct MessageEvent {
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct ContentBlock {
    text: String,
}

#[derive(Deserialize)]
struct NonStreamResponse {
    content: Vec<ContentBlock>,
    usage: Usage,
}

// === Tool calling support ===

/// Tool definition for Anthropic API.
#[derive(Serialize)]
struct AnthropicTool {
    name: String,
    description: String,
    input_schema: serde_json::Value,
}

/// Request body with tools.
#[derive(Serialize)]
struct AnthropicRequestWithTools {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<AnthropicMessageWithContent>,
    tools: Vec<AnthropicTool>,
}

/// Message with content blocks (for tool conversations).
#[derive(Serialize, Clone)]
pub struct AnthropicMessageWithContent {
    role: String,
    content: Vec<MessageContentBlock>,
}

/// Content block in a message - can be text, tool_use, or tool_result.
#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
enum MessageContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

/// Response that may contain tool_use blocks.
#[derive(Deserialize)]
struct ToolResponse {
    content: Vec<ToolResponseBlock>,
    usage: Usage,
    #[allow(dead_code)]
    stop_reason: Option<String>,
}

/// A content block in the response.
#[derive(Deserialize)]
#[serde(tag = "type")]
enum ToolResponseBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
}

/// Client for Anthropic's Claude API.
pub struct AnthropicClient {
    client: Client,
    model: String,
    api_key: String,
}

impl AnthropicClient {
    /// Creates a new Anthropic client.
    pub fn new(model: &str) -> Self {
        let api_key = std::env::var("ANTHROPIC_API_KEY").unwrap_or_default();
        tracing::info!(
            "AnthropicClient: model={}, api_key_len={}",
            model,
            api_key.len()
        );
        Self {
            client: Client::new(),
            model: model.to_string(),
            api_key,
        }
    }

    /// Sends a non-streaming chat request and returns the complete response.
    pub async fn chat(&self, system_prompt: &str, user_input: &str) -> Result<LlmResponse, AgentError> {
        let start = std::time::Instant::now();

        let request = AnthropicRequest {
            model: self.model.clone(),
            max_tokens: 8192,
            system: system_prompt.to_string(),
            messages: vec![AnthropicMessage {
                role: "user",
                content: user_input.to_string(),
            }],
            stream: false,
        };

        let response = self
            .client
            .post(ANTHROPIC_API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| AgentError::LlmError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AgentError::LlmError(format!(
                "Anthropic API error {}: {}",
                status, body
            )));
        }

        let resp: NonStreamResponse = response
            .json()
            .await
            .map_err(|e| AgentError::LlmError(e.to_string()))?;

        let content = resp.content.into_iter().map(|c| c.text).collect::<Vec<_>>().join("");

        Ok(LlmResponse {
            content,
            metrics: LlmMetrics {
                input_tokens: resp.usage.input_tokens.unwrap_or(0),
                output_tokens: resp.usage.output_tokens.unwrap_or(0),
                elapsed_ms: start.elapsed().as_millis() as u64,
            },
        })
    }

    /// Sends a chat request with history and returns a stream of chunks.
    pub async fn chat_stream(
        &self,
        system_prompt: &str,
        history: &[Message],
        user_input: &str,
    ) -> Result<LlmStream, AgentError> {
        use futures::StreamExt;

        let mut messages: Vec<AnthropicMessage> = history
            .iter()
            .map(|msg| AnthropicMessage {
                role: match msg.role {
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                },
                content: msg.content.clone(),
            })
            .collect();

        messages.push(AnthropicMessage {
            role: "user",
            content: user_input.to_string(),
        });

        let request = AnthropicRequest {
            model: self.model.clone(),
            max_tokens: 8192,
            system: system_prompt.to_string(),
            messages,
            stream: true,
        };

        let response = self
            .client
            .post(ANTHROPIC_API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| AgentError::LlmError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AgentError::LlmError(format!(
                "Anthropic API error {}: {}",
                status, body
            )));
        }

        let byte_stream = response.bytes_stream();

        // Use scan to maintain a buffer across chunks for incomplete SSE lines
        let mapped = byte_stream
            .scan(String::new(), |buffer, result| {
                let chunks: Vec<Result<StreamChunk, AgentError>> = match result {
                    Err(e) => vec![Err(AgentError::LlmError(e.to_string()))],
                    Ok(bytes) => {
                        let text = match String::from_utf8(bytes.to_vec()) {
                            Ok(t) => t,
                            Err(_) => return futures::future::ready(Some(vec![])),
                        };

                        buffer.push_str(&text);

                        let mut parsed_chunks = Vec::new();

                        // Process complete lines, keep incomplete line in buffer
                        while let Some(newline_pos) = buffer.find('\n') {
                            let line = buffer[..newline_pos].trim().to_string();
                            *buffer = buffer[newline_pos + 1..].to_string();

                            if !line.starts_with("data: ") {
                                continue;
                            }
                            let json = &line[6..];
                            if json == "[DONE]" {
                                continue;
                            }

                            let event: StreamEvent = match serde_json::from_str(json) {
                                Ok(e) => e,
                                Err(e) => {
                                    error!("Failed to parse Anthropic event: {} - {}", e, json);
                                    continue;
                                }
                            };

                            match event.event_type.as_str() {
                                "content_block_delta" => {
                                    if let Some(delta) = event.delta {
                                        if let Some(text) = delta.text {
                                            parsed_chunks.push(Ok(StreamChunk::Content(text)));
                                        }
                                    }
                                }
                                "message_delta" => {
                                    if let Some(usage) = event.usage {
                                        parsed_chunks.push(Ok(StreamChunk::Usage {
                                            input_tokens: usage.input_tokens.unwrap_or(0),
                                            output_tokens: usage.output_tokens.unwrap_or(0),
                                        }));
                                    }
                                }
                                "message_start" => {
                                    if let Some(msg) = event.message {
                                        if let Some(usage) = msg.usage {
                                            parsed_chunks.push(Ok(StreamChunk::Usage {
                                                input_tokens: usage.input_tokens.unwrap_or(0),
                                                output_tokens: 0,
                                            }));
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        parsed_chunks
                    }
                };
                futures::future::ready(Some(chunks))
            })
            .flat_map(futures::stream::iter);

        Ok(Box::pin(mapped))
    }

    /// Sends a chat request with tools and returns either content or tool calls.
    pub async fn chat_with_tools(
        &self,
        system_prompt: &str,
        messages: Vec<AnthropicMessageWithContent>,
        tools: &[ToolSchema],
    ) -> Result<ChatResponse, AgentError> {
        let start = std::time::Instant::now();

        let anthropic_tools: Vec<AnthropicTool> = tools
            .iter()
            .map(|t| AnthropicTool {
                name: t.name.clone(),
                description: t.description.clone(),
                input_schema: t.parameters.clone(),
            })
            .collect();

        let request = AnthropicRequestWithTools {
            model: self.model.clone(),
            max_tokens: 8192,
            system: system_prompt.to_string(),
            messages,
            tools: anthropic_tools,
        };

        let response = self
            .client
            .post(ANTHROPIC_API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| AgentError::LlmError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AgentError::LlmError(format!(
                "Anthropic API error {}: {}",
                status, body
            )));
        }

        let resp: ToolResponse = response
            .json()
            .await
            .map_err(|e| AgentError::LlmError(e.to_string()))?;

        let elapsed_ms = start.elapsed().as_millis() as u64;
        let metrics = LlmMetrics {
            input_tokens: resp.usage.input_tokens.unwrap_or(0),
            output_tokens: resp.usage.output_tokens.unwrap_or(0),
            elapsed_ms,
        };

        // Check if response contains tool_use blocks
        let tool_calls: Vec<ToolCall> = resp
            .content
            .iter()
            .filter_map(|block| match block {
                ToolResponseBlock::ToolUse { id, name, input } => Some(ToolCall {
                    id: id.clone(),
                    name: name.clone(),
                    arguments: input.clone(),
                }),
                _ => None,
            })
            .collect();

        if !tool_calls.is_empty() {
            info!(
                "Anthropic: {}ms, tokens: {}/{}, tool_calls: {}",
                elapsed_ms,
                metrics.input_tokens,
                metrics.output_tokens,
                tool_calls.len()
            );
            return Ok(ChatResponse::ToolCalls {
                calls: tool_calls,
                metrics,
            });
        }

        // No tool calls - extract text content
        let content: String = resp
            .content
            .iter()
            .filter_map(|block| match block {
                ToolResponseBlock::Text { text } => Some(text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("");

        info!(
            "Anthropic: {}ms, tokens: {}/{}, content: {} chars",
            elapsed_ms,
            metrics.input_tokens,
            metrics.output_tokens,
            content.len()
        );

        Ok(ChatResponse::Content(LlmResponse { content, metrics }))
    }
}

// === Public helper functions for tool conversations ===

impl AnthropicMessageWithContent {
    /// Creates a user message with text content.
    pub fn user(text: &str) -> Self {
        Self {
            role: "user".to_string(),
            content: vec![MessageContentBlock::Text {
                text: text.to_string(),
            }],
        }
    }

    /// Creates an assistant message with tool_use blocks.
    pub fn assistant_tool_use(tool_calls: &[ToolCall]) -> Self {
        Self {
            role: "assistant".to_string(),
            content: tool_calls
                .iter()
                .map(|tc| MessageContentBlock::ToolUse {
                    id: tc.id.clone(),
                    name: tc.name.clone(),
                    input: tc.arguments.clone(),
                })
                .collect(),
        }
    }

    /// Creates a user message with tool_result blocks.
    pub fn tool_results(results: &[(String, String)]) -> Self {
        Self {
            role: "user".to_string(),
            content: results
                .iter()
                .map(|(id, content)| MessageContentBlock::ToolResult {
                    tool_use_id: id.clone(),
                    content: content.clone(),
                })
                .collect(),
        }
    }
}

/// Re-export for use in unified client.
pub use AnthropicMessageWithContent as AnthropicToolMessage;
