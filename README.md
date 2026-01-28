# Fissio

Fissio treats declarative agent definitions and graph topology as the primary abstraction for building agentic systems.

## Features

- **Visual Pipeline Editor** — Drag-and-drop node configuration with real-time preview
- **Template Patterns** — Built-in templates based on proven agentic architectures
- **Custom Configs** — Save, load, and manage pipeline configurations (SQLite)
- **Tool Support** — Assign tools (web search, URL fetch) to worker nodes
- **SSE Streaming** — Token-by-token response streaming via Server-Sent Events
- **Multi-provider** — OpenAI, Anthropic, and Ollama (local models)

## Architecture

```
┌─────────────────┐                    ┌─────────────────────────────────────┐
│  SolidJS UI     │◄────── SSE ────────│           fissio-server             │
│  (fissio-editor)│                    │                                     │
│                 │                    │  LLM Providers                      │
│  Header         │                    │  ├── OpenAI                         │
│  ├─ Pipeline    │                    │  ├── Anthropic                      │
│  └─ Model       │                    │  └── Ollama (local)                 │
│                 │                    │                                     │
│  Pipeline       │                    │  Pipeline Execution                 │
│  Editor         │                    │  ├── Templates (read-only)          │
│                 │                    │  └── User Configs (SQLite)          │
│  Chat           │                    │                                     │
│  └─ Streaming   │                    │  Node Types                         │
└─────────────────┘                    │  ├── llm, router, gate              │
                                       │  ├── coordinator, aggregator        │
                                       │  ├── orchestrator, worker           │
                                       │  └── evaluator, synthesizer         │
                                       └─────────────────────────────────────┘
```

## Quick Start (Docker)

```bash
# Create .env file
cp fissio/crates/fissio-server/.env.example fissio/crates/fissio-server/.env
# Edit with your API keys

docker compose up
```

- Editor: http://localhost:3001
- Server: http://localhost:8000

## Native Build

### Prerequisites

