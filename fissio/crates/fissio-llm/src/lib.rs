//! LLM client abstractions for OpenAI, Anthropic, and Ollama APIs.
//!
//! This crate provides unified access to multiple LLM providers:
//!
//! - [`UnifiedLlmClient`] — Recommended: auto-routes to correct provider
//! - [`LlmClient`] — OpenAI-compatible client (also works with Ollama)
//! - [`AnthropicClient`] — Claude models via Anthropic API
//!
//! # Quick Start
//!
//! ```rust,ignore
//! use fissio_llm::UnifiedLlmClient;
//!
//! // Automatically uses OpenAI or Anthropic based on model name
//! let client = UnifiedLlmClient::new("gpt-4", None);
//! let response = client.chat("You are helpful.", "Hello!").await?;
//!
//! // Claude models auto-route to Anthropic
//! let client = UnifiedLlmClient::new("claude-3-opus-20240229", None);
//! ```
//!
//! # Streaming
//!
//! ```rust,ignore
//! use fissio_llm::{UnifiedLlmClient, StreamChunk};
//! use futures::StreamExt;
//!
//! let client = UnifiedLlmClient::new("gpt-4", None);
//! let mut stream = client.chat_stream("Be helpful.", &[], "Hi").await?;
//!
//! while let Some(chunk) = stream.next().await {
//!     match chunk? {
//!         StreamChunk::Content(text) => print!("{}", text),
//!         StreamChunk::Usage { input_tokens, output_tokens } => {
//!             println!("\nTokens: {}/{}", input_tokens, output_tokens);
//!         }
//!     }
//! }
//! ```
//!
//! # Tool Calling
//!
//! ```rust,ignore
//! use fissio_llm::{UnifiedLlmClient, ChatResponse, ToolSchema};
//!
//! let tools = vec![ToolSchema {
//!     name: "get_weather".to_string(),
//!     description: "Get current weather".to_string(),
//!     parameters: serde_json::json!({
//!         "type": "object",
//!         "properties": { "city": { "type": "string" } }
//!     }),
//! }];
//!
//! let response = client.chat_with_tools(system, messages, &tools, None).await?;
//! match response {
//!     ChatResponse::Content(resp) => println!("{}", resp.content),
//!     ChatResponse::ToolCalls { calls, .. } => {
//!         for call in calls {
//!             println!("Call {}: {}({:?})", call.id, call.name, call.arguments);
//!         }
//!     }
//! }
//! ```

mod anthropic;
mod client;
mod ollama;
mod unified;

pub use anthropic::AnthropicClient;
pub use client::{ChatResponse, LlmClient, LlmMetrics, LlmResponse, LlmStream, StreamChunk};
pub use fissio_core::{ToolCall, ToolResult, ToolSchema};
pub use ollama::{discover_models, unload_model, OllamaClient, OllamaMetrics, OllamaMetricsCollector};
pub use unified::UnifiedLlmClient;
