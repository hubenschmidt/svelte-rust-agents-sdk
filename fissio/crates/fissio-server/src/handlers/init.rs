//! Init endpoint returning models, templates, and configs.

use std::sync::Arc;

use axum::{extract::State, Json};

use crate::dto::InitResponse;
use crate::ServerState;

/// Returns initialization data for the frontend.
pub async fn init(State(state): State<Arc<ServerState>>) -> Json<InitResponse> {
    Json(InitResponse {
        models: state.models.clone(),
        templates: state.templates.clone(),
        configs: state.configs.read().await.clone(),
    })
}
