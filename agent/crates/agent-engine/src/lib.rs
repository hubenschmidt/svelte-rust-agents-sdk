//! Pipeline execution engine with parallel and sequential node traversal.
//!
//! Executes agent pipelines as directed graphs, handling different edge types
//! (direct, parallel, conditional) and node types (LLM, gate, router, etc.).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use agent_config::{EdgeConfig, EdgeEndpoint, EdgeType, NodeConfig, NodeType, PipelineConfig};
use agent_core::{AgentError, ModelConfig};
use agent_network::{LlmClient, LlmStream};
use async_recursion::async_recursion;
use futures::future::join_all;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Input data passed to a node during execution.
#[derive(Debug, Clone, Default)]
pub struct NodeInput {
    pub user_input: String,
    pub history: Vec<agent_core::Message>,
    pub context: HashMap<String, String>,
}

/// Output produced by a node after execution.
#[derive(Debug, Clone)]
pub struct NodeOutput {
    pub content: String,
    pub next_nodes: Vec<String>,
}

/// Result of pipeline execution: either a stream or complete response.
pub enum EngineOutput {
    Stream(LlmStream),
    Complete(String),
}

/// Resolves model IDs to ModelConfig, with fallback to default.
pub struct ModelResolver {
    models: HashMap<String, ModelConfig>,
    default_model: ModelConfig,
}

impl ModelResolver {
    /// Creates a resolver with available models and a default fallback.
    pub fn new(models: Vec<ModelConfig>, default: ModelConfig) -> Self {
        let map = models.into_iter().map(|m| (m.id.clone(), m)).collect();
        Self { models: map, default_model: default }
    }

    /// Resolves a model ID to its config, or returns the default.
    pub fn resolve(&self, model_id: Option<&str>) -> &ModelConfig {
        model_id
            .and_then(|id| self.models.get(id))
            .unwrap_or(&self.default_model)
    }
}

/// Executes pipeline configurations as directed graphs.
pub struct PipelineEngine {
    config: PipelineConfig,
    resolver: ModelResolver,
    node_overrides: HashMap<String, String>,
}

impl PipelineEngine {
    /// Creates a new engine with the given config, models, and node overrides.
    pub fn new(
        config: PipelineConfig,
        models: Vec<ModelConfig>,
        default_model: ModelConfig,
        node_overrides: HashMap<String, String>,
    ) -> Self {
        Self {
            config,
            resolver: ModelResolver::new(models, default_model),
            node_overrides,
        }
    }

    /// Gets the model to use for a node, considering overrides.
    fn get_node_model(&self, node: &NodeConfig) -> &ModelConfig {
        let model_id = self.node_overrides
            .get(&node.id)
            .or(node.model.as_ref());
        self.resolver.resolve(model_id.map(|s| s.as_str()))
    }

    /// Finds a node by ID.
    fn get_node(&self, id: &str) -> Option<&NodeConfig> {
        self.config.nodes.iter().find(|n| n.id == id)
    }

    /// Gets all edges originating from a node.
    fn get_outgoing_edges(&self, node_id: &str) -> Vec<&EdgeConfig> {
        self.config.edges.iter().filter(|e| {
            e.from.as_vec().contains(&node_id)
        }).collect()
    }

    /// Executes the pipeline and returns the result.
    pub async fn execute_stream(
        &self,
        user_input: &str,
        history: &[agent_core::Message],
    ) -> Result<EngineOutput, AgentError> {
        info!("╔══════════════════════════════════════════════════════════════");
        info!("║ PIPELINE: {}", self.config.name);
        info!("║ Input: {}...", user_input.chars().take(50).collect::<String>());
        info!("╠══════════════════════════════════════════════════════════════");

        if !self.node_overrides.is_empty() {
            info!("║ Node model overrides: {:?}", self.node_overrides);
        }

        let context = Arc::new(RwLock::new(HashMap::<String, String>::new()));
        context.write().await.insert("input".to_string(), user_input.to_string());

        let mut executed: HashSet<String> = HashSet::new();
        let step = Arc::new(RwLock::new(0usize));

        // Find starting edges (from "input")
        let start_edges: Vec<&EdgeConfig> = self.config.edges.iter()
            .filter(|e| matches!(&e.from, EdgeEndpoint::Single(s) if s == "input"))
            .collect();

        for start_edge in start_edges {
            self.process_edge(start_edge, &context, &mut executed, history, &step).await?;
        }

        // Find output
        let ctx = context.read().await;
        for edge in &self.config.edges {
            if !matches!(&edge.to, EdgeEndpoint::Single(s) if s == "output") {
                continue;
            }

            let from_nodes = edge.from.as_vec();
            let output = from_nodes.iter()
                .rev()
                .find_map(|id| ctx.get(*id))
                .cloned()
                .unwrap_or_default();

            info!("║ Pipeline complete");
            info!("╚══════════════════════════════════════════════════════════════");
            return Ok(EngineOutput::Complete(output));
        }

        info!("║ Pipeline complete (no output edge found)");
        info!("╚══════════════════════════════════════════════════════════════");
        Ok(EngineOutput::Complete(String::new()))
    }

