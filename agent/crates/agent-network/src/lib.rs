//! LLM client abstractions for OpenAI, Anthropic, and Ollama APIs.
//!
//! Provides streaming and non-streaming chat completions, model discovery,
//! and metrics collection for local Ollama models.

mod anthropic;
mod client;
mod ollama;
mod unified;

pub use anthropic::AnthropicClient;
pub use client::{ChatResponse, LlmClient, LlmMetrics, LlmResponse, LlmStream, StreamChunk, ToolCall, ToolSchema};
pub use ollama::{discover_models, unload_model, OllamaClient, OllamaMetrics, OllamaMetricsCollector};
pub use unified::UnifiedLlmClient;
