//! Tool registry and built-in tools for fissio pipelines.
//!
//! This crate provides the tool abstraction for LLM function calling in fissio:
//!
//! - [`Tool`] — Trait for implementing custom tools
//! - [`ToolRegistry`] — Registry for managing available tools
//! - [`ToolSchema`] — JSON schema for tool parameters
//! - [`FetchUrlTool`] — Built-in HTTP fetch tool
//! - [`WebSearchTool`] — Built-in web search (requires Tavily API key)
//!
//! # Implementing a Custom Tool
//!
//! ```rust,ignore
//! use fissio_tools::{Tool, ToolError, ToolSchema};
//! use async_trait::async_trait;
//!
//! struct CalculatorTool;
//!
//! #[async_trait]
//! impl Tool for CalculatorTool {
//!     fn name(&self) -> &str { "calculator" }
//!     fn description(&self) -> &str { "Performs math calculations" }
//!     fn parameters(&self) -> serde_json::Value {
//!         serde_json::json!({
//!             "type": "object",
//!             "properties": {
//!                 "expression": { "type": "string" }
//!             },
//!             "required": ["expression"]
//!         })
//!     }
//!     async fn execute(&self, args: serde_json::Value) -> Result<String, ToolError> {
//!         // implementation
//!         Ok("42".to_string())
//!     }
//! }
//! ```
//!
//! # Using the Registry
//!
//! ```rust,ignore
//! use fissio_tools::ToolRegistry;
//!
//! // Create with defaults (includes fetch_url, web_search if TAVILY_API_KEY set)
//! let registry = ToolRegistry::with_defaults();
//!
//! // Or build manually
//! let mut registry = ToolRegistry::new();
//! registry.register(MyCustomTool);
//!
//! // Get schemas for LLM
//! let schemas = registry.schemas_for(&["fetch_url".to_string()]);
//! ```

mod fetch_url;
mod web_search;

pub use fetch_url::FetchUrlTool;
pub use web_search::WebSearchTool;

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

pub use fissio_core::{ToolCall, ToolResult, ToolSchema};

/// Errors that can occur during tool execution.
#[derive(Error, Debug)]
pub enum ToolError {
    /// Tool execution failed with a message.
    #[error("Tool execution failed: {0}")]
    ExecutionFailed(String),

    /// Invalid arguments were passed to the tool.
    #[error("Invalid arguments: {0}")]
    InvalidArguments(String),

    /// Network request failed.
    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),

    /// Requested tool was not found in the registry.
    #[error("Tool not found: {0}")]
    NotFound(String),
}

/// Trait for implementing tools that can be called by LLMs.
///
/// Tools are the bridge between LLM reasoning and external actions.
/// Implement this trait to create custom tools for your pipeline.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Returns the unique name of this tool.
    fn name(&self) -> &str;

    /// Returns a description of what this tool does.
    fn description(&self) -> &str;

    /// Returns the JSON Schema for this tool's parameters.
    fn parameters(&self) -> serde_json::Value;

    /// Executes the tool with the given arguments.
    ///
    /// # Arguments
    /// * `args` - JSON object containing the tool arguments
    ///
    /// # Returns
    /// The tool's output as a string, or an error.
    async fn execute(&self, args: serde_json::Value) -> Result<String, ToolError>;

    /// Generates the schema for this tool (default implementation).
    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters: self.parameters(),
        }
    }
}

/// Registry of tools available to pipeline nodes.
///
/// The registry manages tool instances and provides schemas for LLM function calling.
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolRegistry {
    /// Creates an empty tool registry.
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Creates a registry with default built-in tools.
    ///
    /// Includes:
    /// - `fetch_url` — Always available
    /// - `web_search` — Available if `TAVILY_API_KEY` env var is set
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();

        registry.register(FetchUrlTool::new());

        if let Ok(api_key) = std::env::var("TAVILY_API_KEY") {
            registry.register(WebSearchTool::new(api_key));
        }

        registry
    }

    /// Registers a tool in the registry.
    ///
    /// If a tool with the same name already exists, it will be replaced.
    pub fn register<T: Tool + 'static>(&mut self, tool: T) {
        self.tools.insert(tool.name().to_string(), Arc::new(tool));
    }

    /// Gets a tool by name.
    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    /// Returns schemas for all registered tools.
    pub fn list(&self) -> Vec<ToolSchema> {
        self.tools.values().map(|t| t.schema()).collect()
    }

    /// Returns schemas for the specified tool names.
    ///
    /// Unknown tool names are silently ignored.
    pub fn schemas_for(&self, names: &[String]) -> Vec<ToolSchema> {
        names
            .iter()
            .filter_map(|name| self.tools.get(name).map(|t| t.schema()))
            .collect()
    }

    /// Returns true if a tool with the given name is registered.
    pub fn has(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    /// Returns the names of all registered tools.
    pub fn tool_names(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }
}
