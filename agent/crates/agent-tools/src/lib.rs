mod fetch_url;
mod web_search;

pub use fetch_url::FetchUrlTool;
pub use web_search::WebSearchTool;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ToolError {
    #[error("Tool execution failed: {0}")]
    ExecutionFailed(String),
    #[error("Invalid arguments: {0}")]
    InvalidArguments(String),
    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),
    #[error("Tool not found: {0}")]
    NotFound(String),
}

/// Schema for a tool, used for LLM function calling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Result of a tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub tool_call_id: String,
    pub content: String,
}

/// Trait for implementing tools
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> serde_json::Value;
    async fn execute(&self, args: serde_json::Value) -> Result<String, ToolError>;

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters: self.parameters(),
        }
    }
}

/// Registry of available tools
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Create a registry with default built-in tools
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();

        // Always available tools
        registry.register(FetchUrlTool::new());

        // Register web search if API key is available
        if let Ok(api_key) = std::env::var("TAVILY_API_KEY") {
            registry.register(WebSearchTool::new(api_key));
        }

        registry
    }

    pub fn register<T: Tool + 'static>(&mut self, tool: T) {
        self.tools.insert(tool.name().to_string(), Arc::new(tool));
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    pub fn list(&self) -> Vec<ToolSchema> {
        self.tools.values().map(|t| t.schema()).collect()
    }

    pub fn schemas_for(&self, names: &[String]) -> Vec<ToolSchema> {
        names
            .iter()
            .filter_map(|name| self.tools.get(name).map(|t| t.schema()))
            .collect()
    }

    pub fn has(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    pub fn tool_names(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }
}
