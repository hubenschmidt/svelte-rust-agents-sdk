# Consolidate into fissio Framework

## Goal
1. Move `agent-server` → `fissio-server` (default server, optional feature)
2. Replace WebSocket with SSE + REST
3. Delete `agent/` and `client/` directories

## Target Structure
```
fissio/
├── crates/
│   ├── fissio/           # Main crate with feature flags
│   ├── fissio-server/    # Default server (from agent-server)
│   ├── fissio-editor/    # Visual editor (done)
│   └── ...
```

## Usage
```toml
# Library with visual editor
fissio = { features = ["editor"] }

# Library only (for existing apps)
fissio = {}
```

The server (`fissio-server`) is a separate binary, run via:
```bash
cargo run -p fissio-server
# or via docker compose
```

## SSE + REST Endpoints (replacing WebSocket)
| Endpoint | Method | Purpose |
|----------|--------|---------|
| `POST /chat` | SSE stream | Chat with streaming response |
| `GET /init` | JSON | Models, templates, configs |
| `POST /models/:id/wake` | JSON | Wake/load a model |
| `DELETE /models/:id` | JSON | Unload a model |

### SSE Response Format (matching Anthropic style)
```
event: stream
data: {"content": "Hello"}

event: end
data: {"metadata": {"input_tokens": 10, "output_tokens": 50, "elapsed_ms": 1200}}
```

## Steps
1. Create `fissio/crates/fissio-server/`
2. Move `agent/crates/agent-server/src/*` → `fissio-server/src/`
3. Replace `ws.rs` with SSE handlers
4. Add `server` feature flag to main fissio crate
5. Update docker-compose to use fissio-server
6. Update frontend to use fetch/EventSource
7. Delete `agent/` and `client/` directories

## Verification
- `cargo build -p fissio --features server,editor` succeeds
- `docker compose up` works
- Chat streaming works via SSE
- Model endpoints work via REST
