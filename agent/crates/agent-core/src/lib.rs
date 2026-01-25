//! Core domain types, error definitions, and worker trait.
//!
//! This crate defines the fundamental types shared across the agent system:
//! errors, worker abstractions, message types, and model configuration.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors that can occur during agent operations.
#[derive(Error, Debug)]
pub enum AgentError {
    #[error("LLM request failed: {0}")]
    LlmError(String),

    #[error("Failed to parse structured output: {0}")]
    ParseError(String),

    #[error("Worker execution failed: {0}")]
    WorkerFailed(String),

    #[error("External API error: {0}")]
    ExternalApi(String),

    #[error("Max retries exceeded")]
    MaxRetriesExceeded,

    #[error("Unknown worker type: {0}")]
    UnknownWorker(String),

    #[error("WebSocket error: {0}")]
    WebSocket(String),
}

impl From<serde_json::Error> for AgentError {
    fn from(err: serde_json::Error) -> Self {
        AgentError::ParseError(err.to_string())
    }
}

/// Types of workers that can execute tasks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum WorkerType {
    Search,
    Email,
    General,
}

/// A handoff request to transfer work to another worker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Handoff {
    pub target: WorkerType,
    pub context: String,
}

/// Decision made by the orchestrator about which worker to use.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorDecision {
    pub worker_type: WorkerType,
    pub task_description: String,
    pub parameters: serde_json::Value,
    pub success_criteria: String,
}

/// Result of an evaluator checking work quality.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluatorResult {
    pub passed: bool,
    pub score: u8,
    pub feedback: String,
    #[serde(default)]
    pub suggestions: String,
}

/// Result returned by a worker after execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerResult {
    pub success: bool,
    pub output: String,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub handoff: Option<Handoff>,
}

impl WorkerResult {
    /// Creates a successful result with the given output.
    pub fn ok(output: String) -> Self {
        Self { success: true, output, error: None, handoff: None }
    }

    /// Creates a failed result with the given error.
    pub fn err(e: impl ToString) -> Self {
        Self { success: false, output: String::new(), error: Some(e.to_string()), handoff: None }
    }

    /// Creates a result that hands off to another worker.
    pub fn handoff(target: WorkerType, context: String) -> Self {
        Self {
            success: true,
            output: String::new(),
            error: None,
            handoff: Some(Handoff { target, context }),
        }
    }
}

/// Parameters for sending an email.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailParams {
    pub to: String,
    pub subject: String,
    pub body: String,
}

/// Parameters for a search query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchParams {
    pub query: String,
    #[serde(default = "default_num_results")]
    pub num_results: u8,
}

fn default_num_results() -> u8 {
    5
}

/// Role of a message in a conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
}

/// A single message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: MessageRole,
    pub content: String,
}

/// Decision made by the frontline agent about routing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrontlineDecision {
    pub should_route: bool,
    pub response: String,
}

/// Configuration for an LLM model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub id: String,
    pub name: String,
    pub model: String,
    pub api_base: Option<String>,
}

/// Trait for workers that can execute tasks.
#[async_trait]
pub trait Worker: Send + Sync {
    /// Returns the type of this worker.
    fn worker_type(&self) -> WorkerType;

    /// Executes a task with the given parameters.
    async fn execute(
        &self,
        task_description: &str,
        parameters: &serde_json::Value,
        feedback: Option<&str>,
        model: &ModelConfig,
    ) -> Result<WorkerResult, AgentError>;
}
