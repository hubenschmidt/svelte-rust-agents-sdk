import { writable, get } from 'svelte/store';
import type { ChatMsg, ModelConfig, PipelineInfo, RuntimePipelineConfig, WsPayload, WsResponse, WsMetadata } from '$lib/types';
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

	// Mutable pipeline config (cloned from selected config, user can modify)
	const pipelineConfig = writable<PipelineInfo | null>(null);
	const pipelineModified = writable(false);

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

	function connect(url: string) {
		ws = new WebSocket(url);

		ws.onopen = () => {
			isConnected.set(true);
			const payload: WsPayload = { uuid, init: true };
			ws?.send(JSON.stringify(payload));
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
				if (data.configs.length > 0 && !get(selectedPipeline)) {
					selectedPipeline.set(data.configs[0].id);
				}
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
			if (last?.streaming) {
				return [...msgs.slice(0, -1), { ...last, streaming: false, metadata }];
			}
			return msgs;
		});
	}

	function toRuntimeConfig(config: PipelineInfo): RuntimePipelineConfig {
		return {
			nodes: config.nodes.map((n) => ({
				id: n.id,
				type: n.node_type,
				model: n.model,
				prompt: n.prompt
			})),
			edges: config.edges.map((e) => ({
				from: e.from,
				to: e.to,
				edge_type: e.edge_type
			}))
		};
	}

	function send(text: string) {
		if (!ws || !text.trim()) return;

		messages.update((msgs) => [...msgs, { user: 'User', msg: text }]);
		isThinking.set(true);

		const config = get(pipelineConfig);

		const payload: WsPayload = {
			uuid,
			message: text,
			model_id: get(selectedModel),
			verbose: get(devMode)
		};

		// Always send full config for user-saved pipelines
		if (config) {
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
			const remaining = get(pipelines);
			selectedPipeline.set(remaining.length > 0 ? remaining[0].id : '');
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
		pipelineConfig,
		pipelineModified,
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
		deletePipeline
	};
}

export const chat = createChatStore();
