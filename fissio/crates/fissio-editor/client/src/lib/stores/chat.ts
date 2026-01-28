import { createSignal, createEffect } from 'solid-js';
import type { ChatMsg, HistoryMessage, ModelConfig, PipelineInfo, RuntimePipelineConfig, WsMetadata, ToolSchema } from '../types';
import { devMode } from './settings';

const API_BASE = 'http://localhost:8000';

function createChatStore() {
  const [messages, setMessages] = createSignal<ChatMsg[]>([
    { user: 'Bot', msg: 'Welcome! How can I help you today?' }
  ]);
  const [isConnected, setIsConnected] = createSignal(false);
  const [isStreaming, setIsStreaming] = createSignal(false);
  const [isThinking, setIsThinking] = createSignal(false);
  const [models, setModels] = createSignal<ModelConfig[]>([]);
  const [selectedModel, setSelectedModel] = createSignal<string>('');
  const [templates, setTemplates] = createSignal<PipelineInfo[]>([]);
  const [pipelines, setPipelines] = createSignal<PipelineInfo[]>([]);
  const [selectedPipeline, setSelectedPipeline] = createSignal<string>('');
  const [nodeModelOverrides, setNodeModelOverrides] = createSignal<Record<string, string>>({});
  const [modelStatus, setModelStatus] = createSignal<string>('');
  const [availableTools, setAvailableTools] = createSignal<ToolSchema[]>([]);

  // Mutable pipeline config (cloned from selected config, user can modify)
  const [pipelineConfig, setPipelineConfig] = createSignal<PipelineInfo | null>(null);
  const [pipelineModified, setPipelineModified] = createSignal(false);

  // Compose mode state
  const [composeMode, setComposeMode] = createSignal<'idle' | 'composing' | 'finalizing'>('idle');
  const [composeDraft, setComposeDraft] = createSignal<Partial<PipelineInfo> | null>(null);

  function buildComposePrompt(): string {
    const tools = availableTools();
    const toolsList = tools.length > 0
      ? tools.map(t => `- ${t.name}: ${t.description}`).join('\n')
      : '- (No tools currently available)';

    return `You are a pipeline design assistant helping users create agentic workflow patterns.

Available node types:
- llm: Language model node for text generation
- worker: Task execution node
- coordinator: Distributes work to multiple nodes
- aggregator: Combines outputs from multiple nodes
- orchestrator: Dynamic task decomposition and worker dispatch
- synthesizer: Synthesizes multiple inputs into coherent output
- router: Routes input to one of several paths based on classification
- gate: Checkpoint that validates before proceeding
- evaluator: Evaluates output quality, can trigger feedback loops

Available edge types:
- direct: Standard flow from one node to next
- conditional: Router decides which path (one of many)
- dynamic: Orchestrator decides which workers (subset of many)
- feedback: Loop back for iterative refinement (e.g., evaluator → generator)

Available tools that can be assigned to nodes:
${toolsList}

When a node needs to access external information or perform actions, assign appropriate tools.
For example, a "researcher" node that needs to find information should have the "web_search" tool.

Guide the user by:
1. Understanding their use case and requirements
2. Suggesting appropriate patterns (routing for classification, aggregator for multi-source, evaluator-optimizer for quality)
3. Building the pipeline incrementally through conversation
4. Suggesting appropriate tools for nodes that need external capabilities
5. Explaining your design choices

When the user says "/done" or indicates they're satisfied, output the final configuration as a fenced JSON block:
\`\`\`json
{
  "name": "Descriptive Pipeline Name",
  "description": "What this pipeline does",
  "nodes": [
    { "id": "node_id", "node_type": "llm|worker|router|etc", "prompt": "System prompt for this node", "tools": ["tool_name"] }
  ],
  "edges": [
    { "from": "input", "to": "first_node" },
    { "from": "node_id", "to": "output", "edge_type": "direct|conditional|dynamic|feedback" }
  ]
}
\`\`\`

The "tools" field is optional - only include it for nodes that need external capabilities.
Always include edges from "input" to the first node(s) and from final node(s) to "output".`;
  }

  let abortController: AbortController | null = null;

  // When selected config changes, clone it as the working config (or clear for Direct Chat)
  createEffect(() => {
    const id = selectedPipeline();
    const configs = pipelines();
    const config = configs.find((p) => p.id === id);
    setPipelineConfig(config ? structuredClone(config) : null);
    setPipelineModified(false);
    setNodeModelOverrides({});
  });

  async function fetchTools() {
    try {
      const res = await fetch(`${API_BASE}/tools`);
      if (res.ok) {
        const tools = await res.json();
        setAvailableTools(tools);
        console.log('[tools] Fetched', tools.length, 'available tools');
      }
    } catch (e) {
      console.warn('[tools] Failed to fetch tools:', e);
    }
  }

  async function connect(_url?: string) {
    try {
      const res = await fetch(`${API_BASE}/init`);
      if (!res.ok) {
        console.error('[init] Failed to fetch init data:', res.status);
        setIsConnected(false);
        return;
      }

      const data = await res.json();
      setIsConnected(true);

      if (data.models) {
        setModels(data.models);
        if (data.models.length > 0 && !selectedModel()) {
          setSelectedModel(data.models[0].id);
        }
      }

      if (data.templates) {
        setTemplates(data.templates);
      }

      if (data.configs) {
        console.log('[init] Received configs from backend:');
        data.configs.forEach((c: PipelineInfo) => {
          console.log(`  - ${c.id}: nodes with positions:`, c.nodes.map(n => ({ id: n.id, x: n.x, y: n.y })));
          console.log(`    layout:`, c.layout);
        });
        setPipelines(data.configs);
      }

      fetchTools();
    } catch (e) {
      console.error('[init] Connection error:', e);
      setIsConnected(false);
    }
  }

  function handleStreamChunk(chunk: string) {
    setIsThinking(false);
    const msgs = messages();
    const last = msgs[msgs.length - 1];

    if (last?.user === 'Bot' && last.streaming) {
      setMessages([...msgs.slice(0, -1), { user: 'Bot', msg: last.msg + chunk, streaming: true }]);
      return;
    }
    setIsStreaming(true);
    setMessages([...msgs, { user: 'Bot', msg: chunk, streaming: true }]);
  }

  function handleStreamEnd(metadata?: WsMetadata) {
    setIsStreaming(false);
    setIsThinking(false);
    const msgs = messages();
    const last = msgs[msgs.length - 1];
    if (!last?.streaming) return;
    setMessages([...msgs.slice(0, -1), { ...last, streaming: false, metadata }]);

    // In compose mode, check for JSON config in the response
    if (composeMode() !== 'composing') return;

    const currentMsgs = messages();
    const lastMsg = currentMsgs[currentMsgs.length - 1];
    if (!lastMsg || lastMsg.user !== 'Bot') return;

    const jsonMatch = lastMsg.msg.match(/```json\n([\s\S]*?)\n```/);
    if (!jsonMatch) return;

    try {
      const parsed = JSON.parse(jsonMatch[1]) as Partial<PipelineInfo>;
      // Generate ID if not provided
      if (!parsed.id) {
        parsed.id = `composed_${Date.now()}`;
      }
      setComposeDraft(parsed);
      setComposeMode('finalizing');
    } catch (e) {
      console.error('[compose] Failed to parse JSON:', e);
    }
  }

  function toRuntimeConfig(config: PipelineInfo): RuntimePipelineConfig {
    return {
      nodes: config.nodes.map((n) => ({
        id: n.id,
        type: n.node_type,
        model: n.model,
        prompt: n.prompt,
        tools: n.tools
      })),
      edges: config.edges.map((e) => ({
        from: e.from,
        to: e.to,
        edge_type: e.edge_type
      }))
    };
  }

  function buildHistory(): HistoryMessage[] {
    const msgs = messages();
    return msgs
      .filter((m) => m.user === 'User' || m.user === 'Bot')
      .map((m) => ({
        role: m.user === 'User' ? 'user' : 'assistant',
        content: m.msg
      })) as HistoryMessage[];
  }

  async function send(text: string) {
    if (!text.trim()) return;

    setMessages([...messages(), { user: 'User', msg: text }]);
    setIsThinking(true);

    const config = pipelineConfig();
    const mode = composeMode();

    const payload: Record<string, unknown> = {
      message: text,
      model_id: selectedModel(),
      verbose: devMode()
    };

    // In compose mode, send history and custom system prompt
    if (mode === 'composing') {
      payload.system_prompt = buildComposePrompt();
      payload.history = buildHistory();
    }

    // Always send full config for user-saved pipelines (unless in compose mode)
    if (config && mode !== 'composing') {
      payload.pipeline_config = toRuntimeConfig(config);
    }

    // Cancel any existing request
    abortController?.abort();
    abortController = new AbortController();

    try {
      const res = await fetch(`${API_BASE}/chat`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
        signal: abortController.signal
      });

      if (!res.ok || !res.body) {
        setIsThinking(false);
        handleStreamChunk('Error: Failed to connect to server.');
        handleStreamEnd();
        return;
      }

      const reader = res.body.getReader();
      const decoder = new TextDecoder();
      let buffer = '';

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split('\n');
        buffer = lines.pop() || '';

        for (const line of lines) {
          if (line.startsWith('data: ')) {
            try {
              const data = JSON.parse(line.slice(6));
              if (data.type === 'stream') {
                handleStreamChunk(data.content);
              } else if (data.type === 'end') {
                handleStreamEnd(data.metadata);
              }
            } catch (e) {
              console.warn('[sse] Failed to parse:', line, e);
            }
          }
        }
      }
    } catch (e) {
      if ((e as Error).name !== 'AbortError') {
        console.error('[chat] Request failed:', e);
        setIsThinking(false);
        handleStreamChunk('Error: Request failed.');
        handleStreamEnd();
      }
    }
  }

  function updateNode(nodeId: string, updates: Partial<{ prompt: string; model: string | null; node_type: string }>) {
    const config = pipelineConfig();
    if (!config) return;
    const nodes = config.nodes.map((n) =>
      n.id === nodeId ? { ...n, ...updates } : n
    );
    setPipelineConfig({ ...config, nodes });
    setPipelineModified(true);
  }

  function addNode(node: { id: string; node_type: string; prompt: string | null; model: string | null }) {
    const config = pipelineConfig();
    if (!config) return;
    setPipelineConfig({ ...config, nodes: [...config.nodes, node] });
    setPipelineModified(true);
  }

  function removeNode(nodeId: string) {
    const config = pipelineConfig();
    if (!config) return;
    const nodes = config.nodes.filter((n) => n.id !== nodeId);
    // Also remove edges referencing this node
    const edges = config.edges.filter((e) => {
      const fromIds = Array.isArray(e.from) ? e.from : [e.from];
      const toIds = Array.isArray(e.to) ? e.to : [e.to];
      return !fromIds.includes(nodeId) && !toIds.includes(nodeId);
    });
    setPipelineConfig({ ...config, nodes, edges });
    setPipelineModified(true);
  }

  function updateEdges(edges: PipelineInfo['edges']) {
    const config = pipelineConfig();
    if (!config) return;
    setPipelineConfig({ ...config, edges });
    setPipelineModified(true);
  }

  function resetPipeline() {
    const id = selectedPipeline();
    const preset = pipelines().find((p) => p.id === id);
    if (preset) {
      setPipelineConfig(structuredClone(preset));
      setPipelineModified(false);
    }
  }

  async function savePipeline(config: PipelineInfo) {
    const body = {
      id: config.id,
      name: config.name,
      description: config.description,
      nodes: config.nodes,
      edges: config.edges,
      layout: config.layout
    };
    console.log('[save] Sending save request:', config.id, config.name);
    console.log('[save] nodes with positions:', config.nodes.map(n => ({ id: n.id, x: n.x, y: n.y })));
    console.log('[save] layout:', config.layout);
    try {
      const res = await fetch(`${API_BASE}/pipelines/save`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body)
      });
      console.log('[save] Response status:', res.status);
      if (!res.ok) {
        console.error('[save] Save failed:', await res.text());
        return;
      }
    } catch (e) {
      console.error('[save] Fetch error:', e);
      return;
    }
    const list = pipelines();
    const idx = list.findIndex((p) => p.id === config.id);
    const updated = idx >= 0
      ? [...list.slice(0, idx), config, ...list.slice(idx + 1)]
      : [...list, config];
    setPipelines(updated);
    // Directly update pipelineConfig with the saved config (including positions)
    // Don't rely on selectedPipeline subscription since ID might not change
    console.log('[save] Setting pipelineConfig with positions:', config.nodes.map(n => ({ id: n.id, x: n.x, y: n.y })));
    setPipelineConfig(structuredClone(config));
    console.log('[save] Updated pipelines store and pipelineConfig:', config.id);
    setPipelineModified(false);
  }

  async function deletePipeline(id: string) {
    const res = await fetch(`${API_BASE}/pipelines/delete`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ id })
    });
    if (!res.ok) return;
    setPipelines(pipelines().filter((p) => p.id !== id));
    if (selectedPipeline() === id) {
      setSelectedPipeline('');
    }
  }

  async function wake(modelId: string, previousModelId?: string) {
    setModelStatus('loading');
    try {
      const params = previousModelId ? `?previous_model_id=${encodeURIComponent(previousModelId)}` : '';
      const res = await fetch(`${API_BASE}/models/${encodeURIComponent(modelId)}/wake${params}`, {
        method: 'POST'
      });
      if (res.ok) {
        console.log('[model] Woke model:', modelId);
      }
    } catch (e) {
      console.error('[model] Wake failed:', e);
    }
    setModelStatus('ready');
  }

  async function unload(modelId: string) {
    setModelStatus('unloading');
    try {
      const res = await fetch(`${API_BASE}/models/${encodeURIComponent(modelId)}`, {
        method: 'DELETE'
      });
      if (res.ok) {
        console.log('[model] Unloaded model:', modelId);
      }
    } catch (e) {
      console.error('[model] Unload failed:', e);
    }
    setModelStatus('ready');
  }

  function isLocalModel(modelId: string): boolean {
    const model = models().find((m) => m.id === modelId);
    return model?.api_base !== null && model?.api_base !== undefined;
  }

  function enterComposeMode() {
    setComposeMode('composing');
    setComposeDraft(null);
    setMessages([
      ...messages(),
      {
        user: 'Bot',
        msg: `**Compose Mode Activated** 🎨

I'll help you design an agentic workflow pattern. Describe your use case and I'll suggest appropriate node types and connections.

**Example use cases:**
- "I need to route customer questions to different specialists"
- "I want to generate content and then have it reviewed for quality"
- "I need to break down complex tasks and assign them to workers"

When you're satisfied with the design, type \`/done\` and I'll output the final configuration.

What would you like to build?`
      }
    ]);
  }

  function exitComposeMode() {
    setComposeMode('idle');
    setComposeDraft(null);
  }

  function reset() {
    setMessages([{ user: 'Bot', msg: 'Welcome! How can I help you today?' }]);
  }

  function disconnect() {
    abortController?.abort();
    abortController = null;
    setIsConnected(false);
  }

  return {
    messages,
    isConnected,
    isStreaming,
    isThinking,
    models,
    selectedModel,
    setSelectedModel,
    templates,
    pipelines,
    selectedPipeline,
    setSelectedPipeline,
    nodeModelOverrides,
    modelStatus,
    availableTools,
    pipelineConfig,
    setPipelineConfig,
    pipelineModified,
    composeMode,
    composeDraft,
    connect,
    send,
    wake,
    unload,
    isLocalModel,
    reset,
    disconnect,
    updateNode,
    addNode,
    removeNode,
    updateEdges,
    resetPipeline,
    savePipeline,
    deletePipeline,
    enterComposeMode,
    exitComposeMode
  };
}

export const chat = createChatStore();
