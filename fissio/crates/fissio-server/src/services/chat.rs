//! Chat execution service - business logic for chat streaming.

use std::collections::HashMap;

use fissio_config::{EdgeConfig, EdgeEndpoint, EdgeType, NodeConfig, NodeType, PipelineConfig};
use fissio_core::{Message as CoreMessage, ModelConfig};
use fissio_engine::{EngineOutput, PipelineEngine};
use fissio_llm::{LlmStream, OllamaClient, OllamaMetrics, StreamChunk, UnifiedLlmClient};
use futures::StreamExt;
use tracing::{error, info};

use crate::dto::{RuntimePipelineConfig, WsMetadata};

/// Result of a streaming chat operation.
pub struct StreamResult {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub ollama_metrics: Option<OllamaMetrics>,
}

/// Converts a runtime config from the frontend to a PipelineConfig.
pub fn runtime_to_pipeline_config(runtime: &RuntimePipelineConfig) -> PipelineConfig {
    let nodes = runtime.nodes.iter().map(|n| NodeConfig {
        id: n.id.clone(),
        node_type: n.node_type.parse().unwrap_or(NodeType::Llm),
        model: n.model.clone(),
        config: serde_json::Value::Null,
        prompt: n.prompt.clone(),
        tools: n.tools.clone().unwrap_or_default(),
    }).collect();

    let edges = runtime.edges.iter().map(|e| EdgeConfig {
        from: EdgeEndpoint::from(&e.from),
        to: EdgeEndpoint::from(&e.to),
        edge_type: e.edge_type.as_deref()
            .and_then(|t| t.parse().ok())
            .unwrap_or(EdgeType::Direct),
    }).collect();

    PipelineConfig {
        id: "runtime".to_string(),
        name: "Runtime Config".to_string(),
        description: String::new(),
        nodes,
        edges,
    }
}

/// Executes a streaming chat with Ollama native API (for verbose metrics).
pub async fn execute_ollama_stream(
    model: &ModelConfig,
    history: &[CoreMessage],
    message: &str,
    system_prompt: &str,
) -> Result<(LlmStream, OllamaMetrics), String> {
    let api_base = model.api_base.as_ref().ok_or("ollama requires api_base")?;
    let client = OllamaClient::new(&model.model, api_base);
    info!("Using native Ollama API for verbose metrics");

    let (stream, metrics_collector) = client
        .chat_stream_with_metrics(system_prompt, history, message)
        .await
        .map_err(|e| e.to_string())?;

    Ok((stream, metrics_collector.get_metrics()))
}

/// Executes a direct chat without pipeline.
pub async fn execute_direct_chat(
    model: &ModelConfig,
    history: &[CoreMessage],
    message: &str,
    system_prompt: &str,
) -> Result<LlmStream, String> {
    let client = UnifiedLlmClient::new(&model.model, model.api_base.as_deref());
    client
        .chat_stream(system_prompt, history, message)
        .await
        .map_err(|e| e.to_string())
}

/// Executes a pipeline and returns the output stream.
pub async fn execute_pipeline(
    config: &PipelineConfig,
    message: &str,
    history: &[CoreMessage],
    models: &[ModelConfig],
    default_model: &ModelConfig,
    node_overrides: HashMap<String, String>,
) -> Result<EngineOutput, String> {
    let engine = PipelineEngine::new(
        config.clone(),
        models.to_vec(),
        default_model.clone(),
        node_overrides,
    );

    engine
        .execute_stream(message, history)
        .await
        .map_err(|e| e.to_string())
}

/// Consumes an LLM stream, calling the sender for each content chunk.
/// Returns token counts.
pub async fn consume_stream<F>(mut stream: LlmStream, on_chunk: F) -> (u32, u32)
where
    F: Fn(&str),
{
    let mut input_tokens = 0u32;
    let mut output_tokens = 0u32;

    while let Some(chunk_result) = stream.next().await {
        match chunk_result {
            Ok(StreamChunk::Content(chunk)) => on_chunk(&chunk),
            Ok(StreamChunk::Usage { input_tokens: i, output_tokens: o }) => {
                input_tokens = i;
                output_tokens = o;
            }
            Err(e) => {
                error!("Stream error: {}", e);
                break;
            }
        }
    }

    (input_tokens, output_tokens)
}

/// Builds metadata from stream result.
pub fn build_metadata(result: &StreamResult, elapsed_ms: u64) -> WsMetadata {
    match &result.ollama_metrics {
        Some(m) => {
            info!(
                "Ollama metrics: {:.1} tok/s, {} tokens, {}ms total",
                m.tokens_per_sec(),
                m.eval_count,
                m.total_duration_ms()
            );
            WsMetadata {
                input_tokens: m.prompt_eval_count,
                output_tokens: m.eval_count,
                elapsed_ms,
                load_duration_ms: Some(m.load_duration_ms()),
                prompt_eval_ms: Some(m.prompt_eval_ms()),
                eval_ms: Some(m.eval_ms()),
                tokens_per_sec: Some(m.tokens_per_sec()),
            }
        }
        None => WsMetadata {
            input_tokens: result.input_tokens,
            output_tokens: result.output_tokens,
            elapsed_ms,
            ..Default::default()
        },
    }
}
