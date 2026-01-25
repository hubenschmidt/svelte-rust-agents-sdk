//! OpenAI-compatible chat client with streaming support.
//!
//! Works with OpenAI API and any compatible endpoint (including Ollama's /v1 endpoint).
//! Supports regular chat, streaming, and structured JSON output.

use std::pin::Pin;
use std::time::Instant;

use agent_core::{AgentError, Message, MessageRole};
use async_openai::{
    config::OpenAIConfig,
    types::{
        ChatCompletionRequestAssistantMessageArgs, ChatCompletionRequestMessage,
        ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestUserMessageArgs,
        ChatCompletionStreamOptions, CreateChatCompletionRequestArgs,
        CreateChatCompletionResponse, ResponseFormat,
    },
    Client,
};
use futures::Stream;
use serde::de::DeserializeOwned;
use tracing::{debug, info};

/// A chunk from a streaming LLM response.
pub enum StreamChunk {
    Content(String),
    Usage { input_tokens: u32, output_tokens: u32 },
}

/// A stream of LLM response chunks.
pub type LlmStream = Pin<Box<dyn Stream<Item = Result<StreamChunk, AgentError>> + Send>>;

/// Token usage and timing metrics from an LLM call.
#[derive(Debug, Clone, Default)]
pub struct LlmMetrics {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub elapsed_ms: u64,
}

/// Complete response from an LLM call.
#[derive(Debug, Clone)]
pub struct LlmResponse {
    pub content: String,
    pub metrics: LlmMetrics,
}

/// Converts any error into an AgentError::LlmError.
fn llm_err(e: impl ToString) -> AgentError {
    AgentError::LlmError(e.to_string())
}

/// Builds the message list for a simple system + user request.
fn build_messages(
    system_prompt: &str,
    user_input: &str,
) -> Result<Vec<ChatCompletionRequestMessage>, AgentError> {
    Ok(vec![
        ChatCompletionRequestMessage::System(
            ChatCompletionRequestSystemMessageArgs::default()
                .content(system_prompt)
                .build()
                .map_err(llm_err)?,
        ),
        ChatCompletionRequestMessage::User(
            ChatCompletionRequestUserMessageArgs::default()
                .content(user_input)
                .build()
                .map_err(llm_err)?,
        ),
    ])
}

/// Extracts content and metrics from a completion response.
fn extract_response(response: CreateChatCompletionResponse, elapsed_ms: u64) -> Result<LlmResponse, AgentError> {
    let content = response
        .choices
        .into_iter()
        .next()
        .and_then(|c| c.message.content)
        .ok_or_else(|| AgentError::LlmError("No response content".into()))?;

    let (input_tokens, output_tokens) = response
        .usage
        .map(|u| (u.prompt_tokens, u.completion_tokens))
        .unwrap_or((0, 0));

    info!(
        "LLM: {}ms, tokens: {}/{} (in/out)",
        elapsed_ms, input_tokens, output_tokens
    );

    Ok(LlmResponse {
        content,
        metrics: LlmMetrics { input_tokens, output_tokens, elapsed_ms },
    })
}

/// Client for OpenAI-compatible chat completion APIs.
pub struct LlmClient {
    client: Client<OpenAIConfig>,
    default_model: String,
}

impl LlmClient {
    /// Creates a new client for the given model and optional API base URL.
    pub fn new(model: &str, api_base: Option<&str>) -> Self {
        let config = match api_base {
            Some(base) => OpenAIConfig::new()
                .with_api_base(base)
                .with_api_key("ollama"),
            None => OpenAIConfig::default(),
        };

        Self {
            client: Client::with_config(config),
            default_model: model.to_string(),
        }
    }

    /// Sends a chat request and returns the complete response.
    pub async fn chat(&self, system_prompt: &str, user_input: &str) -> Result<LlmResponse, AgentError> {
        let start = Instant::now();
        let messages = build_messages(system_prompt, user_input)?;

        let request = CreateChatCompletionRequestArgs::default()
            .model(&self.default_model)
            .messages(messages)
            .build()
            .map_err(llm_err)?;

        let response = self.client.chat().create(request).await.map_err(llm_err)?;
        extract_response(response, start.elapsed().as_millis() as u64)
    }

    /// Sends a chat request with history and returns a stream of chunks.
    pub async fn chat_stream(
        &self,
        system_prompt: &str,
        history: &[Message],
        user_input: &str,
    ) -> Result<LlmStream, AgentError> {
        use futures::StreamExt;

        let mut messages = vec![
            ChatCompletionRequestMessage::System(
                ChatCompletionRequestSystemMessageArgs::default()
                    .content(system_prompt)
                    .build()
                    .map_err(llm_err)?,
            ),
        ];

        for msg in history {
            let role_msg = match msg.role {
                MessageRole::User => ChatCompletionRequestMessage::User(
                    ChatCompletionRequestUserMessageArgs::default()
                        .content(&*msg.content)
                        .build()
                        .map_err(llm_err)?,
                ),
                MessageRole::Assistant => ChatCompletionRequestMessage::Assistant(
                    ChatCompletionRequestAssistantMessageArgs::default()
                        .content(&*msg.content)
                        .build()
                        .map_err(llm_err)?,
                ),
            };
            messages.push(role_msg);
        }

        messages.push(ChatCompletionRequestMessage::User(
            ChatCompletionRequestUserMessageArgs::default()
                .content(user_input)
                .build()
                .map_err(llm_err)?,
        ));

        let request = CreateChatCompletionRequestArgs::default()
            .model(&self.default_model)
            .stream_options(ChatCompletionStreamOptions { include_usage: true })
            .messages(messages)
            .build()
            .map_err(llm_err)?;

        let stream = self.client.chat().create_stream(request).await.map_err(llm_err)?;

        let mapped = stream.filter_map(|result| async move {
            match result {
                Ok(response) => {
                    if let Some(usage) = response.usage {
                        return Some(Ok(StreamChunk::Usage {
                            input_tokens: usage.prompt_tokens,
                            output_tokens: usage.completion_tokens,
                        }));
                    }
                    let chunk = response.choices.first()?.delta.content.clone()?;
                    Some(Ok(StreamChunk::Content(chunk)))
                }
                Err(e) => Some(Err(AgentError::LlmError(e.to_string()))),
            }
        });

        Ok(Box::pin(mapped))
    }

    /// Sends a chat request expecting a JSON response, parses into the given type.
    pub async fn structured<T: DeserializeOwned>(
        &self,
        system_prompt: &str,
        user_input: &str,
    ) -> Result<(T, LlmMetrics), AgentError> {
        let start = Instant::now();
        let messages = build_messages(system_prompt, user_input)?;

        let request = CreateChatCompletionRequestArgs::default()
            .model(&self.default_model)
            .response_format(ResponseFormat::JsonObject)
            .messages(messages)
            .build()
            .map_err(llm_err)?;

        let response = self.client.chat().create(request).await.map_err(llm_err)?;
        let llm_response = extract_response(response, start.elapsed().as_millis() as u64)?;

        debug!("Structured response: {}", llm_response.content);

        let parsed = serde_json::from_str(&llm_response.content).map_err(|e| {
            AgentError::ParseError(format!("Failed to parse: {} - content: {}", e, llm_response.content))
        })?;

        Ok((parsed, llm_response.metrics))
    }
}
