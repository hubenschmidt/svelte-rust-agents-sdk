//! Data transfer objects for HTTP and WebSocket message serialization.

use std::collections::HashMap;
use std::fmt;

use fissio_core::{Message, ModelConfig};
use serde::{Deserialize, Serialize};

// === HTTP Request/Response Types ===

/// Request to warm up a model.
#[derive(Debug, Deserialize)]
pub struct WakeRequest {
    pub model_id: String,
    pub previous_model_id: Option<String>,
}

/// Response from model warmup.
#[derive(Debug, Serialize)]
pub struct WakeResponse {
    pub success: bool,
    pub model: String,
}

/// Request to unload a model.
#[derive(Debug, Deserialize)]
pub struct UnloadRequest {
    pub model_id: String,
}

/// Response from model unload.
#[derive(Debug, Serialize)]
pub struct UnloadResponse {
    pub success: bool,
}

// === WebSocket Message Types ===

/// Runtime node configuration from the frontend.
#[derive(Debug, Clone, Deserialize)]
pub struct RuntimeNodeConfig {
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub tools: Option<Vec<String>>,
}

/// Runtime edge configuration from the frontend.
#[derive(Debug, Clone, Deserialize)]
pub struct RuntimeEdgeConfig {
    pub from: serde_json::Value,
    pub to: serde_json::Value,
    #[serde(default)]
    pub edge_type: Option<String>,
}

/// Complete runtime pipeline configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct RuntimePipelineConfig {
    pub nodes: Vec<RuntimeNodeConfig>,
    pub edges: Vec<RuntimeEdgeConfig>,
}

/// Incoming WebSocket message payload.
#[derive(Debug, Deserialize)]
pub struct WsPayload {
    pub uuid: Option<String>,
    pub message: Option<String>,
    pub model_id: Option<String>,
    pub pipeline_id: Option<String>,
    #[serde(default)]
    pub node_models: HashMap<String, String>,
    #[serde(default)]
    pub init: bool,
    #[serde(default)]
    pub verbose: bool,
    pub wake_model_id: Option<String>,
    pub unload_model_id: Option<String>,
    #[serde(default)]
    pub history: Vec<Message>,
    pub pipeline_config: Option<RuntimePipelineConfig>,
    pub system_prompt: Option<String>,
}

// === Pipeline Info Types ===

/// Node information for API responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub id: String,
    pub node_type: String,
    pub model: Option<String>,
    pub prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y: Option<f64>,
}

/// Edge information for API responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeInfo {
    pub from: serde_json::Value,
    pub to: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge_type: Option<String>,
}

/// Position for layout storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

/// Complete pipeline information for API responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub nodes: Vec<NodeInfo>,
    pub edges: Vec<EdgeInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<HashMap<String, Position>>,
}

// === Pipeline CRUD Types ===

/// Request to save a pipeline configuration.
#[derive(Debug, Deserialize)]
pub struct SavePipelineRequest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub nodes: Vec<NodeInfo>,
    pub edges: Vec<EdgeInfo>,
    #[serde(default)]
    pub layout: Option<HashMap<String, Position>>,
}

/// Response from saving a pipeline.
#[derive(Debug, Serialize)]
pub struct SavePipelineResponse {
    pub success: bool,
    pub id: String,
}

/// Request to delete a pipeline.
#[derive(Debug, Deserialize)]
pub struct DeletePipelineRequest {
    pub id: String,
}

/// Response sent on WebSocket connection init.
#[derive(Debug, Serialize)]
pub struct InitResponse {
    pub models: Vec<ModelConfig>,
    pub templates: Vec<PipelineInfo>,
    pub configs: Vec<PipelineInfo>,
}

/// Metadata about an LLM response (timing, tokens).
#[derive(Debug, Clone, Serialize, Default)]
pub struct WsMetadata {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub elapsed_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub load_duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_eval_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eval_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_per_sec: Option<f64>,
}

impl fmt::Display for WsMetadata {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}ms, {}/{} tokens", self.elapsed_ms, self.input_tokens, self.output_tokens)?;
        if let Some(tps) = self.tokens_per_sec {
            write!(f, ", {:.1} tok/s", tps)?;
        }
        Ok(())
    }
}

/// Outgoing WebSocket message types.
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum WsResponse {
    Stream { on_chat_model_stream: String },
    End { on_chat_model_end: bool, metadata: Option<WsMetadata> },
    ModelStatus { model_status: String },
}

impl WsResponse {
    /// Creates a streaming content chunk response.
    pub fn stream(content: &str) -> Self {
        Self::Stream { on_chat_model_stream: content.to_string() }
    }

    /// Creates an end-of-stream response with metadata.
    pub fn end_with_metadata(metadata: WsMetadata) -> Self {
        Self::End { on_chat_model_end: true, metadata: Some(metadata) }
    }

    /// Creates a model status update response.
    pub fn model_status(status: &str) -> Self {
        Self::ModelStatus { model_status: status.to_string() }
    }
}
