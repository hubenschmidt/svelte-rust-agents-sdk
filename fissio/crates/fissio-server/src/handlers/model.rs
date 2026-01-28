//! Model management HTTP handlers (wake/unload).

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};
use serde::Deserialize;

use crate::dto::{UnloadResponse, WakeResponse};
use crate::error::AppError;
use crate::services;
use crate::ServerState;

/// Optional query params for wake endpoint.
#[derive(Debug, Deserialize, Default)]
pub struct WakeQuery {
    pub previous_model_id: Option<String>,
}

/// Warms up a model by running a minimal request.
pub async fn wake(
    State(state): State<Arc<ServerState>>,
    Path(model_id): Path<String>,
    axum::extract::Query(query): axum::extract::Query<WakeQuery>,
) -> Result<Json<WakeResponse>, AppError> {
    let prev = query.previous_model_id.as_deref();
    let model = services::model::warmup(&state, &model_id, prev).await?;
    Ok(Json(WakeResponse {
        success: true,
        model: model.name,
    }))
}

/// Unloads a model from GPU memory.
pub async fn unload(
    State(state): State<Arc<ServerState>>,
    Path(model_id): Path<String>,
) -> Result<Json<UnloadResponse>, AppError> {
    services::model::unload(&state, &model_id).await?;
    Ok(Json(UnloadResponse { success: true }))
}
