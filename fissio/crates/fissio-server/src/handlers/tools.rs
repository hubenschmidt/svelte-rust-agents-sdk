//! Tool-related HTTP handlers.

use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Serialize;

use crate::ServerState;

/// Tool schema for API responses.
#[derive(Debug, Serialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Lists all available tools.
pub async fn list(State(state): State<Arc<ServerState>>) -> Json<Vec<ToolInfo>> {
    let tools = state.tool_registry.list()
        .into_iter()
        .map(|s| ToolInfo {
            name: s.name,
            description: s.description,
            parameters: s.parameters,
        })
        .collect();

    Json(tools)
}
