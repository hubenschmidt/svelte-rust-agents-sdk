//! SSE-based chat streaming handler.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use fissio_core::Message as CoreMessage;
use fissio_engine::EngineOutput;
use fissio_monitor::{MetricsCollector, NodeMetrics, TracingCollector};
use futures::stream::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{error, info};

use crate::dto::{RuntimePipelineConfig, WsMetadata};
use crate::services::chat::{
    build_metadata, execute_direct_chat, execute_ollama_stream,
    execute_pipeline, runtime_to_pipeline_config, PipelineResult, StreamResult,
};
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
}

const DEFAULT_SYSTEM_PROMPT: &str = "You are a helpful assistant.";

type EventSender = mpsc::Sender<Result<Event, std::convert::Infallible>>;

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
        let result = execute_chat(&tx, &req, &state).await;
        let metadata = build_metadata(&result, start.elapsed().as_millis() as u64);

        let end_data = SseData::End { metadata };
        if let Ok(event) = Event::default().event("end").json_data(&end_data) {
            let _ = tx.send(Ok(event)).await;
        }
    });

    Sse::new(ReceiverStream::new(rx)).keep_alive(KeepAlive::default())
}

async fn send_chunk(tx: &EventSender, content: &str) {
    let data = SseData::Stream { content: content.to_string() };
    if let Ok(event) = Event::default().event("stream").json_data(&data) {
        let _ = tx.send(Ok(event)).await;
    }
}

/// Consumes a stream and sends chunks to the SSE channel.
/// Returns (full_response, input_tokens, output_tokens).
async fn stream_to_sse_with_response(tx: &EventSender, stream: fissio_llm::LlmStream) -> (String, u32, u32) {
    let mut full_response = String::new();
    let mut input_tokens = 0u32;
    let mut output_tokens = 0u32;
    let mut stream = stream;

    while let Some(chunk_result) = stream.next().await {
        match chunk_result {
            Ok(fissio_llm::StreamChunk::Content(chunk)) => {
                full_response.push_str(&chunk);
                send_chunk(tx, &chunk).await;
            }
            Ok(fissio_llm::StreamChunk::Usage { input_tokens: i, output_tokens: o }) => {
                input_tokens = i;
                output_tokens = o;
            }
            Err(e) => {
                error!("Stream error: {}", e);
            }
        }
    }

    (full_response, input_tokens, output_tokens)
}

async fn execute_chat(tx: &EventSender, req: &ChatRequest, state: &ServerState) -> StreamResult {
    let model_id = req.model_id.as_deref().unwrap_or("");
    let model = state.get_model(model_id);
    let system_prompt = req.system_prompt.as_deref().unwrap_or(DEFAULT_SYSTEM_PROMPT);

    // Use native Ollama API for local models (provides rich metrics)
    if model.api_base.is_some() {
        return execute_ollama_chat(tx, &model, &req.history, &req.message, system_prompt, state).await;
    }

    // Runtime pipeline config from frontend
    if let Some(ref runtime_config) = req.pipeline_config {
        let config = runtime_to_pipeline_config(runtime_config);
        info!("Using runtime pipeline config ({} nodes)", config.nodes.len());
        return execute_pipeline_chat(tx, &config, &req.message, &req.history, state, &model, req.node_models.clone()).await;
    }

    // Preset pipeline by ID
    if let Some(config) = req.pipeline_id.as_deref().and_then(|id| state.presets.get(id)) {
        info!("Using pipeline preset: {}", config.name);
        return execute_pipeline_chat(tx, config, &req.message, &req.history, state, &model, req.node_models.clone()).await;
    }

    // Direct chat
    execute_direct(tx, &model, &req.history, &req.message, system_prompt, state).await
}

