//! Pipeline CRUD HTTP handlers.

use std::sync::Arc;

use axum::{extract::State, Json};
use tracing::{error, info};

use crate::dto::{DeletePipelineRequest, PipelineInfo, SavePipelineRequest, SavePipelineResponse};
use crate::error::AppError;
use crate::ServerState;

/// Lists all saved pipeline configurations.
pub async fn list(
    State(state): State<Arc<ServerState>>,
) -> Json<Vec<PipelineInfo>> {
    let configs = state.configs.read().await;
    Json(configs.clone())
}

/// Saves a pipeline configuration.
pub async fn save(
    State(state): State<Arc<ServerState>>,
    Json(req): Json<SavePipelineRequest>,
) -> Result<Json<SavePipelineResponse>, AppError> {
    info!("Saving pipeline config: {} ({})", req.name, req.id);

    {
        let db = state.db_lock()?;
        crate::db::save_pipeline(&db, &req).map_err(|e| {
            error!("Failed to save pipeline: {}", e);
            AppError::Internal(format!("save failed: {}", e))
        })?;
    }

    let new_info = PipelineInfo {
        id: req.id.clone(),
        name: req.name,
        description: req.description,
        nodes: req.nodes,
        edges: req.edges,
        layout: req.layout,
    };

    let mut configs = state.configs.write().await;
    if let Some(idx) = configs.iter().position(|p| p.id == new_info.id) {
        configs[idx] = new_info;
    } else {
        configs.push(new_info);
    }

    info!("Pipeline config saved successfully: {}", req.id);
    Ok(Json(SavePipelineResponse { success: true, id: req.id }))
}

/// Deletes a pipeline configuration.
pub async fn delete(
    State(state): State<Arc<ServerState>>,
    Json(req): Json<DeletePipelineRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    info!("Deleting pipeline config: {}", req.id);

    {
        let db = state.db_lock()?;
        crate::db::delete_pipeline(&db, &req.id).map_err(|e| {
            error!("Failed to delete pipeline: {}", e);
            AppError::Internal(format!("delete failed: {}", e))
        })?;
    }

    let mut configs = state.configs.write().await;
    configs.retain(|p| p.id != req.id);

    Ok(Json(serde_json::json!({ "success": true })))
}
