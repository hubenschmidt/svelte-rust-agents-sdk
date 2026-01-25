//! Pipeline configuration schema and preset registry.
//!
//! Defines the data structures for agent pipelines: nodes, edges, and their types.
//! Provides a registry for loading preset configurations from JSON files.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Configuration parsing and loading errors.
#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]
    Io(#[from] std::io::Error),

    #[error("Failed to parse config: {0}")]
    Parse(#[from] serde_json::Error),

    #[error("Preset not found: {0}")]
    PresetNotFound(String),
}

/// Types of nodes in a pipeline graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeType {
    Llm,
    Gate,
    Router,
    Coordinator,
    Aggregator,
    Orchestrator,
    Worker,
    Synthesizer,
    Evaluator,
}

impl FromStr for NodeType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "llm" => Ok(Self::Llm),
            "gate" => Ok(Self::Gate),
            "router" => Ok(Self::Router),
            "coordinator" => Ok(Self::Coordinator),
            "aggregator" => Ok(Self::Aggregator),
            "orchestrator" => Ok(Self::Orchestrator),
            "worker" => Ok(Self::Worker),
            "synthesizer" => Ok(Self::Synthesizer),
            "evaluator" => Ok(Self::Evaluator),
            _ => Err(()),
        }
    }
}

impl NodeType {
    /// Returns true if this node type makes an LLM call.
    pub fn requires_llm(&self) -> bool {
        matches!(self, NodeType::Llm | NodeType::Worker)
    }

    /// Returns a human-readable label for logging.
    pub fn action_label(&self) -> &'static str {
        match self {
            NodeType::Llm => "Calling LLM",
            NodeType::Gate => "Gate check",
            NodeType::Router => "Routing",
            NodeType::Coordinator => "Coordinating",
            NodeType::Orchestrator => "Orchestrating",
            NodeType::Aggregator => "Aggregating",
            NodeType::Synthesizer => "Synthesizing",
            NodeType::Worker => "Worker executing",
            NodeType::Evaluator => "Evaluating",
        }
    }
}

/// Types of edges connecting nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EdgeType {
    #[default]
    Direct,
    Dynamic,
    Conditional,
    Parallel,
}

impl FromStr for EdgeType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "parallel" => Ok(Self::Parallel),
            "dynamic" => Ok(Self::Dynamic),
            "conditional" => Ok(Self::Conditional),
            _ => Ok(Self::Direct),
        }
    }
}

/// Configuration for a single node in the pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: NodeType,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub config: serde_json::Value,
    #[serde(default)]
    pub prompt: Option<String>,
}

/// Configuration for an edge connecting nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeConfig {
    pub from: EdgeEndpoint,
    pub to: EdgeEndpoint,
    #[serde(default)]
    pub edge_type: EdgeType,
}

/// An edge endpoint: either a single node ID or multiple node IDs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EdgeEndpoint {
    Single(String),
    Multiple(Vec<String>),
}

impl EdgeEndpoint {
    /// Returns the endpoint as a vector of string slices.
    pub fn as_vec(&self) -> Vec<&str> {
        match self {
            EdgeEndpoint::Single(s) => vec![s.as_str()],
            EdgeEndpoint::Multiple(v) => v.iter().map(|s| s.as_str()).collect(),
        }
    }
}

/// Complete pipeline configuration with nodes and edges.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub nodes: Vec<NodeConfig>,
    pub edges: Vec<EdgeConfig>,
}

/// Registry of preset pipeline configurations loaded from disk.
#[derive(Debug, Default)]
pub struct PresetRegistry {
    presets: HashMap<String, PipelineConfig>,
}

impl PresetRegistry {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Loads all JSON preset files from a directory.
    pub fn load_from_dir(dir: &Path) -> Result<Self, ConfigError> {
        let mut registry = Self::new();

        for entry in fs::read_dir(dir)?.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "json") {
                let content = fs::read_to_string(&path)?;
                let config: PipelineConfig = serde_json::from_str(&content)?;
                registry.presets.insert(config.id.clone(), config);
            }
        }

        Ok(registry)
    }

    /// Gets a preset by ID.
    pub fn get(&self, id: &str) -> Option<&PipelineConfig> {
        self.presets.get(id)
    }

    /// Returns all loaded presets.
    pub fn list(&self) -> Vec<&PipelineConfig> {
        self.presets.values().collect()
    }

    /// Returns all preset IDs.
    pub fn ids(&self) -> Vec<&str> {
        self.presets.keys().map(|s| s.as_str()).collect()
    }
}