    /// Processes an edge, executing target nodes based on edge type.
    #[async_recursion]
    async fn process_edge(
        &self,
        edge: &EdgeConfig,
        context: &Arc<RwLock<HashMap<String, String>>>,
        executed: &mut HashSet<String>,
        history: &[agent_core::Message],
        step: &Arc<RwLock<usize>>,
    ) -> Result<(), AgentError> {
        let target_ids = edge.to.as_vec();

        if target_ids.len() == 1 && target_ids[0] == "output" {
            return Ok(());
        }

        if edge.edge_type == EdgeType::Parallel {
            return self.execute_parallel(target_ids, context, executed, history, step).await;
        }

        self.execute_sequential(target_ids, context, executed, history, step).await
    }

    /// Executes nodes in parallel.
    async fn execute_parallel(
        &self,
        target_ids: Vec<&str>,
        context: &Arc<RwLock<HashMap<String, String>>>,
        executed: &mut HashSet<String>,
        history: &[agent_core::Message],
        step: &Arc<RwLock<usize>>,
    ) -> Result<(), AgentError> {
        info!("╠══════════════════════════════════════════════════════════════");
        info!("║ PARALLEL EXECUTION: {:?}", target_ids);

        // Gather node data
        let mut node_data = Vec::new();
        for id in target_ids.iter().filter(|&id| !executed.contains(*id)) {
            let Some(node) = self.get_node(id) else { continue };
            let input = self.get_input_for_node(id, context).await;
            let model = self.get_node_model(node).clone();
            node_data.push((node.id.clone(), node.node_type, model, node.prompt.clone(), input));
        }

        // Execute in parallel
        let futures: Vec<_> = node_data.into_iter()
            .map(|(node_id, node_type, model, prompt, input)| {
                let step = Arc::clone(step);
                async move {
                    let current_step = {
                        let mut s = step.write().await;
                        *s += 1;
                        *s
                    };
                    let result = execute_node(&node_id, node_type, &model, prompt.as_deref(), &input, current_step).await;
                    (node_id, result)
                }
            })
            .collect();

        let results = join_all(futures).await;

        // Store results
        for (node_id, result) in results {
            let output = result?;
            context.write().await.insert(node_id.clone(), output.content);
            executed.insert(node_id);
        }

        info!("║ PARALLEL EXECUTION COMPLETE");
        info!("╠══════════════════════════════════════════════════════════════");

        // Process outgoing edges
        for node_id in target_ids {
            for next_edge in self.get_outgoing_edges(node_id) {
                let next_targets = next_edge.to.as_vec();
                if !next_targets.iter().any(|t| executed.contains(*t)) {
                    self.process_edge(next_edge, context, executed, history, step).await?;
                }
            }
        }

        Ok(())
    }

    /// Executes nodes sequentially.
    async fn execute_sequential(
        &self,
        target_ids: Vec<&str>,
        context: &Arc<RwLock<HashMap<String, String>>>,
        executed: &mut HashSet<String>,
        history: &[agent_core::Message],
        step: &Arc<RwLock<usize>>,
    ) -> Result<(), AgentError> {
        for node_id in target_ids {
            if executed.contains(node_id) || node_id == "output" {
                continue;
            }

            let Some(node) = self.get_node(node_id) else { continue };
            let input = self.get_input_for_node(node_id, context).await;

            let current_step = {
                let mut s = step.write().await;
                *s += 1;
                *s
            };

            let model = self.get_node_model(node);
            let output = execute_node(node_id, node.node_type, model, node.prompt.as_deref(), &input, current_step).await?;

            context.write().await.insert(node_id.to_string(), output.content);
            executed.insert(node_id.to_string());

            for next_edge in self.get_outgoing_edges(node_id) {
                self.process_edge(next_edge, context, executed, history, step).await?;
            }
        }

        Ok(())
    }

    /// Gets the input text for a node from its incoming edges.
    async fn get_input_for_node(&self, node_id: &str, context: &Arc<RwLock<HashMap<String, String>>>) -> String {
        let ctx = context.read().await;

        for edge in &self.config.edges {
            if !edge.to.as_vec().contains(&node_id) {
                continue;
            }

            let inputs: Vec<String> = edge.from.as_vec()
                .iter()
                .filter_map(|id| ctx.get(*id).cloned())
                .collect();

            if !inputs.is_empty() {
                return inputs.join("\n\n---\n\n");
            }
        }

        ctx.get("input").cloned().unwrap_or_default()
    }
}

/// Executes a single node and returns its output.
async fn execute_node(
    node_id: &str,
    node_type: NodeType,
    model: &ModelConfig,
    prompt: Option<&str>,
    input: &str,
    step: usize,
) -> Result<NodeOutput, AgentError> {
    info!("╠──────────────────────────────────────────────────────────────");
    info!("║ [{}] NODE: {} ({:?})", step, node_id, node_type);
    info!("║     Model: {}", model.name);
    debug!("║     Input: {}...", input.chars().take(100).collect::<String>());

    let start = std::time::Instant::now();
    info!("║     → {}", node_type.action_label());

    let content = if node_type.requires_llm() {
        let client = LlmClient::new(&model.model, model.api_base.as_deref());
        let response = client.chat(prompt.unwrap_or(""), input).await?;
        info!("║     ← Response: {} chars", response.content.len());
        response.content
    } else {
        input.to_string()
    };

    info!("║     ✓ Completed in {:?}", start.elapsed());

    Ok(NodeOutput { content, next_nodes: vec![] })
}
