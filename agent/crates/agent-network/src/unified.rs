//! Unified LLM client that routes to the appropriate provider based on model name.

use agent_core::{AgentError, Message};
use async_openai::types::ChatCompletionRequestMessage;

use crate::anthropic::AnthropicClient;
use crate::client::{ChatResponse, LlmClient, ToolSchema};
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

    /// Sends a chat request with tools (OpenAI only for now).
    /// Returns either content or tool calls that need to be executed.
    pub async fn chat_with_tools(
        &self,
        system_prompt: &str,
        messages: Vec<ChatCompletionRequestMessage>,
        tools: &[ToolSchema],
    ) -> Result<ChatResponse, AgentError> {
        match self.provider {
            ProviderType::OpenAI => {
                let client = LlmClient::new(&self.model, self.api_base.as_deref());
                client.chat_with_tools(system_prompt, messages, tools).await
            }
            ProviderType::Anthropic => {
                // TODO: Implement Anthropic tool calling
                Err(AgentError::LlmError(
                    "Tool calling not yet supported for Anthropic models".to_string(),
                ))
            }
        }
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
