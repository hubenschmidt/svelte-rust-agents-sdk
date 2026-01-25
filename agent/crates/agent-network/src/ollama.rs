//! Native Ollama API client for model discovery, loading, and verbose metrics.
//!
//! Uses Ollama's native /api/chat endpoint (not OpenAI-compatible) to access
//! detailed performance metrics like tokens/sec, eval time, and load duration.

use std::pin::Pin;
use std::sync::{Arc, Mutex};

use agent_core::{AgentError, Message, MessageRole, ModelConfig};
use futures::Stream;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::StreamChunk;

/// Response from Ollama's /api/tags endpoint.
#[derive(Debug, Deserialize)]
pub struct OllamaTagsResponse {
    pub models: Vec<OllamaModelInfo>,
}

/// Information about a single Ollama model.
#[derive(Debug, Deserialize)]
pub struct OllamaModelInfo {
    pub name: String,
}

/// Discovers available models from an Ollama instance.
pub async fn discover_models(ollama_host: &str) -> Result<Vec<ModelConfig>, AgentError> {
    let client = Client::new();
    let url = format!("{}/api/tags", ollama_host.trim_end_matches('/'));

    let response = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .map_err(|e| AgentError::LlmError(format!("Ollama discovery failed: {}", e)))?;

    let tags: OllamaTagsResponse = response
        .json()
        .await
        .map_err(|e| AgentError::LlmError(format!("Failed to parse Ollama response: {}", e)))?;

    let models: Vec<ModelConfig> = tags
        .models
        .into_iter()
        .map(|m| {
            let display_name = format_display_name(&m.name);
            let id = format!("ollama-{}", slugify(&m.name));
            ModelConfig {
                id,
                name: display_name,
                model: m.name,
                api_base: Some(format!("{}/v1", ollama_host.trim_end_matches('/'))),
            }
        })
        .collect();

    info!("Discovered {} Ollama models", models.len());
    Ok(models)
}

/// Unloads a model from Ollama's memory.
pub async fn unload_model(ollama_host: &str, model_name: &str) -> Result<(), AgentError> {
    let client = Client::new();
    let url = format!("{}/api/chat", ollama_host.trim_end_matches('/'));

    let body = serde_json::json!({
        "model": model_name,
        "messages": [],
        "keep_alive": 0
    });

    client
        .post(&url)
        .json(&body)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| AgentError::LlmError(format!("Failed to unload model: {}", e)))?;

    info!("Unloaded model: {}", model_name);
    Ok(())
}

/// Formats a model name for display (e.g., "llama3:8b" -> "Llama3:8b (Local)").
fn format_display_name(model_name: &str) -> String {
    let last_segment = model_name.rsplit('/').next().unwrap_or(model_name);
    let (base, tag) = last_segment.split_once(':').unwrap_or((last_segment, ""));

    let mut chars = base.chars();
    let display_base = match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => String::new(),
    };

    let tag_suffix = if tag.is_empty() { String::new() } else { format!(":{tag}") };
    format!("{display_base}{tag_suffix} (Local)")
}

/// Converts a model name to a URL-safe slug.
fn slugify(name: &str) -> String {
    name.to_lowercase()
        .replace(['/', ':', '.'], "-")
        .replace("--", "-")
        .trim_matches('-')
        .to_string()
}

/// Performance metrics from Ollama's native API.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OllamaMetrics {
    #[serde(default)]
    pub total_duration: u64,
    #[serde(default)]
    pub load_duration: u64,
    #[serde(default)]
    pub prompt_eval_count: u32,
    #[serde(default)]
    pub prompt_eval_duration: u64,
    #[serde(default)]
    pub eval_count: u32,
    #[serde(default)]
    pub eval_duration: u64,
}

impl OllamaMetrics {
    /// Calculates tokens generated per second.
    pub fn tokens_per_sec(&self) -> f64 {
        if self.eval_duration == 0 {
            return 0.0;
        }
        (self.eval_count as f64) / (self.eval_duration as f64 / 1_000_000_000.0)
    }

    /// Total request duration in milliseconds.
    pub fn total_duration_ms(&self) -> u64 {
        self.total_duration / 1_000_000
    }

    /// Model load time in milliseconds.
    pub fn load_duration_ms(&self) -> u64 {
        self.load_duration / 1_000_000
    }

    /// Prompt evaluation time in milliseconds.
    pub fn prompt_eval_ms(&self) -> u64 {
        self.prompt_eval_duration / 1_000_000
    }

    /// Token generation time in milliseconds.
    pub fn eval_ms(&self) -> u64 {
        self.eval_duration / 1_000_000
    }
}

#[derive(Debug, Serialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    stream: bool,
}

#[derive(Debug, Serialize)]
struct OllamaMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct OllamaChatResponse {
    message: Option<OllamaResponseMessage>,
    done: bool,
    #[serde(flatten)]
    metrics: OllamaMetrics,
}

