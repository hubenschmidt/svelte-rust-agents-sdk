//! SSE-based chat streaming handler.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use fissio_config::{EdgeConfig, EdgeEndpoint, EdgeType, NodeConfig, NodeType, PipelineConfig};
use fissio_core::{Message as CoreMessage, ModelConfig};
use fissio_engine::{EngineOutput, PipelineEngine};
use fissio_llm::{LlmStream, OllamaClient, OllamaMetrics, StreamChunk, UnifiedLlmClient};
use futures::stream::Stream;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{error, info};

use crate::dto::{RuntimePipelineConfig, WsMetadata};
use crate::ServerState;

/// Request body for chat endpoint.
#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub message: String,
    #[serde(default)]
    pub model_id: Option<String>,
    #[serde(default)]
    pub pipeline_id: Option<String>,
    #[serde(default)]
    pub node_models: HashMap<String, String>,
    #[serde(default)]
    pub verbose: bool,
    #[serde(default)]
    pub history: Vec<CoreMessage>,
    #[serde(default)]
    pub pipeline_config: Option<RuntimePipelineConfig>,
    #[serde(default)]
    pub system_prompt: Option<String>,
}

/// SSE event data types.
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum SseData {
    #[serde(rename = "stream")]
    Stream { content: String },
    #[serde(rename = "end")]
    End { metadata: WsMetadata },
    #[serde(rename = "error")]
    Error { message: String },
}

const DEFAULT_SYSTEM_PROMPT: &str = "You are a helpful assistant.";

