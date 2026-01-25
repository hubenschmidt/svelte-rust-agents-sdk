mod db;
mod dto;
mod error;
mod handlers;
mod services;
mod ws;

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::RwLock;

use agent_config::{EdgeEndpoint, PresetRegistry};
use agent_core::ModelConfig;
use agent_network::discover_models;

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

fn cloud_models() -> Vec<ModelConfig> {
    vec![ModelConfig {
        id: "openai-gpt4o".into(),
        name: "GPT-4o (OpenAI)".into(),
        model: "gpt-4o".into(),
        api_base: None,
    }]
}

pub struct ServerState {
    pub models: Vec<ModelConfig>,
    pub presets: PresetRegistry,
    pub templates: Vec<PipelineInfo>,
    pub configs: RwLock<Vec<PipelineInfo>>,
    pub db: Mutex<rusqlite::Connection>,
}

impl ServerState {
    pub fn get_model(&self, model_id: &str) -> ModelConfig {
        self.models
            .iter()
            .find(|m| m.id == model_id)
            .or_else(|| self.models.first())
            .cloned()
            .expect("at least one model must be configured")
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
        .route("/ws", get(ws::ws_handler))
        .route("/wake", post(handlers::model::wake))
        .route("/unload", post(handlers::model::unload))
        .route("/pipelines", get(handlers::pipeline::list))
        .route("/pipelines/save", post(handlers::pipeline::save))
        .route("/pipelines/delete", post(handlers::pipeline::delete))
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
            }).collect(),
            edges: p.edges.iter().map(|e| EdgeInfo {
                from: endpoint_to_json(&e.from),
                to: endpoint_to_json(&e.to),
                edge_type: if e.edge_type == agent_config::EdgeType::Direct {
                    None
                } else {
                    Some(format!("{:?}", e.edge_type).to_lowercase())
                },
            }).collect(),
        })
        .collect();

    info!("Loaded {} pipeline templates", templates.len());
    for p in &templates {
        info!("  - {} ({})", p.name, p.id);
    }

    let conn = db::init_db("data/pipelines.db").expect("failed to initialize database");
    db::seed_examples(&conn).expect("failed to seed examples");
    let configs = db::list_user_pipelines(&conn);
    info!("Loaded {} saved configs", configs.len());

    ServerState {
        models,
        presets,
        templates,
        configs: RwLock::new(configs),
        db: Mutex::new(conn),
    }
}