- Rust 1.75+
- Node.js 18+
- (Optional) [Ollama](https://ollama.ai) for local models

### Server

```bash
cd fissio/crates/fissio-server
cp .env.example .env
# Edit .env with your API keys

cargo run --release
```

### Editor

```bash
cd fissio/crates/fissio-editor/client
npm install
npm run dev -- --host
```

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `DATABASE_URL` | `data/pipelines.db` | SQLite database path |
| `OPENAI_API_KEY` | — | OpenAI API key |
| `ANTHROPIC_API_KEY` | — | Anthropic API key |
| `TAVILY_API_KEY` | — | Tavily web search API key |

## Usage

1. **Select a pipeline** from the dropdown
2. **Click edit** (pencil icon) to open the pipeline editor
3. **Modify nodes** — change prompts, models, or node types
4. **Save** your changes
5. **Send a message** to run the pipeline

---

## Library Usage

### Installation

```toml
[dependencies]
fissio = "0.1"
```

### Load from JSON

```rust
use fissio::prelude::*;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = PipelineConfig::from_file("pipeline.json")?;

    let models = vec![ModelConfig {
        id: "gpt-4".into(),
        name: "GPT-4".into(),
        model: "gpt-4-turbo".into(),
        api_base: None,
    }];
    let default_model = models[0].clone();

    let engine = PipelineEngine::new(config, models, default_model, HashMap::new());
    let result = engine.execute_stream("Hello!", &[]).await?;

    match result {
        EngineOutput::Complete(text) => println!("{}", text),
        EngineOutput::Stream(_) => println!("Streaming response..."),
    }
    Ok(())
}
```

### Builder API

```rust
use fissio::prelude::*;

let config = PipelineConfig::builder("research", "Research Pipeline")
    .description("Searches the web and summarizes findings")
    .node("researcher", NodeType::Worker)
        .prompt("You are a research assistant. Search for information.")
        .tools(["web_search", "fetch_url"])
        .done()
    .node("summarizer", NodeType::Llm)
        .prompt("Summarize the research findings concisely.")
        .model("gpt-4")
        .done()
    .edge("input", "researcher")
    .edge("researcher", "summarizer")
    .edge("summarizer", "output")
    .build();
```

### Pipeline Definition (JSON)

```json
{
  "id": "research-pipeline",
  "name": "Research Assistant",
  "nodes": [
    {
      "id": "researcher",
      "type": "worker",
      "prompt": "You are a research assistant.",
      "tools": ["web_search", "fetch_url"]
    },
    {
      "id": "summarizer",
      "type": "llm",
      "prompt": "Summarize the findings concisely.",
      "model": "gpt-4"
    }
  ],
  "edges": [
    { "from": "input", "to": "researcher" },
    { "from": "researcher", "to": "summarizer" },
    { "from": "summarizer", "to": "output" }
  ]
}
```

## Node Types

| Type | Description | Tools |
|------|-------------|-------|
| `llm` | Simple LLM call with system prompt | No |
| `worker` | LLM with agentic tool loop | Yes |
| `router` | Classifies input, routes to targets | No |
| `gate` | Validates input before proceeding | No |
| `aggregator` | Combines outputs from multiple nodes | No |
| `orchestrator` | Dynamic task decomposition | No |
| `evaluator` | Quality scoring of outputs | No |
| `synthesizer` | Synthesizes multiple inputs | No |
| `coordinator` | Distributes work to workers | No |

## Edge Types

| Type | Description |
|------|-------------|
| `direct` | Sequential execution (default) |
| `parallel` | Concurrent execution of all targets |
| `conditional` | Router chooses which path to follow |
| `dynamic` | Orchestrator dynamically selects targets |

## Custom Tools

```rust
use fissio::{Tool, ToolError, ToolRegistry};
use async_trait::async_trait;

struct CalculatorTool;

#[async_trait]
impl Tool for CalculatorTool {
    fn name(&self) -> &str { "calculator" }
    fn description(&self) -> &str { "Performs math calculations" }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "expression": {
                    "type": "string",
                    "description": "Math expression to evaluate"
                }
            },
            "required": ["expression"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<String, ToolError> {
        let expr = args["expression"].as_str()
            .ok_or_else(|| ToolError::InvalidArguments("missing expression".into()))?;
        Ok("42".to_string())
    }
}

let mut registry = ToolRegistry::with_defaults();
registry.register(CalculatorTool);
```

## LLM Providers

| Provider | Models | API Key Env Var |
|----------|--------|-----------------|
| OpenAI | `gpt-4`, `gpt-3.5-turbo`, etc. | `OPENAI_API_KEY` |
| Anthropic | `claude-3-*`, `claude-2`, etc. | `ANTHROPIC_API_KEY` |
| Ollama | Any local model | N/A (local) |

```rust
use fissio::UnifiedLlmClient;

let client = UnifiedLlmClient::new("gpt-4", None);        // OpenAI
let client = UnifiedLlmClient::new("claude-3-opus", None); // Anthropic
let client = UnifiedLlmClient::new("llama2", Some("http://localhost:11434/v1")); // Ollama
```

## Crate Structure

| Crate | Description |
|-------|-------------|
| `fissio` | Facade crate (re-exports all) |
| `fissio-config` | Pipeline schema, builders, node/edge types |
| `fissio-core` | Error types, messages, model config |
| `fissio-engine` | DAG execution engine |
| `fissio-llm` | LLM provider clients |
| `fissio-tools` | Tool registry and built-in tools |
| `fissio-editor` | Visual pipeline editor (SolidJS) |
| `fissio-server` | Standalone HTTP server with SSE |

## Feature Flags

```toml
[dependencies]
fissio = { version = "0.1", features = ["editor"] }
```

| Feature | Description |
|---------|-------------|
| `openai` | OpenAI provider support (default) |
| `anthropic` | Anthropic provider support (default) |
| `tools-web` | Web tools: fetch_url, web_search (default) |
| `editor` | Embed visual editor UI in your binary |

### Embedding the Editor

```rust
use axum::Router;

#[tokio::main]
async fn main() {
    let app = Router::new()
        .merge(fissio_editor::routes());  // Serves editor at /

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

## Built-in Tools

| Tool | Description | Requires |
|------|-------------|----------|
| `fetch_url` | Fetches content from a URL | — |
| `web_search` | Web search via Tavily API | `TAVILY_API_KEY` |

## Deployment

### Docker (Production)

```bash
docker build -t fissio-server ./fissio/crates/fissio-server
docker build -t fissio-editor ./fissio/crates/fissio-editor/client
```

### Embedded Binary

Single-binary deployment with embedded editor:

```bash
cd fissio/crates/fissio-editor/client
npm install && npm run build

cd ../..
cargo build -p fissio-server --features editor --release
```

## License

MIT