async fn execute_ollama_chat(
    tx: &EventSender,
    model: &fissio_core::ModelConfig,
    history: &[CoreMessage],
    message: &str,
    system_prompt: &str,
    state: &ServerState,
) -> StreamResult {
    let collector = TracingCollector::new(
        state.trace_store.clone(),
        "direct",
        format!("Direct Chat ({})", model.name),
        message,
    );
    let start_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    match execute_ollama_stream(model, history, message, system_prompt).await {
        Ok((stream, metrics)) => {
            let (response, input_tokens, output_tokens) = stream_to_sse_with_response(tx, stream).await;
            let end_time = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);

            let node_metrics = NodeMetrics {
                node_id: "llm".to_string(),
                input_tokens,
                output_tokens,
                elapsed_ms: (end_time - start_time) as u64,
                tool_call_count: 0,
                iteration_count: 1,
                estimated_cost_usd: None,
            };
            collector.record(node_metrics.clone());
            collector.record_span("llm", "llm", start_time, end_time, message, &response, &node_metrics);
            collector.success(&response);

            info!("Direct chat: {}ms, tokens: {}/{}", end_time - start_time, input_tokens, output_tokens);
            StreamResult { input_tokens, output_tokens, ollama_metrics: Some(metrics) }
        }
        Err(e) => {
            error!("Ollama error: {}", e);
            collector.error(&e.to_string());
            send_chunk(tx, "Error generating response.").await;
            StreamResult { input_tokens: 0, output_tokens: 0, ollama_metrics: None }
        }
    }
}

async fn execute_direct(
    tx: &EventSender,
    model: &fissio_core::ModelConfig,
    history: &[CoreMessage],
    message: &str,
    system_prompt: &str,
    state: &ServerState,
) -> StreamResult {
    let collector = TracingCollector::new(
        state.trace_store.clone(),
        "direct",
        format!("Direct Chat ({})", model.name),
        message,
    );
    let start_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    match execute_direct_chat(model, history, message, system_prompt).await {
        Ok(stream) => {
            let (response, input_tokens, output_tokens) = stream_to_sse_with_response(tx, stream).await;
            let end_time = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);

            let node_metrics = NodeMetrics {
                node_id: "llm".to_string(),
                input_tokens,
                output_tokens,
                elapsed_ms: (end_time - start_time) as u64,
                tool_call_count: 0,
                iteration_count: 1,
                estimated_cost_usd: None,
            };
            collector.record(node_metrics.clone());
            collector.record_span("llm", "llm", start_time, end_time, message, &response, &node_metrics);
            collector.success(&response);

            info!("Direct chat: {}ms, tokens: {}/{}", end_time - start_time, input_tokens, output_tokens);
            StreamResult { input_tokens, output_tokens, ollama_metrics: None }
        }
        Err(e) => {
            error!("Chat error: {}", e);
            collector.error(&e.to_string());
            send_chunk(tx, "Error generating response.").await;
            StreamResult { input_tokens: 0, output_tokens: 0, ollama_metrics: None }
        }
    }
}

async fn execute_pipeline_chat(
    tx: &EventSender,
    config: &fissio_config::PipelineConfig,
    message: &str,
    history: &[CoreMessage],
    state: &ServerState,
    default_model: &fissio_core::ModelConfig,
    node_overrides: HashMap<String, String>,
) -> StreamResult {
    let trace_store = Some(state.trace_store.clone());

    match execute_pipeline(config, message, history, &state.models, default_model, node_overrides, trace_store).await {
        Ok(PipelineResult { output: EngineOutput::Stream(stream), collector }) => {
            let (response, input_tokens, output_tokens) = stream_to_sse_with_response(tx, stream).await;
            if let Some(coll) = collector {
                coll.success(&response);
            }
            StreamResult { input_tokens, output_tokens, ollama_metrics: None }
        }
        Ok(PipelineResult { output: EngineOutput::Complete(response), collector }) => {
            send_chunk(tx, &response).await;
            if let Some(coll) = collector {
                coll.success(&response);
            }
            StreamResult { input_tokens: 0, output_tokens: 0, ollama_metrics: None }
        }
        Err(e) => {
            error!("Engine error: {}", e);
            send_chunk(tx, "Error generating response.").await;
            StreamResult { input_tokens: 0, output_tokens: 0, ollama_metrics: None }
        }
    }
}
