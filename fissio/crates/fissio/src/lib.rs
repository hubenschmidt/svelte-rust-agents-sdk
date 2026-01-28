//! # Fissio — Pipeline-first agent framework
//!
//! Fissio is a Rust framework for building LLM-powered agent systems using
//! **declarative pipeline definitions** as the primary abstraction.
//!
//! ## Quick Start — Load from JSON
//!
//! ```rust,ignore
//! use fissio::prelude::*;
//! use std::collections::HashMap;
//!
//! // Load pipeline from JSON file
//! let config = PipelineConfig::from_file("pipeline.json")?;
//!
//! // Create engine and execute
//! let engine = PipelineEngine::new(config, models, default_model, HashMap::new());
//! let result = engine.execute_stream("Hello!", &[]).await?;
//! ```
//!
//! ## Quick Start — Builder API
//!
//! ```rust,ignore
//! use fissio::prelude::*;
//!
//! let config = PipelineConfig::builder("research", "Research Pipeline")
//!     .description("Searches and summarizes information")
//!     .node("researcher", NodeType::Worker)
//!         .prompt("You are a research assistant.")
//!         .tools(["web_search", "fetch_url"])
//!         .done()
//!     .node("summarizer", NodeType::Llm)
//!         .prompt("Summarize the research findings concisely.")
//!         .done()
//!     .edge("input", "researcher")
//!     .edge("researcher", "summarizer")
//!     .edge("summarizer", "output")
//!     .build();
//! ```
//!
//! ## Crate Structure
//!
//! | Crate | Description |
//! |-------|-------------|
//! | [`fissio_config`] | Pipeline schema, node/edge types |
//! | [`fissio_core`] | Error types, messages, model config |
//! | [`fissio_engine`] | DAG execution engine |
//! | [`fissio_llm`] | LLM providers (OpenAI, Anthropic, Ollama) |
//! | [`fissio_tools`] | Tool registry and built-in tools |
//!
//! ## Node Types
//!
//! - `Llm` — Simple LLM call with prompt
//! - `Worker` — LLM with tools (agentic loop)
//! - `Router` — Classifies input, routes to targets
//! - `Gate` — Validates before proceeding
//! - `Aggregator` — Combines multiple inputs
//! - `Orchestrator` — Dynamic task decomposition
//! - `Evaluator` — Quality scoring
//!
//! ## Edge Types
//!
//! - `Direct` — Sequential execution
//! - `Parallel` — Concurrent execution
//! - `Conditional` — Router chooses path
//! - `Dynamic` — Orchestrator picks targets

// Re-export config types
pub use fissio_config::{
    ConfigError, EdgeConfig, EdgeEndpoint, EdgeType, NodeConfig, NodeType, PipelineConfig,
    PresetRegistry,
};

// Re-export builders
pub use fissio_config::{NodeBuilder, PipelineBuilder};

// Re-export core types
pub use fissio_core::{AgentError, Message, MessageRole, ModelConfig};

// Re-export engine
pub use fissio_engine::{EngineOutput, ModelResolver, NodeInput, NodeOutput, PipelineEngine};

// Re-export LLM clients
pub use fissio_llm::{
    ChatResponse, LlmClient, LlmMetrics, LlmResponse, LlmStream, StreamChunk, ToolCall, ToolSchema,
    UnifiedLlmClient,
};

// Re-export tools
pub use fissio_tools::{FetchUrlTool, Tool, ToolError, ToolRegistry, WebSearchTool};

// Re-export editor (optional feature)
#[cfg(feature = "editor")]
pub use fissio_editor as editor;

// Provider-specific clients (hidden by default, use UnifiedLlmClient instead)
#[doc(hidden)]
pub use fissio_llm::{
    discover_models, unload_model, AnthropicClient, OllamaClient, OllamaMetrics,
    OllamaMetricsCollector,
};

// Legacy worker types (hidden)
#[doc(hidden)]
pub use fissio_core::Worker;

/// Prelude module for convenient imports.
///
/// Import everything you need with a single use statement:
///
/// ```rust,ignore
/// use fissio::prelude::*;
/// ```
pub mod prelude {
    // Core types
    pub use crate::{AgentError, Message, MessageRole, ModelConfig};

    // Config types
    pub use crate::{EdgeConfig, EdgeEndpoint, EdgeType, NodeConfig, NodeType, PipelineConfig};

    // Engine
    pub use crate::{EngineOutput, PipelineEngine};

    // LLM
    pub use crate::{ChatResponse, LlmResponse, LlmStream, StreamChunk, UnifiedLlmClient};

    // Tools
    pub use crate::{Tool, ToolError, ToolRegistry, ToolSchema};
}
