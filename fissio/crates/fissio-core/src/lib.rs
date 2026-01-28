//! Core domain types and error definitions for fissio.
//!
//! This crate provides the fundamental types shared across the fissio framework:
//!
//! - [`AgentError`] — Error type for pipeline and LLM operations
//! - [`Message`] and [`MessageRole`] — Conversation message types
//! - [`ModelConfig`] — LLM model configuration
//! - [`ToolCall`], [`ToolResult`], [`ToolSchema`] — Tool interaction types
//!
//! # Example
//!
//! ```rust
//! use fissio_core::{Message, MessageRole, ModelConfig};
//!
//! let msg = Message {
//!     role: MessageRole::User,
//!     content: "Hello!".to_string(),
//! };
//!
//! let model = ModelConfig {
//!     id: "gpt-4".to_string(),
//!     name: "GPT-4".to_string(),
//!     model: "gpt-4-turbo".to_string(),
//!     api_base: None,
//! };
//! ```

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors that can occur during pipeline execution or LLM operations.
#[derive(Error, Debug)]
pub enum AgentError {
    /// LLM API request failed.
    #[error("LLM request failed: {0}")]
    LlmError(String),

    /// Failed to parse structured output from LLM.
    #[error("Failed to parse structured output: {0}")]
    ParseError(String),

    /// Worker node execution failed.
    #[error("Worker execution failed: {0}")]
    WorkerFailed(String),

    /// External API call failed.
    #[error("External API error: {0}")]
    ExternalApi(String),

    /// Maximum retry attempts exceeded.
    #[error("Max retries exceeded")]
    MaxRetriesExceeded,

    /// Unknown worker type specified.
    #[error("Unknown worker type: {0}")]
    UnknownWorker(String),

    /// WebSocket communication error.
    #[error("WebSocket error: {0}")]
    WebSocket(String),
}

impl From<serde_json::Error> for AgentError {
    fn from(err: serde_json::Error) -> Self {
        AgentError::ParseError(err.to_string())
    }
}

/// Role of a message in a conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    /// Message from the user.
    User,
    /// Message from the assistant/LLM.
    Assistant,
}

/// A single message in a conversation history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// The role of the message sender.
    pub role: MessageRole,
    /// The content of the message.
    pub content: String,
}

impl Message {
    /// Creates a new user message.
    pub fn user(content: impl Into<String>) -> Self {
        Self { role: MessageRole::User, content: content.into() }
    }

    /// Creates a new assistant message.
    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: MessageRole::Assistant, content: content.into() }
    }
}

/// Configuration for an LLM model.
///
/// Used to specify which model to use for pipeline nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    /// Unique identifier for this model configuration.
    pub id: String,
    /// Human-readable display name.
    pub name: String,
    /// The actual model identifier (e.g., "gpt-4-turbo", "claude-3-opus").
    pub model: String,
    /// Optional API base URL for self-hosted or alternative endpoints.
    pub api_base: Option<String>,
}

// ============================================================================
// Tool Types
// ============================================================================

/// A tool call requested by the LLM.
///
/// When an LLM decides to use a tool, it returns one or more `ToolCall`
/// instances with the tool name and arguments to execute.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Unique identifier for this tool call (used to match results).
    pub id: String,
    /// Name of the tool to execute.
    pub name: String,
    /// Arguments to pass to the tool (JSON object).
    pub arguments: serde_json::Value,
}

/// Result of a tool execution to be sent back to the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// ID from the original tool call request.
    pub tool_call_id: String,
    /// Output content from the tool execution.
    pub content: String,
}

/// JSON schema describing a tool for LLM function calling.
///
/// This follows the OpenAI function calling format and is used
/// to inform the LLM about available tools and their parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    /// Unique name of the tool (e.g., "web_search", "fetch_url").
    pub name: String,
    /// Human-readable description of what the tool does.
    pub description: String,
    /// JSON Schema object describing the tool's parameters.
    pub parameters: serde_json::Value,
}