#[derive(Debug, Deserialize)]
struct OllamaResponseMessage {
    content: String,
}

/// Client for Ollama's native API with detailed metrics support.
pub struct OllamaClient {
    client: Client,
    api_base: String,
    model: String,
}

impl OllamaClient {
    /// Creates a new client for the given model and Ollama API base URL.
    pub fn new(model: &str, api_base: &str) -> Self {
        let base = api_base
            .trim_end_matches('/')
            .replace("/v1", "");

        Self {
            client: Client::new(),
            api_base: base,
            model: model.to_string(),
        }
    }

    /// Builds the message list for an Ollama chat request.
    fn build_messages(system_prompt: &str, history: &[Message], user_input: &str) -> Vec<OllamaMessage> {
        let mut messages = vec![OllamaMessage {
            role: "system".to_string(),
            content: system_prompt.to_string(),
        }];

        for msg in history {
            messages.push(OllamaMessage {
                role: match msg.role {
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                }
                .to_string(),
                content: msg.content.clone(),
            });
        }

        messages.push(OllamaMessage {
            role: "user".to_string(),
            content: user_input.to_string(),
        });

        messages
    }

    /// Sends a non-streaming chat request, returns content and metrics.
    pub async fn chat_with_metrics(
        &self,
        system_prompt: &str,
        history: &[Message],
        user_input: &str,
    ) -> Result<(String, OllamaMetrics), AgentError> {
        let url = format!("{}/api/chat", self.api_base);

        let request = OllamaChatRequest {
            model: self.model.clone(),
            messages: Self::build_messages(system_prompt, history, user_input),
            stream: false,
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| AgentError::LlmError(e.to_string()))?;

        let resp: OllamaChatResponse = response
            .json()
            .await
            .map_err(|e| AgentError::LlmError(e.to_string()))?;

        let content = resp.message.map(|m| m.content).unwrap_or_default();

        info!(
            "Ollama: {}ms total, {:.1} tok/s, {} eval tokens",
            resp.metrics.total_duration_ms(),
            resp.metrics.tokens_per_sec(),
            resp.metrics.eval_count
        );

        Ok((content, resp.metrics))
    }

    /// Sends a streaming chat request, returns a stream and metrics collector.
    pub async fn chat_stream_with_metrics(
        &self,
        system_prompt: &str,
        history: &[Message],
        user_input: &str,
    ) -> Result<(Pin<Box<dyn Stream<Item = Result<StreamChunk, AgentError>> + Send>>, OllamaMetricsCollector), AgentError>
    {
        use futures::StreamExt;

        let url = format!("{}/api/chat", self.api_base);

        let request = OllamaChatRequest {
            model: self.model.clone(),
            messages: Self::build_messages(system_prompt, history, user_input),
            stream: true,
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| AgentError::LlmError(e.to_string()))?;

        let metrics_collector = OllamaMetricsCollector::new();
        let collector_clone = metrics_collector.clone();

        let stream = response.bytes_stream();

        let mapped = stream.filter_map(move |result| {
            let collector = collector_clone.clone();
            async move {
                let bytes = match result {
                    Ok(b) => b,
                    Err(e) => return Some(Err(AgentError::LlmError(e.to_string()))),
                };

                let text = String::from_utf8_lossy(&bytes);
                for line in text.lines() {
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }

                    if let Ok(resp) = serde_json::from_str::<OllamaChatResponse>(line) {
                        if resp.done {
                            collector.set_metrics(resp.metrics);
                            return Some(Ok(StreamChunk::Usage {
                                input_tokens: collector.get_metrics().prompt_eval_count,
                                output_tokens: collector.get_metrics().eval_count,
                            }));
                        }

                        if let Some(msg) = resp.message {
                            if !msg.content.is_empty() {
                                return Some(Ok(StreamChunk::Content(msg.content)));
                            }
                        }
                    }
                }
                None
            }
        });

        Ok((Box::pin(mapped), metrics_collector))
    }
}

/// Collects metrics from a streaming Ollama response.
#[derive(Clone)]
pub struct OllamaMetricsCollector {
    metrics: Arc<Mutex<OllamaMetrics>>,
}

impl OllamaMetricsCollector {
    /// Creates a new metrics collector.
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(Mutex::new(OllamaMetrics::default())),
        }
    }

    /// Stores the final metrics from a completed stream.
    pub fn set_metrics(&self, metrics: OllamaMetrics) {
        if let Ok(mut m) = self.metrics.lock() {
            *m = metrics;
        }
    }

    /// Retrieves the collected metrics.
    pub fn get_metrics(&self) -> OllamaMetrics {
        self.metrics.lock().ok().map(|g| g.clone()).unwrap_or_default()
    }
}

impl Default for OllamaMetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}
