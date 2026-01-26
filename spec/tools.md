# Tool Calling System Implementation

## Goal
Add a generic tool calling system that allows any agent/node to use tools (web search, etc.) with:
- Extensible tool registry
- Agentic loop execution (LLM calls tools → gets results → continues until done)
- Composer mode tool suggestions
- Manual tool configuration in /composer
- (Ambitious) Composer-defined custom tools

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                        Tool Registry                             │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────────┐    │
│  │web_search│  │fetch_url │  │ run_code │  │ custom_tools │    │
│  └──────────┘  └──────────┘  └──────────┘  └──────────────┘    │
└─────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────┐
│                     Agentic Loop (per node)                      │
│  ┌─────────┐    ┌──────────┐    ┌────────────┐    ┌─────────┐  │
│  │  LLM    │ →  │ Tool     │ →  │ Execute    │ →  │ Append  │  │
│  │ Request │    │ Calls?   │    │ Tools      │    │ Results │  │
│  └─────────┘    └──────────┘    └────────────┘    └─────────┘  │
│       ↑              │ no             │                  │      │
│       │              ↓                │                  │      │
│       │         [Output]              └──────────────────┘      │
│       └─────────────────────────────────────────────────────────┘
└─────────────────────────────────────────────────────────────────┘
```

## Phase 1: Tool Registry & Types

### 1.1 Backend Tool System (`agent-tools` crate - NEW)

**File:** `agent/crates/agent-tools/src/lib.rs`

```rust
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> serde_json::Value; // JSON Schema
    async fn execute(&self, args: serde_json::Value) -> Result<String, ToolError>;
}

pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self;
    pub fn register(&mut self, tool: impl Tool + 'static);
    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>>;
    pub fn list(&self) -> Vec<ToolSchema>;
    pub fn schemas_for(&self, names: &[String]) -> Vec<ToolSchema>;
}
```

### 1.2 Built-in Tools

| Tool | Description | Implementation |
|------|-------------|----------------|
| `web_search` | Search the web via Tavily/SerpAPI | HTTP call to search API |
| `fetch_url` | Fetch and extract text from URL | reqwest + html2text |
| `run_code` | Execute code in sandbox | Subprocess or WASM sandbox |

### 1.3 Schema Updates

**Backend (`agent-config/src/lib.rs`):**
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    // ... existing fields ...
    #[serde(default)]
    pub tools: Vec<String>,  // Tool names from registry
}
```

**Frontend (`client/src/lib/types.ts`):**
```typescript
export type NodeInfo = {
  // ... existing fields ...
  tools?: string[];  // Tool names
};
```

## Phase 2: LLM Tool Integration

### 2.1 OpenAI (`agent-network/src/client.rs`)

Add tools to request builder:
```rust
pub async fn chat_with_tools(
    &self,
    system_prompt: &str,
    user_input: &str,
    tools: &[ToolSchema],
) -> Result<LlmResponse, AgentError> {
    let request = CreateChatCompletionRequestArgs::default()
        .model(&self.model)
        .messages(messages)
        .tools(tools.iter().map(to_openai_tool).collect())
        .build()?;
    // ...
}
```

### 2.2 Response Types

```rust
pub enum LlmResponse {
    Content(String),
    ToolCalls(Vec<ToolCall>),
}

pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}
```

## Phase 3: Agentic Loop Execution

### 3.1 Engine Update (`agent-engine/src/lib.rs`)

```rust
async fn execute_node_agentic(
    &self,
    node: &NodeConfig,
    input: &str,
    tool_registry: &ToolRegistry,
) -> Result<String, AgentError> {
    let tools = tool_registry.schemas_for(&node.tools);
    let mut messages = vec![...];

    loop {
        let response = client.chat_with_tools(&prompt, &messages, &tools).await?;

        match response {
            LlmResponse::Content(text) => return Ok(text),
            LlmResponse::ToolCalls(calls) => {
                for call in calls {
                    let tool = tool_registry.get(&call.name)?;
                    let result = tool.execute(call.arguments).await?;
                    messages.push(tool_result_message(call.id, result));
                }
            }
        }
    }
}
```

