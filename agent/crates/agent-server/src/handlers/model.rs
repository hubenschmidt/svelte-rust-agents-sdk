//! Model management HTTP handlers (wake/unload).

use std::sync::Arc;

use axum::{extract::State, Json};

use crate::dto::{UnloadRequest, UnloadResponse, WakeRequest, WakeResponse};
use crate::error::AppError;
use crate::services;
use crate::ServerState;

/// Warms up a model by running a minimal request.
pub async fn wake(
    State(state): State<Arc<ServerState>>,
    Json(req): Json<WakeRequest>,
) -> Result<Json<WakeResponse>, AppError> {
    let prev = req.previous_model_id.as_deref();
    let model = services::model::warmup(&state, &req.model_id, prev).await?;
    Ok(Json(WakeResponse {
        success: true,
        model: model.name,
    }))
}

/// Unloads a model from GPU memory.
pub async fn unload(
    State(state): State<Arc<ServerState>>,
    Json(req): Json<UnloadRequest>,
) -> Result<Json<UnloadResponse>, AppError> {
    services::model::unload(&state, &req.model_id).await?;
    Ok(Json(UnloadResponse { success: true }))
}
