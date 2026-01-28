//! HTTP server entry point and Axum router setup.
//!
//! Initializes the server state (models, presets, database), configures routes,
//! and starts the Axum server on port 8000.

mod db;
mod dto;
mod error;
mod handlers;
mod services;

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::RwLock;

use fissio_config::{EdgeEndpoint, PresetRegistry};
use fissio_core::ModelConfig;
use fissio_llm::discover_models;
use fissio_tools::ToolRegistry;

use crate::dto::{EdgeInfo, NodeInfo, PipelineInfo};
use anyhow::Result;
use axum::body::Body;
use axum::http::{Request, Response};
use axum::routing::{get, post};
use axum::Router;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::{info, warn};

const OLLAMA_HOST: &str = "http://host.docker.internal:11434";

/// Returns the list of cloud-hosted models (e.g., OpenAI).
fn cloud_models() -> Vec<ModelConfig> {
    vec![
        ModelConfig {
            id: "openai-gpt5".into(),
            name: "GPT-5.2 (OpenAI)".into(),
            model: "gpt-5.2-2025-12-11".into(),
            api_base: None,
        },
        ModelConfig {
            id: "openai-codex".into(),
            name: "GPT-5.2 Codex (OpenAI)".into(),
            model: "gpt-5.2-codex".into(),
            api_base: None,
        },
        ModelConfig {
            id: "anthropic-opus".into(),
            name: "Claude Opus 4.5 (Anthropic)".into(),
            model: "claude-opus-4-5-20251101".into(),
            api_base: None,
        },
        ModelConfig {
            id: "anthropic-sonnet".into(),
            name: "Claude Sonnet 4.5 (Anthropic)".into(),
            model: "claude-sonnet-4-5-20250929".into(),
            api_base: None,
        },
        ModelConfig {
            id: "anthropic-haiku".into(),
            name: "Claude Haiku 4.5 (Anthropic)".into(),
            model: "claude-haiku-4-5-20251001".into(),
            api_base: None,
        },
    ]
}

/// Shared server state accessible from all handlers.
pub struct ServerState {
    pub models: Vec<ModelConfig>,
    pub presets: PresetRegistry,
    pub templates: Vec<PipelineInfo>,
    pub configs: RwLock<Vec<PipelineInfo>>,
    pub db: Mutex<rusqlite::Connection>,
    pub tool_registry: ToolRegistry,
}

impl ServerState {
    /// Gets a model by ID, falling back to the first available model.
    pub fn get_model(&self, model_id: &str) -> ModelConfig {
        self.models
            .iter()
            .find(|m| m.id == model_id)
            .or_else(|| self.models.first())
            .cloned()
            .expect("at least one model must be configured")
    }

    /// Acquires the database lock, converting poison errors to AppError.
    pub fn db_lock(&self) -> Result<std::sync::MutexGuard<'_, rusqlite::Connection>, error::AppError> {
        self.db.lock().map_err(|e| {
            tracing::error!("DB lock poisoned: {}", e);
            error::AppError::Internal("database lock error".into())
        })
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_target(false)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".parse().unwrap()),
        )
        .compact()
        .init();

    let state = Arc::new(init_server_state().await);

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let trace_layer = TraceLayer::new_for_http()
        .make_span_with(|req: &Request<Body>| {
            tracing::info_span!(
                "request",
                method = %req.method(),
                uri = %req.uri(),
                version = ?req.version(),
            )
        })
        .on_response(|res: &Response<Body>, latency: Duration, _span: &tracing::Span| {
            info!(
                latency = %format!("{} ms", latency.as_millis()),
                status = %res.status().as_u16(),
                "finished processing request"
            );
        });

    let logged_routes = Router::new()
        .route("/chat", post(handlers::chat::chat))
        .route("/init", get(handlers::init::init))
        .route("/models/{id}/wake", post(handlers::model::wake))
        .route("/models/{id}", axum::routing::delete(handlers::model::unload))
        .route("/pipelines", get(handlers::pipeline::list))
        .route("/pipelines/save", post(handlers::pipeline::save))
        .route("/pipelines/delete", post(handlers::pipeline::delete))
        .route("/tools", get(handlers::tools::list))
        .layer(trace_layer);

    let app = Router::new()
        .merge(logged_routes)
        .route("/health", get(handlers::health))
        .layer(cors)
        .with_state(state);

    let addr = "0.0.0.0:8000";
    info!("Starting server on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// Initializes the server state: discovers models, loads presets, and seeds the database.
async fn init_server_state() -> ServerState {
    let discovery_future = discover_models(OLLAMA_HOST);

    let mut models = cloud_models();
    match discovery_future.await {
        Ok(ollama_models) => {
            info!("Found {} local Ollama models", ollama_models.len());
            for m in &ollama_models {
                info!("  - {} ({})", m.name, m.id);
            }
            models.extend(ollama_models);
        }
        Err(e) => {
            warn!("Ollama discovery failed (is Ollama running?): {}", e);
        }
    }

    // Load pipeline presets
    let presets_dir = Path::new("presets");
    let presets = PresetRegistry::load_from_dir(presets_dir).unwrap_or_else(|e| {
        warn!("Failed to load presets: {}", e);
        PresetRegistry::new()
    });

    fn endpoint_to_json(ep: &EdgeEndpoint) -> serde_json::Value {
        match ep {
            EdgeEndpoint::Single(s) => serde_json::Value::String(s.clone()),
            EdgeEndpoint::Multiple(v) => serde_json::Value::Array(
                v.iter().map(|s| serde_json::Value::String(s.clone())).collect()
            ),
        }
    }

    let templates: Vec<PipelineInfo> = presets
        .list()
        .iter()
        .map(|p| PipelineInfo {
            id: p.id.clone(),
            name: p.name.clone(),
            description: p.description.clone(),
            nodes: p.nodes.iter().map(|n| NodeInfo {
                id: n.id.clone(),
                node_type: format!("{:?}", n.node_type).to_lowercase(),
                model: n.model.clone(),
                prompt: n.prompt.clone(),
                tools: if n.tools.is_empty() { None } else { Some(n.tools.clone()) },
                x: None,
                y: None,
            }).collect(),
            edges: p.edges.iter().map(|e| EdgeInfo {
                from: endpoint_to_json(&e.from),
                to: endpoint_to_json(&e.to),
                edge_type: if e.edge_type == fissio_config::EdgeType::Direct {
                    None
                } else {
                    Some(format!("{:?}", e.edge_type).to_lowercase())
                },
            }).collect(),
            layout: None,
        })
        .collect();

    info!("Loaded {} pipeline templates", templates.len());
    for p in &templates {
        info!("  - {} ({})", p.name, p.id);
    }

    let db_path = std::env::var("DATABASE_URL").unwrap_or_else(|_| "data/pipelines.db".into());
    let conn = db::init_db(&db_path).expect("failed to initialize database");
    db::seed_examples(&conn).expect("failed to seed examples");
    let configs = db::list_user_pipelines(&conn);
    info!("Loaded {} saved configs", configs.len());

    let tool_registry = ToolRegistry::with_defaults();
    info!("Registered {} tools", tool_registry.list().len());

    ServerState {
        models,
        presets,
        templates,
        configs: RwLock::new(configs),
        db: Mutex::new(conn),
        tool_registry,
    }
}
