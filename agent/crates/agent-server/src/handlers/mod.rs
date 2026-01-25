//! HTTP route handlers for the agent server.

pub mod model;
pub mod pipeline;

/// Health check endpoint.
pub async fn health() -> &'static str {
    "OK"
}
