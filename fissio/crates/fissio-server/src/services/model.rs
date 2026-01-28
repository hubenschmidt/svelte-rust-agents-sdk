//! Model warmup and unload service.
//!
//! Handles pre-loading models into GPU memory for faster first responses,
//! and unloading to free memory when switching models.

use fissio_core::ModelConfig;
use fissio_llm::{unload_model, LlmClient};
use futures::StreamExt;
use tracing::info;

use crate::error::AppError;
use crate::ServerState;

/// Warms up a model by running a minimal chat request.
/// Optionally unloads the previous model first (in parallel).
pub async fn warmup(
    state: &ServerState,
    model_id: &str,
    previous_model_id: Option<&str>,
) -> Result<ModelConfig, AppError> {
    let model = state.get_model(model_id);
    info!("Warming up model: {}", model.name);

    let (_, warmup_result) = tokio::join!(
        unload_previous(state, previous_model_id),
        do_warmup(&model)
    );
    warmup_result?;

    info!("Model {} ready", model.name);
    Ok(model)
}

/// Runs a minimal request to load the model into memory.
async fn do_warmup(model: &ModelConfig) -> Result<(), AppError> {
    let client = LlmClient::new(&model.model, model.api_base.as_deref());
    let mut stream = client
        .chat_stream("You are a helpful assistant.", &[], "hi")
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    while stream.next().await.is_some() {}
    Ok(())
}

/// Unloads a model from GPU memory (Ollama only).
pub async fn unload(state: &ServerState, model_id: &str) -> Result<(), AppError> {
    let model = state.get_model(model_id);

    let Some(api_base) = &model.api_base else {
        return Ok(()); // Not a local model
    };

    let ollama_host = api_base.trim_end_matches("/v1");
    info!("Unloading model: {}", model.name);

    unload_model(ollama_host, &model.model)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(())
}

/// Unloads a previous model if specified (ignores errors).
async fn unload_previous(state: &ServerState, previous_model_id: Option<&str>) {
    let Some(prev_id) = previous_model_id else {
        return;
    };

    if let Err(e) = unload(state, prev_id).await {
        info!("Note: Could not unload model (may already be unloaded): {:?}", e);
    }
}
