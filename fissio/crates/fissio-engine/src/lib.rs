//! Pipeline execution engine for fissio.
//!
//! This crate provides the core DAG execution engine for fissio pipelines:
//!
//! - [`PipelineEngine`] — Executes pipeline configurations
//! - [`ModelResolver`] — Resolves model IDs to configurations
//! - [`EngineOutput`] — Stream or complete response from execution
//! - [`NodeInput`] / [`NodeOutput`] — Data flowing through nodes
//!
//! # Quick Start
//!
//! ```rust,ignore
//! use fissio_engine::PipelineEngine;
//! use fissio_config::PipelineConfig;
//! use fissio_core::ModelConfig;
//!
//! // Load config and create engine
//! let engine = PipelineEngine::new(
//!     config,
//!     vec![model1, model2],
//!     default_model,
//!     HashMap::new(), // node model overrides
//! );
//!
//! // Execute pipeline
//! let result = engine.execute_stream("Hello!", &[]).await?;
//! match result {
//!     EngineOutput::Stream(stream) => { /* consume stream */ }
//!     EngineOutput::Complete(text) => println!("{}", text),
//! }
//! ```
//!
//! # Execution Model
//!
//! The engine traverses the pipeline DAG starting from `input` edges:
//!
//! 1. **Sequential** (Direct edges) — Nodes execute one after another
//! 2. **Parallel** (Parallel edges) — Nodes execute concurrently via `tokio::join_all`
//! 3. **Conditional** (Router nodes) — LLM classifies input to choose path
//!
//! # Agentic Tool Loops
//!
//! Worker nodes with tools configured run an agentic loop:
//! 1. Send message + tool schemas to LLM
//! 2. If LLM returns tool calls, execute them
//! 3. Send results back to LLM
//! 4. Repeat until LLM returns final content (max 10 iterations)

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use fissio_config::{EdgeConfig, EdgeEndpoint, EdgeType, NodeConfig, NodeType, PipelineConfig};
use fissio_core::{AgentError, ModelConfig};
use fissio_llm::{ChatResponse, LlmStream, ToolCall, ToolSchema, UnifiedLlmClient};
use fissio_tools::ToolRegistry;
use async_recursion::async_recursion;
use futures::future::join_all;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Input data passed to a node during execution.
///
/// Contains the user's message, conversation history, and accumulated
/// context from previous nodes in the pipeline.
#[derive(Debug, Clone, Default)]
pub struct NodeInput {
    /// The original user input that started pipeline execution.
    pub user_input: String,
    /// Conversation history for multi-turn interactions.
    pub history: Vec<fissio_core::Message>,
    /// Key-value context accumulated from previous nodes.
    pub context: HashMap<String, String>,
}

/// Output produced by a node after execution.
///
/// For most nodes, `next_nodes` is empty. Router nodes populate it
/// with the target node IDs determined by classification.
#[derive(Debug, Clone)]
pub struct NodeOutput {
    /// The content produced by this node.
    pub content: String,
    /// Target nodes for routing (only set by Router nodes).
    pub next_nodes: Vec<String>,
}

/// Result of pipeline execution.
///
/// Depending on pipeline structure, execution may return a stream
/// for real-time output or a complete response string.
pub enum EngineOutput {
    /// Streaming response for real-time output.
    Stream(LlmStream),
    /// Complete response after pipeline finishes.
    Complete(String),
}

/// Resolves model IDs to their configurations.
///
/// Used by the engine to look up model configs for nodes that specify
/// a model ID. Falls back to a default model when no match is found.
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

