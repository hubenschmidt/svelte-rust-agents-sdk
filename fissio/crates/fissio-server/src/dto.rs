//! Data transfer objects for HTTP message serialization.

use std::collections::HashMap;
use std::fmt;

use fissio_core::ModelConfig;
use serde::{Deserialize, Serialize};

// === Model Management Types ===

/// Response from model warmup.
#[derive(Debug, Serialize)]
pub struct WakeResponse {
    pub success: bool,
    pub model: String,
}

/// Response from model unload.
#[derive(Debug, Serialize)]
pub struct UnloadResponse {
    pub success: bool,
}

// === Runtime Pipeline Config Types ===

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

