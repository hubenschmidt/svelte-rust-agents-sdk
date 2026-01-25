# agents-rs

A visual agent orchestration platform with a Svelte 5 frontend and Rust backend. Design, configure, and run multi-agent pipelines using proven agentic patterns.

## Features

- **Visual Pipeline Editor** — Drag-and-drop node configuration with real-time preview
- **Template Patterns** — 5 built-in templates based on proven agentic architectures
- **Custom Configs** — Save, load, and manage your own pipeline configurations
- **Real-time Streaming** — Token-by-token response streaming via WebSocket
- **Multi-model Support** — OpenAI cloud models + auto-discovered Ollama local models
- **Dev Mode** — Toggle verbose metrics (tokens/sec, eval time, load time)

## Pipeline Patterns

The app includes templates for common agentic patterns:

| Pattern | Use Case | Example |
|---------|----------|---------|
| **Prompt Chaining** | Sequential refinement with quality gates | Blog Post Writer |
| **Routing** | Classify and dispatch to specialized handlers | Customer Support Bot |
| **Parallelization** | Run independent tasks concurrently | Document Reviewer |
| **Orchestrator-Worker** | Dynamic task decomposition | Research Assistant |
| **Evaluator-Optimizer** | Self-critique loop for quality assurance | Code Generator |

## Architecture

```
┌─────────────────┐                    ┌─────────────────────────────────────┐
│  Svelte 5 UI    │◄──── WebSocket ────│              Agent                  │
│                 │                    │                                     │
│  Header         │                    │  Model Discovery                    │
│  ├─ Config      │                    │  ├── OpenAI (cloud)                 │
│  └─ Model       │                    │  └── Ollama /api/tags (local)       │
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

## Technologies

- **Client:** Svelte 5, SvelteKit, TypeScript
- **Agent:** Rust, Axum, Tokio, SQLite (rusqlite)
- **LLM:** OpenAI API, Ollama (local models)

## Prerequisites

- Docker & Docker Compose
- (Optional) [Ollama](https://ollama.ai) for local models

## Environment

Create a `.env` file in `agent/`:

```env
OPENAI_API_KEY=sk-...
RUST_LOG=info
```

## Run

```bash
docker compose up
```

- Client: http://localhost:3001
- Agent: http://localhost:8000

## Usage

1. **Select a config** from the dropdown (5 examples included)
2. **Click the edit button** (✎) to open the pipeline editor
3. **Modify nodes** — change prompts, models, or node types
4. **Save** your changes as a new config or update existing
5. **Send a message** to run the pipeline

## Local Models (Ollama)

The agent auto-discovers installed Ollama models at startup.

```bash
ollama pull llama3.1
ollama serve
```

Models appear in the dropdown automatically.

## Project Structure

```
.
├── agent/                    # Rust backend
│   ├── crates/
│   │   ├── agent-core/       # Shared types, ModelConfig
│   │   ├── agent-config/     # Pipeline presets, node types
│   │   ├── agent-network/    # Ollama discovery
│   │   └── agent-server/     # Axum server, WebSocket, SQLite
│   └── data/                 # SQLite database (auto-created)
├── client/                   # SvelteKit frontend
│   └── src/
│       ├── lib/components/   # Header, PipelineEditor, Chat
│       ├── lib/stores/       # chat.ts, settings.ts
│       └── routes/           # +page.svelte
└── docker-compose.yml
```
