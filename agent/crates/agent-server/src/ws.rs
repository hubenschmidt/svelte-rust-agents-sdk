//! WebSocket handler for real-time LLM streaming and pipeline execution.
//!
//! Handles client connections, message routing, model management commands,
//! and streaming responses back to the frontend.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use agent_config::{EdgeConfig, EdgeEndpoint, EdgeType, NodeConfig, NodeType, PipelineConfig};
use agent_core::{Message as CoreMessage, ModelConfig};
use agent_engine::{EngineOutput, PipelineEngine};
use agent_network::{LlmClient, LlmStream, OllamaClient, OllamaMetrics, StreamChunk};
use axum::{
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    response::IntoResponse,
};
use futures::{stream::SplitSink, SinkExt, StreamExt};
use serde::Serialize;
use tracing::{error, info};

use crate::dto::{InitResponse, RuntimePipelineConfig, WsMetadata, WsPayload, WsResponse};
use crate::services::model;
use crate::ServerState;

/// Result of processing an LLM stream.
struct StreamResult {
    input_tokens: u32,
    output_tokens: u32,
    ollama_metrics: Option<OllamaMetrics>,
}

/// Converts a runtime config from the frontend to a PipelineConfig.
fn runtime_to_pipeline_config(runtime: &RuntimePipelineConfig) -> PipelineConfig {
    let nodes = runtime.nodes.iter().map(|n| NodeConfig {
        id: n.id.clone(),
        node_type: n.node_type.parse().unwrap_or(NodeType::Llm),
        model: n.model.clone(),
        config: serde_json::Value::Null,
        prompt: n.prompt.clone(),
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

/// Converts a JSON value to an EdgeEndpoint.
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

/// Sends a JSON-serialized message over the WebSocket.
async fn send_json<T: Serialize>(sender: &mut SplitSink<WebSocket, Message>, data: &T) -> bool {
    let Ok(json) = serde_json::to_string(data) else {
        error!("JSON serialization failed");
        return false;
    };
    sender.send(Message::Text(json.into())).await.is_ok()
}

/// Consumes an LLM stream, forwarding chunks to the client.
async fn consume_stream(
    sender: &mut SplitSink<WebSocket, Message>,
    mut stream: LlmStream,
) -> (String, u32, u32) {
    let mut accumulated = String::new();
    let mut input_tokens = 0u32;
    let mut output_tokens = 0u32;

    while let Some(chunk_result) = stream.next().await {
        match chunk_result {
            Ok(StreamChunk::Content(chunk)) => {
                accumulated.push_str(&chunk);
                if !send_json(sender, &WsResponse::stream(&chunk)).await {
                    break;
                }
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
    (accumulated, input_tokens, output_tokens)
}

/// Sends an error message to the client.
async fn send_error(sender: &mut SplitSink<WebSocket, Message>) -> String {
    let error_msg = "Sorry—there was an error generating the response.";
    let _ = send_json(sender, &WsResponse::stream(error_msg)).await;
    error_msg.to_string()
}

/// Processes a request using Ollama's native API for verbose metrics.
async fn process_ollama(
    sender: &mut SplitSink<WebSocket, Message>,
    model: &ModelConfig,
    history: &[CoreMessage],
    message: &str,
) -> StreamResult {
    let api_base = model.api_base.as_ref().expect("ollama requires api_base");
    let client = OllamaClient::new(&model.model, api_base);
    info!("Using native Ollama API for verbose metrics");

    let result = client
        .chat_stream_with_metrics("You are a helpful assistant.", history, message)
        .await;

    match result {
        Ok((stream, metrics_collector)) => {
            let (_content, input_tokens, output_tokens) = consume_stream(sender, Box::pin(stream)).await;
            StreamResult {
                input_tokens,
                output_tokens,
                ollama_metrics: Some(metrics_collector.get_metrics()),
            }
        }
        Err(e) => {
            error!("Ollama error: {}", e);
            send_error(sender).await;
            StreamResult { input_tokens: 0, output_tokens: 0, ollama_metrics: None }
        }
    }
}

/// Processes a direct chat request (no pipeline).
async fn process_direct_chat(
    sender: &mut SplitSink<WebSocket, Message>,
    model: &ModelConfig,
    history: &[CoreMessage],
    message: &str,
) -> StreamResult {
    let client = LlmClient::new(&model.model, model.api_base.as_deref());
    let result = client
        .chat_stream("You are a helpful assistant.", history, message)
        .await;

    match result {
        Ok(stream) => {
            let (_content, input_tokens, output_tokens) = consume_stream(sender, stream).await;
            StreamResult { input_tokens, output_tokens, ollama_metrics: None }
        }
        Err(e) => {
            error!("Direct chat error: {}", e);
            send_error(sender).await;
            StreamResult { input_tokens: 0, output_tokens: 0, ollama_metrics: None }
        }
    }
}

/// Processes a request through the pipeline engine.
async fn process_engine(
    sender: &mut SplitSink<WebSocket, Message>,
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

    let result = engine.execute_stream(message, history).await;

    match result {
        Ok(EngineOutput::Stream(stream)) => {
            let (_content, input_tokens, output_tokens) = consume_stream(sender, stream).await;
            StreamResult { input_tokens, output_tokens, ollama_metrics: None }
        }
        Ok(EngineOutput::Complete(response)) => {
            let _ = send_json(sender, &WsResponse::stream(&response)).await;
            StreamResult { input_tokens: 0, output_tokens: 0, ollama_metrics: None }
        }
        Err(e) => {
            error!("Engine error: {}", e);
            send_error(sender).await;
            StreamResult { input_tokens: 0, output_tokens: 0, ollama_metrics: None }
        }
    }
}

/// WebSocket upgrade handler.
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<ServerState>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

/// Main WebSocket connection handler.
async fn handle_socket(socket: WebSocket, state: Arc<ServerState>) {
    let (mut sender, mut receiver) = socket.split();

    // Wait for init message
    let uuid = loop {
        let Some(Ok(msg)) = receiver.next().await else { return };
        let Message::Text(text) = msg else { continue };

        let payload: WsPayload = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(e) => {
                error!("JSON parse error: {}", e);
                continue;
            }
        };

        if !payload.init {
            continue;
        }

        let uuid = payload.uuid.unwrap_or_else(|| "anonymous".to_string());
        info!("Connection initialized: {}", uuid);

        let init_resp = InitResponse {
            models: state.models.clone(),
            templates: state.templates.clone(),
            configs: state.configs.read().await.clone(),
        };
        if !send_json(&mut sender, &init_resp).await {
            return;
        }
        break uuid;
    };

    // Process messages
    while let Some(result) = receiver.next().await {
        let msg = match result {
            Ok(m) => m,
            Err(e) => {
                error!("WS receive error for {}: {}", uuid, e);
                break;
            }
        };
        let Message::Text(text) = msg else {
            info!("WS non-text message for {}: {:?}", uuid, msg);
            continue;
        };

        let payload: WsPayload = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(e) => {
                error!("JSON parse error: {}", e);
                continue;
            }
        };

        // Handle model wake request
        if let Some(wake_model_id) = &payload.wake_model_id {
            if !handle_wake(&mut sender, &state, wake_model_id, payload.unload_model_id.as_deref()).await {
                break;
            }
            continue;
        }

        // Handle model unload request
        if let Some(unload_model_id) = &payload.unload_model_id {
            if !handle_unload(&mut sender, &state, unload_model_id).await {
                break;
            }
            continue;
        }

        // Handle chat message
        let Some(message) = payload.message else { continue };

        let model_id = payload.model_id.as_deref().unwrap_or("");
        let model = state.get_model(model_id);

        info!(
            "Message from {} (model: {}): {}...",
            uuid,
            model.name,
            message.get(..50).unwrap_or(&message)
        );

        let start = Instant::now();
        let use_ollama_native = payload.verbose && model.api_base.is_some();

        let result = if use_ollama_native {
            process_ollama(&mut sender, &model, &payload.history, &message).await
        } else if let Some(ref runtime_config) = payload.pipeline_config {
            let config = runtime_to_pipeline_config(runtime_config);
            info!("Using runtime pipeline config ({} nodes)", config.nodes.len());
            process_engine(&mut sender, &config, &message, &payload.history, &state.models, &model, payload.node_models).await
        } else if let Some(config) = payload.pipeline_id.as_deref().and_then(|id| state.presets.get(id)) {
            info!("Using pipeline preset: {}", config.name);
            process_engine(&mut sender, config, &message, &payload.history, &state.models, &model, payload.node_models).await
        } else {
            process_direct_chat(&mut sender, &model, &payload.history, &message).await
        };

        let metadata = build_metadata(&result, start.elapsed().as_millis() as u64);
        info!("Sending metadata: {:?}", metadata);

        if !send_json(&mut sender, &WsResponse::end_with_metadata(metadata)).await {
            break;
        }
    }

    info!("Connection closed: {}", uuid);
}

/// Handles a model wake request.
async fn handle_wake(
    sender: &mut SplitSink<WebSocket, Message>,
    state: &ServerState,
    model_id: &str,
    prev_model_id: Option<&str>,
) -> bool {
    if !send_json(sender, &WsResponse::model_status("loading")).await {
        return false;
    }
    match model::warmup(state, model_id, prev_model_id).await {
        Ok(m) => info!("Model {} ready via WebSocket", m.name),
        Err(e) => error!("Wake failed: {:?}", e),
    }
    send_json(sender, &WsResponse::model_status("ready")).await
}

/// Handles a model unload request.
async fn handle_unload(
    sender: &mut SplitSink<WebSocket, Message>,
    state: &ServerState,
    model_id: &str,
) -> bool {
    if !send_json(sender, &WsResponse::model_status("unloading")).await {
        return false;
    }
    if let Err(e) = model::unload(state, model_id).await {
        error!("Unload failed: {:?}", e);
    }
    send_json(sender, &WsResponse::model_status("ready")).await
}

/// Builds response metadata from stream result.
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