/// Converts a runtime config from the frontend to a PipelineConfig.
fn runtime_to_pipeline_config(runtime: &RuntimePipelineConfig) -> PipelineConfig {
    let nodes = runtime.nodes.iter().map(|n| NodeConfig {
        id: n.id.clone(),
        node_type: n.node_type.parse().unwrap_or(NodeType::Llm),
        model: n.model.clone(),
        config: serde_json::Value::Null,
        prompt: n.prompt.clone(),
        tools: n.tools.clone().unwrap_or_default(),
    }).collect();

    let edges = runtime.edges.iter().map(|e| EdgeConfig {
        from: json_to_endpoint(&e.from),
        to: json_to_endpoint(&e.to),
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

fn json_to_endpoint(val: &serde_json::Value) -> EdgeEndpoint {
    match val {
        serde_json::Value::String(s) => EdgeEndpoint::Single(s.clone()),
        serde_json::Value::Array(arr) => {
            let strings: Vec<String> = arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
            EdgeEndpoint::Multiple(strings)
        }
        _ => EdgeEndpoint::Single(String::new()),
    }
}

/// SSE chat streaming endpoint.
pub async fn chat(
    State(state): State<Arc<ServerState>>,
    Json(req): Json<ChatRequest>,
) -> Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>> {
    let model_id = req.model_id.as_deref().unwrap_or("");
    let model = state.get_model(model_id);

    info!(
        "Chat request (model: {}): {}...",
        model.name,
        req.message.get(..50).unwrap_or(&req.message)
    );

    let (tx, rx) = mpsc::channel::<Result<Event, std::convert::Infallible>>(100);

    tokio::spawn(async move {
        let start = Instant::now();
        let result = stream_chat(&tx, &req, &model, &state).await;
        let metadata = build_metadata(&result, start.elapsed().as_millis() as u64);

        let end_data = SseData::End { metadata };
        let _ = tx.send(Ok(Event::default()
            .event("end")
            .json_data(&end_data)
            .unwrap())).await;
    });

    Sse::new(ReceiverStream::new(rx)).keep_alive(KeepAlive::default())
}

type EventSender = mpsc::Sender<Result<Event, std::convert::Infallible>>;

struct StreamResult {
    input_tokens: u32,
    output_tokens: u32,
    ollama_metrics: Option<OllamaMetrics>,
}

async fn stream_chat(
    tx: &EventSender,
    req: &ChatRequest,
    model: &ModelConfig,
    state: &ServerState,
) -> StreamResult {
    let system_prompt = req.system_prompt.as_deref().unwrap_or(DEFAULT_SYSTEM_PROMPT);

    // Verbose mode with Ollama native API
    if req.verbose && model.api_base.is_some() {
        return stream_ollama(tx, model, &req.history, &req.message, system_prompt).await;
    }

    // Runtime pipeline config from frontend
    if let Some(ref runtime_config) = req.pipeline_config {
        let config = runtime_to_pipeline_config(runtime_config);
        info!("Using runtime pipeline config ({} nodes)", config.nodes.len());
        return stream_engine(tx, &config, &req.message, &req.history, &state.models, model, req.node_models.clone()).await;
    }

    // Preset pipeline by ID
    if let Some(config) = req.pipeline_id.as_deref().and_then(|id| state.presets.get(id)) {
        info!("Using pipeline preset: {}", config.name);
        return stream_engine(tx, config, &req.message, &req.history, &state.models, model, req.node_models.clone()).await;
    }

    // Direct chat
    stream_direct_chat(tx, model, &req.history, &req.message, system_prompt).await
}

async fn send_chunk(tx: &EventSender, content: &str) {
    let data = SseData::Stream { content: content.to_string() };
    let _ = tx.send(Ok(Event::default()
        .event("stream")
        .json_data(&data)
        .unwrap())).await;
}

async fn stream_ollama(
    tx: &EventSender,
    model: &ModelConfig,
    history: &[CoreMessage],
    message: &str,
    system_prompt: &str,
) -> StreamResult {
    let api_base = model.api_base.as_ref().expect("ollama requires api_base");
    let client = OllamaClient::new(&model.model, api_base);
    info!("Using native Ollama API for verbose metrics");

    match client.chat_stream_with_metrics(system_prompt, history, message).await {
        Ok((stream, metrics_collector)) => {
            let (input_tokens, output_tokens) = consume_stream(tx, stream).await;
            StreamResult {
                input_tokens,
                output_tokens,
                ollama_metrics: Some(metrics_collector.get_metrics()),
            }
        }
        Err(e) => {
            error!("Ollama error: {}", e);
            send_chunk(tx, "Error generating response.").await;
            StreamResult { input_tokens: 0, output_tokens: 0, ollama_metrics: None }
        }
    }
}

async fn stream_direct_chat(
    tx: &EventSender,
    model: &ModelConfig,
    history: &[CoreMessage],
    message: &str,
    system_prompt: &str,
) -> StreamResult {
    let client = UnifiedLlmClient::new(&model.model, model.api_base.as_deref());

    match client.chat_stream(system_prompt, history, message).await {
        Ok(stream) => {
            let (input_tokens, output_tokens) = consume_stream(tx, stream).await;
            StreamResult { input_tokens, output_tokens, ollama_metrics: None }
        }
        Err(e) => {
            error!("Chat error: {}", e);
            send_chunk(tx, "Error generating response.").await;
            StreamResult { input_tokens: 0, output_tokens: 0, ollama_metrics: None }
        }
    }
}

async fn stream_engine(
    tx: &EventSender,
    config: &PipelineConfig,
    message: &str,
    history: &[CoreMessage],
    models: &[ModelConfig],
    default_model: &ModelConfig,
    node_overrides: HashMap<String, String>,
) -> StreamResult {
    let engine = PipelineEngine::new(
        config.clone(),
        models.to_vec(),
        default_model.clone(),
        node_overrides,
    );

    match engine.execute_stream(message, history).await {
        Ok(EngineOutput::Stream(stream)) => {
            let (input_tokens, output_tokens) = consume_stream(tx, stream).await;
            StreamResult { input_tokens, output_tokens, ollama_metrics: None }
        }
        Ok(EngineOutput::Complete(response)) => {
            send_chunk(tx, &response).await;
            StreamResult { input_tokens: 0, output_tokens: 0, ollama_metrics: None }
        }
        Err(e) => {
            error!("Engine error: {}", e);
            send_chunk(tx, "Error generating response.").await;
            StreamResult { input_tokens: 0, output_tokens: 0, ollama_metrics: None }
        }
    }
}

async fn consume_stream(tx: &EventSender, mut stream: LlmStream) -> (u32, u32) {
    let mut input_tokens = 0u32;
    let mut output_tokens = 0u32;

    while let Some(chunk_result) = stream.next().await {
        match chunk_result {
            Ok(StreamChunk::Content(chunk)) => {
                send_chunk(tx, &chunk).await;
            }
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

fn build_metadata(result: &StreamResult, elapsed_ms: u64) -> WsMetadata {
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