/// Core pipeline execution engine.
///
/// Executes [`PipelineConfig`] definitions as directed acyclic graphs,
/// handling parallel execution, conditional routing, and agentic tool loops.
///
/// # Example
///
/// ```rust,ignore
/// let engine = PipelineEngine::new(config, models, default_model, HashMap::new());
/// let result = engine.execute_stream("Hello!", &[]).await?;
/// ```
pub struct PipelineEngine {
    config: PipelineConfig,
    resolver: ModelResolver,
    node_overrides: HashMap<String, String>,
    tool_registry: Arc<ToolRegistry>,
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
            tool_registry: Arc::new(ToolRegistry::with_defaults()),
        }
    }

    /// Creates a new engine with a custom tool registry.
    pub fn with_tools(
        config: PipelineConfig,
        models: Vec<ModelConfig>,
        default_model: ModelConfig,
        node_overrides: HashMap<String, String>,
        tool_registry: ToolRegistry,
    ) -> Self {
        Self {
            config,
            resolver: ModelResolver::new(models, default_model),
            node_overrides,
            tool_registry: Arc::new(tool_registry),
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

    /// Gets all target node IDs from outgoing edges (for router decisions).
    fn get_outgoing_targets(&self, node_id: &str) -> Vec<String> {
        self.get_outgoing_edges(node_id)
            .iter()
            .flat_map(|e| e.to.as_vec().into_iter().map(String::from))
            .filter(|t| t != "output")
            .collect()
    }

    /// Executes the pipeline and returns the result.
    pub async fn execute_stream(
        &self,
        user_input: &str,
        history: &[fissio_core::Message],
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
        history: &[fissio_core::Message],
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
        history: &[fissio_core::Message],
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
            let outgoing_targets = self.get_outgoing_targets(id);
            node_data.push((node.id.clone(), node.node_type, model, node.prompt.clone(), node.tools.clone(), input, outgoing_targets));
        }

        // Execute in parallel
        let tool_registry = Arc::clone(&self.tool_registry);
        let futures: Vec<_> = node_data.into_iter()
            .map(|(node_id, node_type, model, prompt, tools, input, outgoing_targets)| {
                let step = Arc::clone(step);
                let registry = Arc::clone(&tool_registry);
                async move {
                    let current_step = {
                        let mut s = step.write().await;
                        *s += 1;
                        *s
                    };
                    let result = execute_node(&node_id, node_type, &model, prompt.as_deref(), &input, &tools, &registry, current_step, &outgoing_targets).await;
                    (node_id, result)
                }
            })
            .collect();

        let results = join_all(futures).await;

        // Store results and track router decisions
        let mut router_decisions: HashMap<String, Vec<String>> = HashMap::new();
        for (node_id, result) in results {
            let output = result?;
            context.write().await.insert(node_id.clone(), output.content);
            if !output.next_nodes.is_empty() {
                router_decisions.insert(node_id.clone(), output.next_nodes);
            }
            executed.insert(node_id);
        }

        info!("║ PARALLEL EXECUTION COMPLETE");
        info!("╠══════════════════════════════════════════════════════════════");

        // Process outgoing edges
        for node_id in target_ids {
            let router_targets = router_decisions.get(node_id).map(|v| v.as_slice()).unwrap_or(&[]);
            self.process_outgoing_edges(node_id, router_targets, context, executed, history, step).await?;
        }

        Ok(())
    }

    /// Executes nodes sequentially.
    async fn execute_sequential(
        &self,
        target_ids: Vec<&str>,
        context: &Arc<RwLock<HashMap<String, String>>>,
        executed: &mut HashSet<String>,
        history: &[fissio_core::Message],
        step: &Arc<RwLock<usize>>,
    ) -> Result<(), AgentError> {
        for node_id in target_ids {
            if executed.contains(node_id) || node_id == "output" {
                continue;
            }

            let Some(node) = self.get_node(node_id) else { continue };
            let input = self.get_input_for_node(node_id, context).await;
            let outgoing_targets = self.get_outgoing_targets(node_id);

            let current_step = {
                let mut s = step.write().await;
                *s += 1;
                *s
            };

            let model = self.get_node_model(node);
            let output = execute_node(node_id, node.node_type, model, node.prompt.as_deref(), &input, &node.tools, &self.tool_registry, current_step, &outgoing_targets).await?;

            context.write().await.insert(node_id.to_string(), output.content.clone());
            executed.insert(node_id.to_string());

            // Process outgoing edges - filter by router decision if applicable
            self.process_outgoing_edges(node_id, &output.next_nodes, context, executed, history, step).await?;
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

    /// Processes outgoing edges for a node, filtering by router decisions if applicable.
    async fn process_outgoing_edges(
        &self,
        node_id: &str,
        router_targets: &[String],
        context: &Arc<RwLock<HashMap<String, String>>>,
        executed: &mut HashSet<String>,
        history: &[fissio_core::Message],
        step: &Arc<RwLock<usize>>,
    ) -> Result<(), AgentError> {
        for next_edge in self.get_outgoing_edges(node_id) {
            let edge_targets = next_edge.to.as_vec();

            // Skip if any target already executed
            if edge_targets.iter().any(|t| executed.contains(*t)) {
                continue;
            }

            // If router returned specific targets, only follow matching edges
            if !router_targets.is_empty() {
                let should_follow = edge_targets.iter().any(|t| router_targets.contains(&t.to_string()));
                if !should_follow {
                    continue;
                }
            }

            self.process_edge(next_edge, context, executed, history, step).await?;
        }
        Ok(())
    }
}

/// Maximum number of tool call iterations to prevent infinite loops.
const MAX_TOOL_ITERATIONS: usize = 10;

/// Executes a single node and returns its output.
/// If the node has tools configured, runs an agentic loop until the LLM produces final output.
/// For Router nodes, executes an LLM call to determine routing and returns the target in next_nodes.
async fn execute_node(
    node_id: &str,
    node_type: NodeType,
    model: &ModelConfig,
    prompt: Option<&str>,
    input: &str,
    tools: &[String],
    tool_registry: &ToolRegistry,
    step: usize,
    outgoing_targets: &[String],
) -> Result<NodeOutput, AgentError> {
    info!("╠──────────────────────────────────────────────────────────────");
    info!("║ [{}] NODE: {} ({:?})", step, node_id, node_type);
    info!("║     Model: {}", model.name);
    if !tools.is_empty() {
        info!("║     Tools: {:?}", tools);
    }
    debug!("║     Input: {}...", input.chars().take(100).collect::<String>());

    let start = std::time::Instant::now();
    info!("║     → {}", node_type.action_label());

    // Router node: execute LLM to classify and determine routing target
    if node_type.is_router() {
        let (content, next_nodes) = execute_router(model, prompt, input, outgoing_targets).await?;
        info!("║     ✓ Completed in {:?}, routed to: {:?}", start.elapsed(), next_nodes);
        return Ok(NodeOutput { content, next_nodes });
    }

    let content = if node_type.requires_llm() {
        execute_node_with_tools(model, prompt, input, tools, tool_registry).await?
    } else {
        input.to_string()
    };

    info!("║     ✓ Completed in {:?}", start.elapsed());

    Ok(NodeOutput { content, next_nodes: vec![] })
}

/// Executes a Router node: LLM classifies input and returns the target node(s).
async fn execute_router(
    model: &ModelConfig,
    prompt: Option<&str>,
    input: &str,
    outgoing_targets: &[String],
) -> Result<(String, Vec<String>), AgentError> {
    let client = UnifiedLlmClient::new(&model.model, model.api_base.as_deref());

    // Build routing prompt
    let targets_list = outgoing_targets.join(", ");
    let routing_prompt = format!(
        "{}\n\nYou are a routing classifier. Based on the input, determine which target to route to.\n\
        Available targets: [{}]\n\n\
        IMPORTANT: Respond with ONLY the target name, nothing else. No explanation, no punctuation.",
        prompt.unwrap_or("Classify the following input and route to the appropriate target."),
        targets_list
    );

    let response = client.chat(&routing_prompt, input).await?;
    let decision = response.content.trim().to_lowercase();

    info!("║     Router decision: '{}'", decision);

    // Match decision to available targets (case-insensitive, exact match only)
    let matched = outgoing_targets
        .iter()
        .find(|t| t.to_lowercase() == decision)
        .cloned();

    // Fall back to first target if no match
    let next_nodes = match matched {
        Some(target) => vec![target],
        None => {
            warn!("║     ⚠ No exact match for '{}' in {:?}, defaulting to first", decision, outgoing_targets);
            outgoing_targets.first().map(|t| vec![t.clone()]).unwrap_or_default()
        }
    };

    Ok((response.content, next_nodes))
}

/// Executes an LLM node, potentially with an agentic tool loop.
async fn execute_node_with_tools(
    model: &ModelConfig,
    prompt: Option<&str>,
    input: &str,
    tools: &[String],
    tool_registry: &ToolRegistry,
) -> Result<String, AgentError> {
    let client = UnifiedLlmClient::new(&model.model, model.api_base.as_deref());
    let system_prompt = prompt.unwrap_or("");

    // No tools configured - simple chat
    if tools.is_empty() {
        let response = client.chat(system_prompt, input).await?;
        info!("║     ← Response: {} chars", response.content.len());
        return Ok(response.content);
    }

    // Get tool schemas for configured tools
    let tool_schemas: Vec<ToolSchema> = tools
        .iter()
        .filter_map(|name| {
            tool_registry.get(name).map(|t| ToolSchema {
                name: t.name().to_string(),
                description: t.description().to_string(),
                parameters: t.parameters(),
            })
        })
        .collect();

    if tool_schemas.is_empty() {
        warn!("║     ⚠ No valid tools found in registry for: {:?}", tools);
        let response = client.chat(system_prompt, input).await?;
        return Ok(response.content);
    }

    info!("║     → Starting agentic loop with {} tools", tool_schemas.len());

    // Agentic loop
    let mut messages = vec![UnifiedLlmClient::user_message(input)?];
    let mut pending_tool_calls: Option<Vec<ToolCall>> = None;
    let mut iterations = 0;

    loop {
        iterations += 1;
        if iterations > MAX_TOOL_ITERATIONS {
            warn!("║     ⚠ Max tool iterations ({}) reached", MAX_TOOL_ITERATIONS);
            return Err(AgentError::LlmError(format!(
                "Max tool iterations ({}) exceeded",
                MAX_TOOL_ITERATIONS
            )));
        }

        let response = client
            .chat_with_tools(
                system_prompt,
                &messages,
                &tool_schemas,
                pending_tool_calls.as_deref(),
            )
            .await?;

        match response {
            ChatResponse::Content(llm_response) => {
                info!(
                    "║     ← Final response: {} chars (after {} iterations)",
                    llm_response.content.len(),
                    iterations
                );
                return Ok(llm_response.content);
            }
            ChatResponse::ToolCalls { calls, metrics: _ } => {
                info!(
                    "║     ← Tool calls: {:?}",
                    calls.iter().map(|c| &c.name).collect::<Vec<_>>()
                );

                for call in &calls {
                    let tool = tool_registry.get(&call.name).ok_or_else(|| {
                        AgentError::LlmError(format!("Tool not found: {}", call.name))
                    })?;

                    info!("║       → Executing tool: {}", call.name);
                    let result = tool.execute(call.arguments.clone()).await.map_err(|e| {
                        AgentError::LlmError(format!("Tool execution failed: {}", e))
                    })?;

                    info!("║       ← Tool result: {} chars", result.len());

                    // Add tool result to messages
                    messages.push(UnifiedLlmClient::tool_result_message(&call.id, &result)?);
                }

                // Store tool calls for next iteration (needed for Anthropic message format)
                pending_tool_calls = Some(calls);
            }
        }
    }
}
