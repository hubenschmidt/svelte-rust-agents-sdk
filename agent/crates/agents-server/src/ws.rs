use std::sync::Arc;
use std::time::Instant;

use agents_core::MessageRole;
use agents_llm::StreamChunk;
use agents_pipeline::StreamResponse;
use axum::{
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt};
use tracing::{error, info};

use crate::protocol::{WsMetadata, WsPayload, WsResponse};
use crate::state::AppState;

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();
    let mut uuid = String::new();

    while let Some(Ok(msg)) = receiver.next().await {
        let Message::Text(text) = msg else { continue };

        let payload: WsPayload = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(e) => {
                error!("JSON parse error: {}", e);
                continue;
            }
        };

        if payload.init {
            uuid = payload.uuid.unwrap_or_else(|| "anonymous".to_string());
            info!("Connection initialized: {}", uuid);
            continue;
        }

        let Some(message) = payload.message else { continue };

        info!("Message from {}: {}...", uuid, &message[..message.len().min(50)]);

        let history = state.get_conversation(&uuid);
        state.add_message(&uuid, MessageRole::User, &message);

        let start = Instant::now();
        let stream_result = state.pipeline.process_stream(&message, &history).await;

        let mut input_tokens = 0u32;
        let mut output_tokens = 0u32;

        let full_response = match stream_result {
            Ok(StreamResponse::Stream(mut stream)) => {
                let mut accumulated = String::new();
                while let Some(chunk_result) = stream.next().await {
                    match chunk_result {
                        Ok(StreamChunk::Content(chunk)) => {
                            accumulated.push_str(&chunk);
                            let msg = serde_json::to_string(&WsResponse::stream(&chunk)).expect("serialize");
                            if sender.send(Message::Text(msg.into())).await.is_err() {
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
                accumulated
            }
            Ok(StreamResponse::Complete(response)) => {
                let msg = serde_json::to_string(&WsResponse::stream(&response)).expect("serialize");
                if sender.send(Message::Text(msg.into())).await.is_err() {
                    continue;
                }
                response
            }
            Err(e) => {
                error!("Pipeline error: {}", e);
                let error_msg = "Sorry—there was an error generating the response.";
                let msg = serde_json::to_string(&WsResponse::stream(error_msg)).expect("serialize");
                let _ = sender.send(Message::Text(msg.into())).await;
                error_msg.to_string()
            }
        };

        let elapsed_ms = start.elapsed().as_millis() as u64;

        state.add_message(&uuid, MessageRole::Assistant, &full_response);

        let metadata = WsMetadata {
            input_tokens,
            output_tokens,
            elapsed_ms,
        };
        info!("Sending metadata: {:?}", metadata);
        let end_msg = serde_json::to_string(&WsResponse::end_with_metadata(metadata)).expect("serialize");
        info!("End message: {}", end_msg);
        if sender.send(Message::Text(end_msg.into())).await.is_err() {
            break;
        }
    }

    info!("Connection closed: {}", uuid);
}
