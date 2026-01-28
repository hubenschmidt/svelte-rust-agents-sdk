//! Unified LLM client that routes to the appropriate provider based on model name.

use fissio_core::{AgentError, Message, ToolCall, ToolSchema};
use async_openai::types::ChatCompletionRequestMessage;

use crate::anthropic::{AnthropicClient, AnthropicToolMessage};
use crate::client::{ChatResponse, LlmClient};
use crate::{LlmResponse, LlmStream};

/// Provider type determined from model name.
#[derive(Debug, Clone, Copy)]
enum ProviderType {
    OpenAI,
    Anthropic,
}

/// Unified client that routes requests to OpenAI or Anthropic based on model name.
pub struct UnifiedLlmClient {
    model: String,
    provider: ProviderType,
    api_base: Option<String>,
}

impl UnifiedLlmClient {
    /// Creates a new unified client, detecting provider from model name.
    pub fn new(model: &str, api_base: Option<&str>) -> Self {
        let provider = match model.starts_with("claude-") {
            true => ProviderType::Anthropic,
            false => ProviderType::OpenAI,
        };

        Self {
            model: model.to_string(),
            provider,
            api_base: api_base.map(String::from),
        }
    }

    /// Returns true if this client is configured for Anthropic.
    pub fn is_anthropic(&self) -> bool {
        matches!(self.provider, ProviderType::Anthropic)
    }

    /// Sends a non-streaming chat request and returns the complete response.
    pub async fn chat(&self, system_prompt: &str, user_input: &str) -> Result<LlmResponse, AgentError> {
        match self.provider {
            ProviderType::OpenAI => {
                let client = LlmClient::new(&self.model, self.api_base.as_deref());
                client.chat(system_prompt, user_input).await
            }
            ProviderType::Anthropic => {
                let client = AnthropicClient::new(&self.model);
                client.chat(system_prompt, user_input).await
            }
        }
    }

    /// Sends a chat request with history and returns a stream of chunks.
    pub async fn chat_stream(
        &self,
        system_prompt: &str,
        history: &[Message],
        user_input: &str,
    ) -> Result<LlmStream, AgentError> {
        match self.provider {
            ProviderType::OpenAI => {
                let client = LlmClient::new(&self.model, self.api_base.as_deref());
                client.chat_stream(system_prompt, history, user_input).await
            }
            ProviderType::Anthropic => {
                let client = AnthropicClient::new(&self.model);
                client.chat_stream(system_prompt, history, user_input).await
            }
        }
    }

    /// Sends a chat request with tools.
    /// Returns either content or tool calls that need to be executed.
    ///
    /// For multi-turn tool conversations, pass `pending_tool_calls` with the
    /// tool calls from the previous response that are being fulfilled.
    pub async fn chat_with_tools(
        &self,
        system_prompt: &str,
        messages: &[ChatCompletionRequestMessage],
        tools: &[ToolSchema],
        pending_tool_calls: Option<&[ToolCall]>,
    ) -> Result<ChatResponse, AgentError> {
        match self.provider {
            ProviderType::OpenAI => {
                let client = LlmClient::new(&self.model, self.api_base.as_deref());
                client.chat_with_tools(system_prompt, messages, tools).await
            }
            ProviderType::Anthropic => {
                let client = AnthropicClient::new(&self.model);
                let anthropic_messages = self.convert_to_anthropic_messages(messages, pending_tool_calls)?;
                client.chat_with_tools(system_prompt, anthropic_messages, tools).await
            }
        }
    }

    /// Converts OpenAI-format messages to Anthropic format.
    fn convert_to_anthropic_messages(
        &self,
        messages: &[ChatCompletionRequestMessage],
        pending_tool_calls: Option<&[ToolCall]>,
    ) -> Result<Vec<AnthropicToolMessage>, AgentError> {
        let mut result = Vec::new();
        let mut tool_results: Vec<(String, String)> = Vec::new();

        for msg in messages {
            match msg {
                ChatCompletionRequestMessage::User(user_msg) => {
                    // Flush any pending tool results first
                    if !tool_results.is_empty() {
                        // Add assistant message with tool_use blocks before tool results
                        if let Some(calls) = pending_tool_calls {
                            result.push(AnthropicToolMessage::assistant_tool_use(calls));
                        }
                        result.push(AnthropicToolMessage::tool_results(&tool_results));
                        tool_results.clear();
                    }

                    // Extract text content
                    let text = match &user_msg.content {
                        async_openai::types::ChatCompletionRequestUserMessageContent::Text(t) => t.clone(),
                        async_openai::types::ChatCompletionRequestUserMessageContent::Array(parts) => {
                            parts.iter().filter_map(|p| {
                                if let async_openai::types::ChatCompletionRequestUserMessageContentPart::Text(t) = p {
                                    Some(t.text.clone())
                                } else {
                                    None
                                }
                            }).collect::<Vec<_>>().join("\n")
                        }
                    };
                    result.push(AnthropicToolMessage::user(&text));
                }
                ChatCompletionRequestMessage::Tool(tool_msg) => {
                    // Collect tool results to batch them
                    let id = tool_msg.tool_call_id.clone();
                    let content = match &tool_msg.content {
                        async_openai::types::ChatCompletionRequestToolMessageContent::Text(t) => t.clone(),
                        async_openai::types::ChatCompletionRequestToolMessageContent::Array(parts) => {
                            parts.iter().map(|p| {
                                let async_openai::types::ChatCompletionRequestToolMessageContentPart::Text(t) = p;
                                t.text.clone()
                            }).collect::<Vec<_>>().join("\n")
                        }
                    };
                    tool_results.push((id, content));
                }
                _ => {} // Skip system and other message types
            }
        }

        // Flush any remaining tool results
        if !tool_results.is_empty() {
            if let Some(calls) = pending_tool_calls {
                result.push(AnthropicToolMessage::assistant_tool_use(calls));
            }
            result.push(AnthropicToolMessage::tool_results(&tool_results));
        }

        Ok(result)
    }

    /// Helper to create a user message for tool conversations.
    pub fn user_message(content: &str) -> Result<ChatCompletionRequestMessage, AgentError> {
        LlmClient::user_message(content)
    }

    /// Helper to create an assistant message for tool conversations.
    pub fn assistant_message(content: &str) -> Result<ChatCompletionRequestMessage, AgentError> {
        LlmClient::assistant_message(content)
    }

    /// Helper to create a tool result message.
    pub fn tool_result_message(tool_call_id: &str, content: &str) -> Result<ChatCompletionRequestMessage, AgentError> {
        LlmClient::tool_result_message(tool_call_id, content)
    }
}
