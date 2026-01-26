import { writable, get } from 'svelte/store';
import type { ChatMsg, HistoryMessage, ModelConfig, PipelineInfo, RuntimePipelineConfig, WsPayload, WsResponse, WsMetadata, ToolSchema } from '$lib/types';
import { devMode } from './settings';

function createChatStore() {
	const messages = writable<ChatMsg[]>([
		{ user: 'Bot', msg: 'Welcome! How can I help you today?' }
	]);
	const isConnected = writable(false);
	const isStreaming = writable(false);
	const isThinking = writable(false);
	const models = writable<ModelConfig[]>([]);
	const selectedModel = writable<string>('');
	const templates = writable<PipelineInfo[]>([]);
	const pipelines = writable<PipelineInfo[]>([]);
	const selectedPipeline = writable<string>('');
	const nodeModelOverrides = writable<Record<string, string>>({});
	const modelStatus = writable<string>('');
	const availableTools = writable<ToolSchema[]>([]);

	// Mutable pipeline config (cloned from selected config, user can modify)
	const pipelineConfig = writable<PipelineInfo | null>(null);
	const pipelineModified = writable(false);

	// Compose mode state
	const composeMode = writable<'idle' | 'composing' | 'finalizing'>('idle');
	const composeDraft = writable<Partial<PipelineInfo> | null>(null);

	function buildComposePrompt(): string {
		const tools = get(availableTools);
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

	let ws: WebSocket | null = null;
	const uuid = crypto.randomUUID();

	// When selected config changes, clone it as the working config (or clear for Direct Chat)
	selectedPipeline.subscribe((id) => {
		const configs = get(pipelines);
		const config = configs.find((p) => p.id === id);
		if (config) {
			pipelineConfig.set(structuredClone(config));
		} else {
			pipelineConfig.set(null);
		}
		pipelineModified.set(false);
		nodeModelOverrides.set({});
	});

	async function fetchTools() {
		try {
			const res = await fetch('http://localhost:8000/tools');
			if (res.ok) {
				const tools = await res.json();
				availableTools.set(tools);
				console.log('[tools] Fetched', tools.length, 'available tools');
			}
		} catch (e) {
			console.warn('[tools] Failed to fetch tools:', e);
		}
	}

	function connect(url: string) {
		ws = new WebSocket(url);

		ws.onopen = () => {
			isConnected.set(true);
			const payload: WsPayload = { uuid, init: true };
			ws?.send(JSON.stringify(payload));
			fetchTools();
		};

		ws.onclose = (ev) => {
			console.log('[ws] Connection closed:', ev.code, ev.reason, 'wasClean:', ev.wasClean);
			console.log('[ws] Close event details - code meanings: 1000=normal, 1001=going away, 1006=abnormal (server died)');
			isConnected.set(false);
			isStreaming.set(false);
			isThinking.set(false);
		};

		ws.onerror = (ev) => {
			console.error('[ws] Connection error:', ev);
			isConnected.set(false);
		};

		ws.onmessage = (event) => {
			const data: WsResponse = JSON.parse(event.data);

			if (data.models) {
				models.set(data.models);
				if (data.models.length > 0 && !get(selectedModel)) {
					selectedModel.set(data.models[0].id);
				}
			}

			if (data.templates) {
				templates.set(data.templates);
			}

			if (data.configs) {
				console.log('[ws] Received configs from backend:');
				data.configs.forEach(c => {
					console.log(`  - ${c.id}: nodes with positions:`, c.nodes.map(n => ({ id: n.id, x: n.x, y: n.y })));
					console.log(`    layout:`, c.layout);
				});
				pipelines.set(data.configs);
				// Keep default empty to show "Select agent" placeholder
			}

			if (data.models || data.templates || data.configs) {
				return;
			}

			if (data.model_status !== undefined) {
				modelStatus.set(data.model_status);
				return;
			}

			if (data.on_chat_model_stream !== undefined) {
				handleStreamChunk(data.on_chat_model_stream);
				return;
			}

			if (data.on_chat_model_end) {
				handleStreamEnd(data.metadata);
			}
		};
	}

	function handleStreamChunk(chunk: string) {
		isThinking.set(false);
		messages.update((msgs) => {
			const last = msgs[msgs.length - 1];

			if (last?.user === 'Bot' && last.streaming) {
				return [
					...msgs.slice(0, -1),
					{ user: 'Bot', msg: last.msg + chunk, streaming: true }
				];
			}

			isStreaming.set(true);
			return [...msgs, { user: 'Bot', msg: chunk, streaming: true }];
		});
	}

	function handleStreamEnd(metadata?: WsMetadata) {
		isStreaming.set(false);
		isThinking.set(false);
		messages.update((msgs) => {
			const last = msgs[msgs.length - 1];
			if (!last?.streaming) return msgs;
			return [...msgs.slice(0, -1), { ...last, streaming: false, metadata }];
		});

		// In compose mode, check for JSON config in the response
		if (get(composeMode) !== 'composing') return;

		const msgs = get(messages);
		const lastMsg = msgs[msgs.length - 1];
		if (!lastMsg || lastMsg.user !== 'Bot') return;

		const jsonMatch = lastMsg.msg.match(/```json\n([\s\S]*?)\n```/);
		if (!jsonMatch) return;

		try {
			const parsed = JSON.parse(jsonMatch[1]) as Partial<PipelineInfo>;
			// Generate ID if not provided
			if (!parsed.id) {
				parsed.id = `composed_${Date.now()}`;
			}
			composeDraft.set(parsed);
			composeMode.set('finalizing');
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
		const msgs = get(messages);
		return msgs
			.filter((m) => m.user === 'User' || m.user === 'Bot')
			.map((m) => ({
				role: m.user === 'User' ? 'user' : 'assistant',
				content: m.msg
			})) as HistoryMessage[];
	}

	function send(text: string) {
		if (!ws || !text.trim()) return;

		messages.update((msgs) => [...msgs, { user: 'User', msg: text }]);
		isThinking.set(true);

		const config = get(pipelineConfig);
		const mode = get(composeMode);

		const payload: WsPayload = {
			uuid,
			message: text,
			model_id: get(selectedModel),
			verbose: get(devMode)
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

		ws.send(JSON.stringify(payload));
	}

	function updateNode(nodeId: string, updates: Partial<{ prompt: string; model: string | null; node_type: string }>) {
		pipelineConfig.update((config) => {
			if (!config) return config;
			const nodes = config.nodes.map((n) =>
				n.id === nodeId ? { ...n, ...updates } : n
			);
			return { ...config, nodes };
		});
		pipelineModified.set(true);
	}

	function addNode(node: { id: string; node_type: string; prompt: string | null; model: string | null }) {
		pipelineConfig.update((config) => {
			if (!config) return config;
			return { ...config, nodes: [...config.nodes, node] };
		});
		pipelineModified.set(true);
	}

	function removeNode(nodeId: string) {
		pipelineConfig.update((config) => {
			if (!config) return config;
			const nodes = config.nodes.filter((n) => n.id !== nodeId);
			// Also remove edges referencing this node
			const edges = config.edges.filter((e) => {
				const fromIds = Array.isArray(e.from) ? e.from : [e.from];
				const toIds = Array.isArray(e.to) ? e.to : [e.to];
				return !fromIds.includes(nodeId) && !toIds.includes(nodeId);
			});
			return { ...config, nodes, edges };
		});
		pipelineModified.set(true);
	}

	function updateEdges(edges: PipelineInfo['edges']) {
		pipelineConfig.update((config) => {
			if (!config) return config;
			return { ...config, edges };
		});
		pipelineModified.set(true);
	}

	function resetPipeline() {
		const id = get(selectedPipeline);
		const preset = get(pipelines).find((p) => p.id === id);
		if (preset) {
			pipelineConfig.set(structuredClone(preset));
			pipelineModified.set(false);
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
			const res = await fetch('http://localhost:8000/pipelines/save', {
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
		pipelines.update((list) => {
			const idx = list.findIndex((p) => p.id === config.id);
			if (idx >= 0) return [...list.slice(0, idx), config, ...list.slice(idx + 1)];
			return [...list, config];
		});
		// Directly update pipelineConfig with the saved config (including positions)
		// Don't rely on selectedPipeline subscription since ID might not change
		console.log('[save] Setting pipelineConfig with positions:', config.nodes.map(n => ({ id: n.id, x: n.x, y: n.y })));
		pipelineConfig.set(structuredClone(config));
		console.log('[save] Updated pipelines store and pipelineConfig:', config.id);
		pipelineModified.set(false);
	}

	async function deletePipeline(id: string) {
		const res = await fetch('http://localhost:8000/pipelines/delete', {
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ id })
		});
		if (!res.ok) return;
		pipelines.update((list) => list.filter((p) => p.id !== id));
		if (get(selectedPipeline) === id) {
			selectedPipeline.set('');
		}
	}

	function wake(modelId: string, previousModelId?: string) {
		if (!ws) return;
		const payload: WsPayload = {
			uuid,
			wake_model_id: modelId,
			unload_model_id: previousModelId
		};
		ws.send(JSON.stringify(payload));
	}

	function unload(modelId: string) {
		if (!ws) return;
		const payload: WsPayload = {
			uuid,
			unload_model_id: modelId
		};
		ws.send(JSON.stringify(payload));
	}

	function isLocalModel(modelId: string): boolean {
		const model = get(models).find((m) => m.id === modelId);
		return model?.api_base !== null && model?.api_base !== undefined;
	}

	function enterComposeMode() {
		composeMode.set('composing');
		composeDraft.set(null);
		messages.update((msgs) => [
			...msgs,
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
		composeMode.set('idle');
		composeDraft.set(null);
	}

	function reset() {
		messages.set([{ user: 'Bot', msg: 'Welcome! How can I help you today?' }]);
	}

	function disconnect() {
		ws?.close();
		ws = null;
	}

	return {
		messages,
		isConnected,
		isStreaming,
		isThinking,
		models,
		selectedModel,
		templates,
		pipelines,
		selectedPipeline,
		nodeModelOverrides,
		modelStatus,
		availableTools,
		pipelineConfig,
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
