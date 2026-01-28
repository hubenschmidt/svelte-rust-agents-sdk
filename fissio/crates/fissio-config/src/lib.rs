//! Pipeline configuration schema and preset registry.
//!
//! This crate defines the data structures for fissio pipelines:
//!
//! - [`PipelineConfig`] — Complete pipeline definition with nodes and edges
//! - [`PipelineBuilder`] — Fluent API for building pipelines programmatically
//! - [`NodeConfig`] — Configuration for individual pipeline nodes
//! - [`EdgeConfig`] — Connections between nodes with routing behavior
//! - [`NodeType`] and [`EdgeType`] — Available node and edge types
//! - [`PresetRegistry`] — Load pipeline presets from JSON files
//!
//! # Loading from JSON
//!
//! ```rust,ignore
//! use fissio_config::PipelineConfig;
//!
//! let config = PipelineConfig::from_file("pipeline.json")?;
//! ```
//!
//! # Builder API
//!
//! ```rust
//! use fissio_config::{PipelineConfig, NodeType};
//!
//! let config = PipelineConfig::builder("assistant", "My Assistant")
//!     .description("A helpful assistant pipeline")
//!     .node("llm", NodeType::Llm)
//!         .prompt("You are a helpful assistant.")
//!         .model("gpt-4")
//!         .done()
//!     .edge("input", "llm")
//!     .edge("llm", "output")
//!     .build();
//!
//! assert_eq!(config.nodes.len(), 1);
//! assert_eq!(config.edges.len(), 2);
//! ```

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Errors that can occur when loading or parsing configurations.
#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
    /// Failed to read a configuration file.
    #[error("Failed to read config file '{path}': {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    /// Failed to parse JSON configuration.
    #[error("Failed to parse config: {0}")]
    Parse(#[from] serde_json::Error),

    /// Requested preset was not found in the registry.
    #[error("Preset not found: '{0}'")]
    PresetNotFound(String),

    /// Pipeline validation failed.
    #[error("Invalid pipeline '{pipeline_id}': {message}")]
    Validation {
        pipeline_id: String,
        message: String,
    },

    /// Node not found in pipeline.
    #[error("Node '{node_id}' not found in pipeline '{pipeline_id}'")]
    NodeNotFound {
        pipeline_id: String,
        node_id: String,
    },
}

impl ConfigError {
    /// Creates an IO error with path context.
    pub fn io(path: impl Into<String>, source: std::io::Error) -> Self {
        Self::Io { path: path.into(), source }
    }

    /// Creates a validation error.
    pub fn validation(pipeline_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Validation {
            pipeline_id: pipeline_id.into(),
            message: message.into(),
        }
    }
}

/// Types of nodes available in a pipeline.
///
/// Each node type has different execution behavior:
///
/// | Type | Description |
/// |------|-------------|
/// | `Llm` | Simple LLM call with prompt |
/// | `Worker` | LLM with tools (agentic loop) |
/// | `Router` | Classifies input, routes to targets |
/// | `Gate` | Validates before proceeding |
/// | `Aggregator` | Combines multiple inputs |
/// | `Orchestrator` | Dynamic task decomposition |
/// | `Evaluator` | Quality scoring |
/// | `Synthesizer` | Synthesizes inputs |
/// | `Coordinator` | Distributes to workers |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeType {
    /// Simple LLM call with a system prompt.
    Llm,
    /// Validates input before proceeding.
    Gate,
    /// Routes input to different nodes based on classification.
    Router,
    /// Coordinates distribution of work.
    Coordinator,
    /// Aggregates outputs from multiple nodes.
    Aggregator,
    /// Dynamically decomposes tasks.
    Orchestrator,
    /// LLM with tool calling (agentic loop).
    Worker,
    /// Synthesizes multiple inputs into one output.
    Synthesizer,
    /// Evaluates quality of outputs.
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

impl std::fmt::Display for NodeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Llm => "llm",
            Self::Gate => "gate",
            Self::Router => "router",
            Self::Coordinator => "coordinator",
            Self::Aggregator => "aggregator",
            Self::Orchestrator => "orchestrator",
            Self::Worker => "worker",
            Self::Synthesizer => "synthesizer",
            Self::Evaluator => "evaluator",
        };
        write!(f, "{}", s)
    }
}

impl NodeType {
    /// Returns `true` if this node type makes an LLM call.
    pub fn requires_llm(&self) -> bool {
        matches!(self, NodeType::Llm | NodeType::Worker)
    }

    /// Returns `true` if this node type performs routing decisions.
    pub fn is_router(&self) -> bool {
        matches!(self, NodeType::Router)
    }

