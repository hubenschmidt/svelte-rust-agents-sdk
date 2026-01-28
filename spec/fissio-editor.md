# fissio-editor - Optional Visual Editor Frontend

## Goal
Make the fissio framework's visual editor optional with two deployment options:
1. **Embedded in Rust binary** (feature flag) - for non-Docker projects
2. **Docker-compose service** - for containerized projects

## Architecture

```
fissio/
├── crates/
│   ├── fissio-editor/        # NEW: frontend + asset embedding
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   └── lib.rs        # Serves embedded assets via axum
│   │   ├── frontend/         # SolidJS app (moved from client/)
│   │   │   ├── package.json
│   │   │   ├── src/
│   │   │   └── dist/         # Built assets (gitignored)
│   │   └── build.rs          # Embeds dist/ at compile time
```

## Usage Patterns

### 1. Feature flag (embedded)
```toml
fissio = { version = "0.1", features = ["editor"] }
```
```rust
// Mounts editor UI at /editor
app.merge(fissio_editor::routes());
```

### 2. Docker-compose (separate service)
```yaml
services:
  backend:
    # ... fissio backend
  editor:
    image: fissio-editor
    environment:
      - VITE_API_URL=http://backend:8000
```

## Frontend Config
- `VITE_API_URL` - Backend websocket/API endpoint (build-time or runtime)
- Editor can connect to **any** fissio backend URL

## Steps
1. Create `fissio/crates/fissio-editor/` structure
2. Move `client/` contents to `fissio-editor/client/`
3. Add `build.rs` to embed `dist/` assets using `include_dir` or `rust-embed`
4. Create `lib.rs` with axum routes to serve embedded assets
5. Add `editor` feature flag to main `fissio` crate
6. Update docker-compose to use new location
7. Update `agent-server` to optionally mount editor routes

## Verification
- `cargo build -p fissio --features editor` embeds frontend
- `docker compose up` works with editor service
- Frontend connects to configurable backend URL

## Decisions
- **Separate Docker image** for editor (lightweight nginx/node serving static files)
- Delete `client/` after migration (no symlink)
