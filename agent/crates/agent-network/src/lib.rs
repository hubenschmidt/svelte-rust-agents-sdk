//! LLM client abstractions for OpenAI-compatible and native Ollama APIs.
//!
//! Provides streaming and non-streaming chat completions, model discovery,
//! and metrics collection for local Ollama models.

mod client;
mod ollama;

pub use client::{LlmClient, LlmMetrics, LlmResponse, LlmStream, StreamChunk};
pub use ollama::{discover_models, unload_model, OllamaClient, OllamaMetrics, OllamaMetricsCollector};