    /// Returns a human-readable label for logging.
    #[doc(hidden)]
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

/// Types of edges connecting nodes in a pipeline.
///
/// | Type | Description |
/// |------|-------------|
/// | `Direct` | Sequential execution (default) |
/// | `Parallel` | Concurrent execution of targets |
/// | `Conditional` | Router chooses which path to follow |
/// | `Dynamic` | Orchestrator dynamically selects targets |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EdgeType {
    /// Sequential execution (default).
    #[default]
    Direct,
    /// Orchestrator dynamically selects targets.
    Dynamic,
    /// Router chooses which path to follow.
    Conditional,
    /// Concurrent execution of all targets.
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

impl std::fmt::Display for EdgeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Direct => "direct",
            Self::Dynamic => "dynamic",
            Self::Conditional => "conditional",
            Self::Parallel => "parallel",
        };
        write!(f, "{}", s)
    }
}

/// Configuration for a single node in a pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    /// Unique identifier for this node within the pipeline.
    pub id: String,
    /// The type of node determining its execution behavior.
    #[serde(rename = "type")]
    pub node_type: NodeType,
    /// Optional model ID to use for this node (overrides default).
    #[serde(default)]
    pub model: Option<String>,
    /// Additional configuration (node-type specific).
    #[serde(default)]
    pub config: serde_json::Value,
    /// System prompt for LLM-based nodes.
    #[serde(default)]
    pub prompt: Option<String>,
    /// Tool names this node can access (from the tool registry).
    #[serde(default)]
    pub tools: Vec<String>,
}

/// Configuration for an edge connecting nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeConfig {
    /// Source node(s) for this edge.
    pub from: EdgeEndpoint,
    /// Target node(s) for this edge.
    pub to: EdgeEndpoint,
    /// How this edge should be traversed.
    #[serde(default)]
    pub edge_type: EdgeType,
}

/// An edge endpoint: either a single node ID or multiple node IDs.
///
/// Use `Single` for one-to-one connections, `Multiple` for fan-out/fan-in.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EdgeEndpoint {
    /// A single node ID.
    Single(String),
    /// Multiple node IDs (for parallel edges).
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

impl From<&serde_json::Value> for EdgeEndpoint {
    fn from(val: &serde_json::Value) -> Self {
        match val {
            serde_json::Value::String(s) => EdgeEndpoint::Single(s.clone()),
            serde_json::Value::Array(arr) => {
                let strings: Vec<String> = arr
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect();
                EdgeEndpoint::Multiple(strings)
            }
            _ => EdgeEndpoint::Single(String::new()),
        }
    }
}

impl From<serde_json::Value> for EdgeEndpoint {
    fn from(val: serde_json::Value) -> Self {
        EdgeEndpoint::from(&val)
    }
}

impl From<&EdgeEndpoint> for serde_json::Value {
    fn from(ep: &EdgeEndpoint) -> Self {
        match ep {
            EdgeEndpoint::Single(s) => serde_json::Value::String(s.clone()),
            EdgeEndpoint::Multiple(v) => {
                serde_json::Value::Array(v.iter().map(|s| serde_json::Value::String(s.clone())).collect())
            }
        }
    }
}

impl From<EdgeEndpoint> for serde_json::Value {
    fn from(ep: EdgeEndpoint) -> Self {
        serde_json::Value::from(&ep)
    }
}

/// Complete pipeline configuration with nodes and edges.
///
/// A pipeline is a directed graph where nodes are processing steps
/// and edges define the flow of data between them.
///
/// # Loading from File
///
/// ```rust,ignore
/// let config = PipelineConfig::from_file("pipeline.json")?;
/// ```
///
/// # Building Programmatically
///
/// ```rust,ignore
/// let config = PipelineConfig::builder("my-pipeline", "My Pipeline")
///     .description("A simple pipeline")
///     .node("assistant", NodeType::Llm)
///         .prompt("You are helpful.")
///         .model("gpt-4")
///         .done()
///     .node("researcher", NodeType::Worker)
///         .tools(["web_search", "fetch_url"])
///         .done()
///     .edge("input", "assistant")
///     .edge("assistant", "researcher")
///     .edge("researcher", "output")
///     .build();
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    /// Unique identifier for this pipeline.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Optional description of what this pipeline does.
    #[serde(default)]
    pub description: String,
    /// The nodes in this pipeline.
    pub nodes: Vec<NodeConfig>,
    /// The edges connecting nodes.
    pub edges: Vec<EdgeConfig>,
}

impl PipelineConfig {
    /// Creates a new builder for constructing a pipeline programmatically.
    pub fn builder(id: impl Into<String>, name: impl Into<String>) -> PipelineBuilder {
        PipelineBuilder::new(id, name)
    }

    /// Loads a pipeline configuration from a JSON file.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let content = fs::read_to_string(path)
            .map_err(|e| ConfigError::io(path.display().to_string(), e))?;
        Self::from_json(&content)
    }

    /// Parses a pipeline configuration from a JSON string.
    pub fn from_json(json: &str) -> Result<Self, ConfigError> {
        Ok(serde_json::from_str(json)?)
    }

    /// Serializes this configuration to a JSON string.
    pub fn to_json(&self) -> Result<String, ConfigError> {
        Ok(serde_json::to_string_pretty(self)?)
    }
}

// ============================================================================
// Builder API
// ============================================================================

