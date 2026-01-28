//! Application error types and Axum response conversion.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

/// Application-level errors with HTTP status code mapping.
#[derive(Debug)]
#[allow(dead_code)]
pub enum AppError {
    NotFound(String),
    BadRequest(String),
    Internal(String),
}

impl AppError {
    /// Creates an Internal error from any error type.
    pub fn internal(e: impl std::fmt::Display) -> Self {
        AppError::Internal(e.to_string())
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        AppError::Internal(e.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::Internal(e.to_string())
    }
}

impl From<fissio_core::AgentError> for AppError {
    fn from(e: fissio_core::AgentError) -> Self {
        AppError::Internal(e.to_string())
    }
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            AppError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };
        (status, Json(ErrorResponse { error: message })).into_response()
    }
}