## Phase 4: Frontend Integration

### 4.1 Tool Selector in PipelineEditor

**File:** `client/src/lib/components/PipelineEditor.svelte`

- Fetch available tools from backend endpoint `/tools/list`
- Add multi-select for tools in node properties panel
- Display tool badges on nodes in graph view

### 4.2 Backend Endpoint

**File:** `agent-server/src/main.rs`

```rust
#[get("/tools/list")]
async fn list_tools(registry: &ToolRegistry) -> Json<Vec<ToolSchema>> {
    Json(registry.list())
}
```

## Phase 5: Composer Tool Suggestions

### 5.1 Update COMPOSE_SYSTEM_PROMPT

Add tool awareness to compose mode:
```
Available tools that can be assigned to nodes:
- web_search: Search the internet for information
- fetch_url: Retrieve content from a specific URL
- run_code: Execute code (Python, JavaScript)

When designing pipelines, suggest appropriate tools for each node.
Include tools in the JSON output:
{
  "nodes": [
    { "id": "researcher", "node_type": "worker", "tools": ["web_search", "fetch_url"], ... }
  ]
}
```

## Phase 6: Custom Tool Definition (Ambitious)

### 6.1 Tool Definition Schema

Allow users to define simple tools via JSON:
```json
{
  "name": "get_weather",
  "description": "Get current weather for a location",
  "parameters": {
    "type": "object",
    "properties": {
      "location": { "type": "string" }
    }
  },
  "implementation": {
    "type": "http",
    "url": "https://api.weather.com/v1/current",
    "method": "GET",
    "query_params": { "q": "{{location}}" }
  }
}
```

### 6.2 Composer Tool Builder

In /compose mode, LLM can suggest and define new tools:
- User describes needed capability
- LLM outputs tool definition JSON
- Tool gets registered dynamically

## Files to Modify/Create

| File | Action | Purpose |
|------|--------|---------|
| `agent-tools/src/lib.rs` | **CREATE** | Tool trait, registry, built-in tools |
| `agent-tools/src/web_search.rs` | **CREATE** | Web search tool implementation |
| `agent-tools/src/fetch_url.rs` | **CREATE** | URL fetch tool implementation |
| `agent-config/src/lib.rs` | MODIFY | Add `tools` field to NodeConfig |
| `agent-network/src/client.rs` | MODIFY | Add tool support to OpenAI client |
| `agent-network/src/unified.rs` | MODIFY | Route tools through unified client |
| `agent-engine/src/lib.rs` | MODIFY | Implement agentic loop |
| `agent-server/src/main.rs` | MODIFY | Add /tools/list endpoint |
| `agent-server/src/ws.rs` | MODIFY | Pass tool registry to engine |
| `client/src/lib/types.ts` | MODIFY | Add tools to NodeInfo |
| `client/src/lib/components/PipelineEditor.svelte` | MODIFY | Tool selector UI |
| `client/src/lib/stores/chat.ts` | MODIFY | Update compose prompt |

## Implementation Order

1. **Phase 1**: Tool registry crate + basic web_search tool
2. **Phase 2**: OpenAI tool integration
3. **Phase 3**: Agentic loop in engine
4. **Phase 4**: Frontend tool selector
5. **Phase 5**: Composer tool suggestions
6. **Phase 6**: Custom tool definition (if time permits)

## Verification

1. Create node with `web_search` tool
2. Ask "What's the latest news about AI?"
3. Verify: LLM calls web_search → gets results → formulates answer
4. Check streaming still works during agentic loop
5. Test multi-tool node (web_search + fetch_url)
6. Test /compose suggesting tools for new pipeline

## Environment Variables

```env
TAVILY_API_KEY=...  # For web search
# OR
SERPAPI_KEY=...     # Alternative search provider
```