/// Builder for constructing [`PipelineConfig`] programmatically.
///
/// Use [`PipelineConfig::builder()`] to create a new builder.
#[derive(Debug)]
pub struct PipelineBuilder {
    id: String,
    name: String,
    description: String,
    nodes: Vec<NodeConfig>,
    edges: Vec<EdgeConfig>,
}

impl PipelineBuilder {
    /// Creates a new pipeline builder.
    fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: String::new(),
            nodes: Vec::new(),
            edges: Vec::new(),
        }
    }

    /// Sets the pipeline description.
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// Starts building a new node with the given ID and type.
    pub fn node(self, id: impl Into<String>, node_type: NodeType) -> NodeBuilder {
        NodeBuilder::new(self, id.into(), node_type)
    }

    /// Adds a simple edge from one node to another.
    pub fn edge(mut self, from: impl Into<String>, to: impl Into<String>) -> Self {
        self.edges.push(EdgeConfig {
            from: EdgeEndpoint::Single(from.into()),
            to: EdgeEndpoint::Single(to.into()),
            edge_type: EdgeType::Direct,
        });
        self
    }

    /// Adds an edge with a specific type.
    pub fn edge_typed(
        mut self,
        from: impl Into<String>,
        to: impl Into<String>,
        edge_type: EdgeType,
    ) -> Self {
        self.edges.push(EdgeConfig {
            from: EdgeEndpoint::Single(from.into()),
            to: EdgeEndpoint::Single(to.into()),
            edge_type,
        });
        self
    }

    /// Adds a parallel edge from one node to multiple targets.
    pub fn parallel_edge(mut self, from: impl Into<String>, to: &[&str]) -> Self {
        self.edges.push(EdgeConfig {
            from: EdgeEndpoint::Single(from.into()),
            to: EdgeEndpoint::Multiple(to.iter().map(|s| s.to_string()).collect()),
            edge_type: EdgeType::Parallel,
        });
        self
    }

    /// Adds a conditional edge (for routers).
    pub fn conditional_edge(mut self, from: impl Into<String>, to: &[&str]) -> Self {
        self.edges.push(EdgeConfig {
            from: EdgeEndpoint::Single(from.into()),
            to: EdgeEndpoint::Multiple(to.iter().map(|s| s.to_string()).collect()),
            edge_type: EdgeType::Conditional,
        });
        self
    }

    /// Builds the final [`PipelineConfig`].
    pub fn build(self) -> PipelineConfig {
        PipelineConfig {
            id: self.id,
            name: self.name,
            description: self.description,
            nodes: self.nodes,
            edges: self.edges,
        }
    }

    /// Internal: adds a completed node.
    fn add_node(mut self, node: NodeConfig) -> Self {
        self.nodes.push(node);
        self
    }
}

/// Builder for constructing a single node within a pipeline.
///
/// Created via [`PipelineBuilder::node()`].
#[derive(Debug)]
pub struct NodeBuilder {
    pipeline: PipelineBuilder,
    id: String,
    node_type: NodeType,
    model: Option<String>,
    prompt: Option<String>,
    tools: Vec<String>,
    config: serde_json::Value,
}

impl NodeBuilder {
    fn new(pipeline: PipelineBuilder, id: String, node_type: NodeType) -> Self {
        Self {
            pipeline,
            id,
            node_type,
            model: None,
            prompt: None,
            tools: Vec::new(),
            config: serde_json::Value::Null,
        }
    }

    /// Sets the model ID for this node.
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Sets the system prompt for this node.
    pub fn prompt(mut self, prompt: impl Into<String>) -> Self {
        self.prompt = Some(prompt.into());
        self
    }

    /// Sets the tools available to this node.
    pub fn tools<I, S>(mut self, tools: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.tools = tools.into_iter().map(Into::into).collect();
        self
    }

    /// Sets additional configuration for this node.
    pub fn config(mut self, config: serde_json::Value) -> Self {
        self.config = config;
        self
    }

    /// Finishes building this node and returns to the pipeline builder.
    pub fn done(self) -> PipelineBuilder {
        let node = NodeConfig {
            id: self.id,
            node_type: self.node_type,
            model: self.model,
            prompt: self.prompt,
            tools: self.tools,
            config: self.config,
        };
        self.pipeline.add_node(node)
    }
}

/// Registry of preset pipeline configurations loaded from disk.
///
/// Use this to load and manage reusable pipeline templates.
///
/// # Example
///
/// ```rust,ignore
/// use fissio_config::PresetRegistry;
/// use std::path::Path;
///
/// let registry = PresetRegistry::load_from_dir(Path::new("presets"))?;
/// for id in registry.ids() {
///     println!("Found preset: {}", id);
/// }
/// ```
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
    ///
    /// Each `.json` file in the directory should contain a valid `PipelineConfig`.
    pub fn load_from_dir(dir: &Path) -> Result<Self, ConfigError> {
        let mut registry = Self::new();

        let entries = fs::read_dir(dir)
            .map_err(|e| ConfigError::io(dir.display().to_string(), e))?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "json") {
                let content = fs::read_to_string(&path)
                    .map_err(|e| ConfigError::io(path.display().to_string(), e))?;
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
