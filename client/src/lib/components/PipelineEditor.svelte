<script lang="ts">
	import { onMount } from 'svelte';
	import type { PipelineInfo, NodeInfo, EdgeInfo, ModelConfig, ToolSchema } from '$lib/types';

	// eslint-disable-next-line @typescript-eslint/no-explicit-any
	let dagre: any = null;
	let dagreReady = false;

	onMount(async () => {
		// @ts-ignore - CDN import for ESM-compatible dagre
		const mod = await import('https://esm.sh/@dagrejs/dagre@1.1.4');
		dagre = mod.default;
		dagreReady = true;
	});

	export let config: PipelineInfo;
	export let models: ModelConfig[];
	export let templates: PipelineInfo[] = [];
	export let availableTools: ToolSchema[] = [];
	export let onUpdate: (config: PipelineInfo) => void;
	export let onSave: (config: PipelineInfo) => void = () => {};

	const nodeTypes = ['llm', 'worker', 'coordinator', 'aggregator', 'orchestrator', 'synthesizer', 'router', 'gate', 'evaluator'];
	const edgeTypes = ['direct', 'conditional', 'dynamic', 'feedback'];

	const nodeColors: Record<string, string> = {
		llm: '#8b5cf6',
		router: '#3b82f6',
		gate: '#3b82f6',
		worker: '#22c55e',
		orchestrator: '#f97316',
		synthesizer: '#ec4899',
		aggregator: '#ec4899',
		coordinator: '#06b6d4',
		evaluator: '#eab308',
		input: '#6b7280',
		output: '#6b7280'
	};

	const NODE_WIDTH = 140;
	const NODE_HEIGHT = 50;

	let selectedNodeId: string | null = null;
	let selectedEdgeIndex: number | null = null;
	let selectedTemplate = '';
	let newEdgeFrom = '';
	let newEdgeTo = '';
	let newEdgeType = 'direct';

	// Drag state
	let draggingNodeId: string | null = null;
	let dragOffset = { x: 0, y: 0 };
	let nodePositionOverrides: Record<string, { x: number; y: number }> = {};

	// Sidebar state
	let sidebarOpen = true;
	let sidebarTab: 'properties' | 'tools' = 'properties';

	// Load positions from config on mount
	function loadPositionsFromConfig() {
		console.log('[loadPositions] loading from config:', config.id);
		console.log('[loadPositions] config.nodes:', config.nodes.map(n => ({ id: n.id, x: n.x, y: n.y })));
		console.log('[loadPositions] config.layout:', config.layout);

		const positions: Record<string, { x: number; y: number }> = {};
		// Load from nodes
		config.nodes.forEach(n => {
			if (n.x !== undefined && n.y !== undefined) {
				positions[n.id] = { x: n.x, y: n.y };
			}
		});
		// Load from layout (for input/output)
		if (config.layout) {
			Object.entries(config.layout).forEach(([id, pos]) => {
				positions[id] = pos;
			});
		}
		console.log('[loadPositions] loaded positions:', positions);
		nodePositionOverrides = positions;
	}

	// Load positions on mount
	onMount(() => {
		loadPositionsFromConfig();
		// If dagre is already ready (hot reload), rebuild layout
		if (dagreReady) {
			rebuildLayout();
		}
	});

	$: selectedNode = selectedNodeId ? config.nodes.find(n => n.id === selectedNodeId) : null;
	$: selectedEdge = selectedEdgeIndex !== null ? config.edges[selectedEdgeIndex] : null;

	type LayoutNode = { id: string; x: number; y: number; type: string };
	type LayoutEdge = { from: string; to: string; points: {x: number; y: number}[]; edgeType: string; index: number };
	type Layout = { nodes: LayoutNode[]; edges: LayoutEdge[]; width: number; height: number };

	let layout: Layout = { nodes: [], edges: [], width: 500, height: 400 };

	function rebuildLayout() {
		const base = computeLayout(config.nodes, config.edges);
		// Apply manual position overrides
		base.nodes = base.nodes.map(n => {
			const override = nodePositionOverrides[n.id];
			return override ? { ...n, x: override.x, y: override.y } : n;
		});
		// Recompute edges based on final node positions
		if (Object.keys(nodePositionOverrides).length > 0) {
			base.edges = recomputeEdges(base.nodes, config.edges);
		}
		layout = base;
	}

	// Rebuild layout when dagre ready, positions change, or nodes/edges change
	$: if (dagreReady) {
		// Track these dependencies to trigger rebuild
		config.nodes;
		config.edges;
		nodePositionOverrides;
		rebuildLayout();
	}

	function recomputeEdges(nodes: LayoutNode[], edges: EdgeInfo[]): LayoutEdge[] {
		const posMap: Record<string, { x: number; y: number }> = {};
		nodes.forEach(n => { posMap[n.id] = { x: n.x, y: n.y }; });

		const result: LayoutEdge[] = [];
		(edges || []).forEach((e, idx) => {
			const froms = Array.isArray(e.from) ? e.from : [e.from];
			const tos = Array.isArray(e.to) ? e.to : [e.to];
			froms.forEach(f => {
				tos.forEach(t => {
					const fromPos = posMap[f];
					const toPos = posMap[t];
					if (fromPos && toPos) {
						result.push({
							from: f,
							to: t,
							points: [
								{ x: fromPos.x, y: fromPos.y + NODE_HEIGHT / 2 },
								{ x: toPos.x, y: toPos.y - NODE_HEIGHT / 2 }
							],
							edgeType: e.edge_type || 'direct',
							index: idx
						});
					}
				});
			});
		});
		return result;
	}

	function computeLayout(nodes: NodeInfo[], edges: EdgeInfo[]): Layout {
		const nodeList = nodes || [];
		const edgeList = edges || [];

		// Determine which virtual nodes are needed
		const hasInput = edgeList.some(e => (Array.isArray(e.from) ? e.from : [e.from]).includes('input'));
		const hasOutput = edgeList.some(e => (Array.isArray(e.to) ? e.to : [e.to]).includes('output'));

		if (!dagre) {
			return computeFallbackLayout(nodeList, edgeList, hasInput, hasOutput);
		}

		try {
			const g = new dagre.graphlib.Graph();
			g.setGraph({ rankdir: 'TB', nodesep: 60, ranksep: 80, marginx: 50, marginy: 50 });
			g.setDefaultEdgeLabel(() => ({}));

			if (hasInput) g.setNode('input', { width: NODE_WIDTH, height: NODE_HEIGHT });
			if (hasOutput) g.setNode('output', { width: NODE_WIDTH, height: NODE_HEIGHT });

			nodeList.forEach(n => g.setNode(n.id, { width: NODE_WIDTH, height: NODE_HEIGHT }));

			edgeList.forEach(e => {
				const froms = Array.isArray(e.from) ? e.from : [e.from];
				const tos = Array.isArray(e.to) ? e.to : [e.to];
				froms.forEach(f => tos.forEach(t => g.setEdge(f, t)));
			});

			dagre.layout(g);

				const layoutNodes: LayoutNode[] = [];

				if (hasInput) {
					const n = g.node('input');
					if (n) layoutNodes.push({ id: 'input', x: n.x, y: n.y, type: 'input' });
				}

				nodeList.forEach(node => {
					const n = g.node(node.id);
					if (n) layoutNodes.push({ id: node.id, x: n.x, y: n.y, type: node.node_type });
				});

				if (hasOutput) {
					const n = g.node('output');
					if (n) layoutNodes.push({ id: 'output', x: n.x, y: n.y, type: 'output' });
				}

				const layoutEdges: LayoutEdge[] = [];
				edgeList.forEach((e, idx) => {
					const froms = Array.isArray(e.from) ? e.from : [e.from];
					const tos = Array.isArray(e.to) ? e.to : [e.to];
					froms.forEach(f => {
						tos.forEach(t => {
							const edge = g.edge(f, t);
							if (edge && edge.points) {
								layoutEdges.push({ from: f, to: t, points: edge.points, edgeType: e.edge_type || 'direct', index: idx });
							}
						});
					});
				});

				const info = g.graph();
				console.log('[dagre] layout complete:', layoutNodes.length, 'nodes,', layoutEdges.length, 'edges');
				return {
					nodes: layoutNodes,
					edges: layoutEdges,
					width: Math.max((info?.width || 0) + 100, 500),
					height: Math.max((info?.height || 0) + 100, 400)
				};
			} catch (err) {
			console.error('Dagre error:', err);
			return computeFallbackLayout(nodeList, edgeList, hasInput, hasOutput);
		}
	}

	function computeFallbackLayout(nodeList: NodeInfo[], edgeList: EdgeInfo[], hasInput: boolean, hasOutput: boolean): Layout {
		const nodePositions: Record<string, {x: number; y: number}> = {};
		const allNodes: LayoutNode[] = [];
		const cols = Math.max(Math.ceil(Math.sqrt(nodeList.length + 2)), 2);
		let i = 0;

		if (hasInput || nodeList.length === 0) {
			const pos = { x: 100 + (i % cols) * 180, y: 80 + Math.floor(i / cols) * 100 };
			allNodes.push({ id: 'input', ...pos, type: 'input' });
			nodePositions['input'] = pos;
			i++;
		}

		nodeList.forEach(n => {
			const pos = { x: 100 + (i % cols) * 180, y: 80 + Math.floor(i / cols) * 100 };
			allNodes.push({ id: n.id, ...pos, type: n.node_type });
			nodePositions[n.id] = pos;
			i++;
		});

		if (hasOutput || nodeList.length === 0) {
			const pos = { x: 100 + (i % cols) * 180, y: 80 + Math.floor(i / cols) * 100 };
			allNodes.push({ id: 'output', ...pos, type: 'output' });
			nodePositions['output'] = pos;
		}

		const fallbackEdges: LayoutEdge[] = [];
		edgeList.forEach((e, idx) => {
			const froms = Array.isArray(e.from) ? e.from : [e.from];
			const tos = Array.isArray(e.to) ? e.to : [e.to];
			froms.forEach(f => {
				tos.forEach(t => {
					const fromPos = nodePositions[f];
					const toPos = nodePositions[t];
					if (!fromPos || !toPos) return;
					fallbackEdges.push({
						from: f,
						to: t,
						points: [
							{ x: fromPos.x, y: fromPos.y + NODE_HEIGHT / 2 },
							{ x: toPos.x, y: toPos.y - NODE_HEIGHT / 2 }
						],
						edgeType: e.edge_type || 'direct',
						index: idx
					});
				});
			});
		});

		console.log('[fallback] layout:', allNodes.length, 'nodes,', fallbackEdges.length, 'edges');
		return { nodes: allNodes, edges: fallbackEdges, width: cols * 180 + 100, height: Math.ceil((nodeList.length + 2) / cols) * 100 + 100 };
	}

	function pathD(points: {x: number; y: number}[]): string {
		if (points.length < 2) return '';
		return `M ${points[0].x} ${points[0].y} ` + points.slice(1).map(p => `L ${p.x} ${p.y}`).join(' ');
	}

	function edgeDash(type: string): string {
		if (type === 'conditional') return '8,4';
		if (type === 'dynamic') return '2,4';
		if (type === 'feedback') return '6,3,2,3';
		return '';
	}

	function edgeColor(type: string, selected: boolean): string {
		if (selected) return '#3b82f6';
		if (type === 'feedback') return '#ef4444';
		return '#888';
	}

	function applyTemplate(templateId: string) {
		const tpl = templates.find(t => t.id === templateId);
		if (!tpl) return;
		onUpdate({ ...config, nodes: structuredClone(tpl.nodes), edges: structuredClone(tpl.edges) });
	}

	function selectNode(id: string) {
		selectedNodeId = id;
		selectedEdgeIndex = null;
		sidebarOpen = true;
	}

	function selectEdge(idx: number) {
		selectedEdgeIndex = idx;
		selectedNodeId = null;
		sidebarOpen = true;
	}

	function updateNodeField(nodeId: string, field: string, value: string | null) {
		const nodes = config.nodes.map(n => n.id === nodeId ? { ...n, [field]: value } : n);
		onUpdate({ ...config, nodes });
	}

	function toggleNodeTool(nodeId: string, toolName: string) {
		const nodes = config.nodes.map(n => {
			if (n.id !== nodeId) return n;
			const currentTools = n.tools || [];
			const hasIt = currentTools.includes(toolName);
			const newTools = hasIt ? currentTools.filter(t => t !== toolName) : [...currentTools, toolName];
			return { ...n, tools: newTools.length > 0 ? newTools : undefined };
		});
		onUpdate({ ...config, nodes });
	}

	function updateEdgeType(idx: number, type: string) {
		const edges = config.edges.map((e, i) => i === idx ? { ...e, edge_type: type } : e);
		onUpdate({ ...config, edges });
	}

	function addNode() {
		const id = `node_${config.nodes.length + 1}`;
		onUpdate({ ...config, nodes: [...config.nodes, { id, node_type: 'worker', model: null, prompt: 'You are a helpful assistant.' }] });
		selectedNodeId = id;
	}

	function removeNode(id: string) {
		const nodes = config.nodes.filter(n => n.id !== id);
		const edges = config.edges.filter(e => {
			const f = Array.isArray(e.from) ? e.from : [e.from];
			const t = Array.isArray(e.to) ? e.to : [e.to];
			return !f.includes(id) && !t.includes(id);
		});
		if (selectedNodeId === id) selectedNodeId = null;
		onUpdate({ ...config, nodes, edges });
	}

	function addEdge() {
		if (!newEdgeFrom || !newEdgeTo) return;
		onUpdate({ ...config, edges: [...config.edges, { from: newEdgeFrom, to: newEdgeTo, edge_type: newEdgeType }] });
		newEdgeFrom = '';
		newEdgeTo = '';
	}

	function removeEdge(idx: number) {
		const edges = config.edges.filter((_, i) => i !== idx);
		if (selectedEdgeIndex === idx) selectedEdgeIndex = null;
		onUpdate({ ...config, edges });
	}

	function saveWithPositions() {
		console.log('[saveWithPositions] nodePositionOverrides:', nodePositionOverrides);

		// Merge positions into nodes
		const nodesWithPositions = config.nodes.map(n => {
			const pos = nodePositionOverrides[n.id];
			return pos ? { ...n, x: pos.x, y: pos.y } : n;
		});

		// Collect input/output positions into layout
		const layoutPositions: Record<string, { x: number; y: number }> = {};
		if (nodePositionOverrides['input']) {
			layoutPositions['input'] = nodePositionOverrides['input'];
		}
		if (nodePositionOverrides['output']) {
			layoutPositions['output'] = nodePositionOverrides['output'];
		}

		const configWithPositions = {
			...config,
			nodes: nodesWithPositions,
			layout: Object.keys(layoutPositions).length > 0 ? layoutPositions : undefined
		};

		console.log('[saveWithPositions] saving config with positions:', configWithPositions.nodes.map(n => ({ id: n.id, x: n.x, y: n.y })));
		console.log('[saveWithPositions] layout:', configWithPositions.layout);

		onSave(configWithPositions);
	}

	// Drag handlers
	let svgElement: SVGSVGElement;

	function startDrag(e: MouseEvent, nodeId: string) {
		e.preventDefault();
		e.stopPropagation();
		draggingNodeId = nodeId;
		const node = layout.nodes.find(n => n.id === nodeId);
		if (node && svgElement) {
			const pt = svgElement.createSVGPoint();
			pt.x = e.clientX;
			pt.y = e.clientY;
			const ctm = svgElement.getScreenCTM();
			if (ctm) {
				const svgP = pt.matrixTransform(ctm.inverse());
				dragOffset = { x: svgP.x - node.x, y: svgP.y - node.y };
			}
		}
		// Attach to window for dragging outside SVG bounds
		window.addEventListener('mousemove', onDrag);
		window.addEventListener('mouseup', endDrag);
	}

	function onDrag(e: MouseEvent) {
		if (!draggingNodeId || !svgElement) return;
		const pt = svgElement.createSVGPoint();
		pt.x = e.clientX;
		pt.y = e.clientY;
		const ctm = svgElement.getScreenCTM();
		if (ctm) {
			const svgP = pt.matrixTransform(ctm.inverse());
			nodePositionOverrides = {
				...nodePositionOverrides,
				[draggingNodeId]: { x: svgP.x - dragOffset.x, y: svgP.y - dragOffset.y }
			};
		}
	}

	function endDrag() {
		draggingNodeId = null;
		window.removeEventListener('mousemove', onDrag);
		window.removeEventListener('mouseup', endDrag);
	}

	// Compute dynamic SVG size based on node positions
	$: svgWidth = Math.max(layout.width, ...layout.nodes.map(n => n.x + NODE_WIDTH)) + 100;
	$: svgHeight = Math.max(layout.height, ...layout.nodes.map(n => n.y + NODE_HEIGHT)) + 100;
</script>

<div class="editor-container">
	<div class="editor-header">
		<input class="pipeline-name-input" type="text" value={config.name} on:input={(e) => onUpdate({ ...config, name: e.currentTarget.value })} placeholder="Pipeline name..." />
		<div class="header-actions">
			<select class="template-select" bind:value={selectedTemplate} on:change={() => applyTemplate(selectedTemplate)}>
				<option value="">Apply template...</option>
				{#each templates as tpl}
					<option value={tpl.id}>{tpl.name}</option>
				{/each}
			</select>
			<button class="add-btn" on:click={addNode}>+ Add Node</button>
			<button class="save-btn" on:click={saveWithPositions}>Save</button>
		</div>
	</div>

	<div class="editor-body">
		<div class="graph-panel">
			<svg
				bind:this={svgElement}
				width={svgWidth}
				height={svgHeight}
				class="graph-svg"
				role="img"
			>
				<defs>
					<marker id="arrowhead" markerWidth="10" markerHeight="7" refX="9" refY="3.5" orient="auto">
						<polygon points="0 0, 10 3.5, 0 7" fill="#888" />
					</marker>
					<marker id="arrowhead-sel" markerWidth="10" markerHeight="7" refX="9" refY="3.5" orient="auto">
						<polygon points="0 0, 10 3.5, 0 7" fill="#3b82f6" />
					</marker>
					<marker id="arrowhead-feedback" markerWidth="10" markerHeight="7" refX="9" refY="3.5" orient="auto">
						<polygon points="0 0, 10 3.5, 0 7" fill="#ef4444" />
					</marker>
				</defs>

				{#each layout.edges as edge}
					<path
						d={pathD(edge.points)}
						fill="none"
						stroke={edgeColor(edge.edgeType, selectedEdgeIndex === edge.index)}
						stroke-width={selectedEdgeIndex === edge.index ? 2.5 : 2}
						stroke-dasharray={edgeDash(edge.edgeType)}
						marker-end={selectedEdgeIndex === edge.index ? 'url(#arrowhead-sel)' : edge.edgeType === 'feedback' ? 'url(#arrowhead-feedback)' : 'url(#arrowhead)'}
						class="edge"
						on:click={() => selectEdge(edge.index)}
						on:keydown={(e) => e.key === 'Enter' && selectEdge(edge.index)}
						role="button"
						tabindex="0"
					/>
				{/each}

				{#each layout.nodes as node}
					<g
						transform="translate({node.x - NODE_WIDTH/2}, {node.y - NODE_HEIGHT/2})"
						class="node"
						class:selected={selectedNodeId === node.id}
						class:dragging={draggingNodeId === node.id}
						on:mousedown={(e) => startDrag(e, node.id)}
						on:click={() => selectNode(node.id)}
						on:keydown={(e) => e.key === 'Enter' && selectNode(node.id)}
						role="button"
						tabindex="0"
					>
						<rect width={NODE_WIDTH} height={NODE_HEIGHT} rx="8" fill={nodeColors[node.type] || '#6b7280'} stroke={selectedNodeId === node.id ? '#fff' : 'none'} stroke-width="2" />
						<text x={NODE_WIDTH/2} y={NODE_HEIGHT/2 + 5} text-anchor="middle" fill="#fff" font-size="13" font-weight="500">{node.id}</text>
						{#if config.nodes.find(n => n.id === node.id)?.tools?.length}
						<title>Tools: {config.nodes.find(n => n.id === node.id)?.tools?.join(', ')}</title>
						<text x={NODE_WIDTH/2} y={NODE_HEIGHT + 14} text-anchor="middle" fill="#fbbf24" font-size="12">🔧</text>
						{/if}
					</g>
				{/each}
			</svg>
		</div>

		{#if sidebarOpen}
		<div class="side-panel">
			<div class="panel-header">
				<div class="sidebar-tabs">
					<button class:active={sidebarTab === 'properties'} on:click={() => sidebarTab = 'properties'}>Properties</button>
					<button class:active={sidebarTab === 'tools'} on:click={() => sidebarTab = 'tools'}>Tools</button>
				</div>
				<button class="done-btn" on:click={() => sidebarOpen = false}>Done</button>
			</div>

			{#if sidebarTab === 'tools'}
			<div class="tools-panel">
				<h4>Available Tools</h4>
				{#each availableTools as tool}
					<div class="tool-item">
						<strong>{tool.name}</strong>
						<p class="tool-desc">{tool.description}</p>
						<span class="tool-usage">Used by: {config.nodes.filter(n => n.tools?.includes(tool.name)).map(n => n.id).join(', ') || 'none'}</span>
					</div>
				{:else}
					<p class="no-tools-msg">No tools available. Set TAVILY_API_KEY to enable web search.</p>
				{/each}
			</div>
			{:else}
			<div class="edge-controls">
				<h4>Add Edge</h4>
				<div class="edge-form">
					<select bind:value={newEdgeFrom}>
						<option value="">from...</option>
						<option value="input">input</option>
						{#each config.nodes as n}<option value={n.id}>{n.id}</option>{/each}
					</select>
					<span>→</span>
					<select bind:value={newEdgeTo}>
						<option value="">to...</option>
						{#each config.nodes as n}<option value={n.id}>{n.id}</option>{/each}
						<option value="output">output</option>
					</select>
					<select bind:value={newEdgeType}>
						{#each edgeTypes as t}<option value={t}>{t}</option>{/each}
					</select>
					<button on:click={addEdge} disabled={!newEdgeFrom || !newEdgeTo}>+</button>
				</div>
			</div>

			{#if selectedNode}
				<div class="properties">
					<h4>Node: {selectedNode.id}</h4>
					<label><span>ID</span><input type="text" value={selectedNode.id} on:change={(e) => updateNodeField(selectedNode.id, 'id', e.currentTarget.value)} /></label>
					<label><span>Type</span>
						<select value={selectedNode.node_type} on:change={(e) => updateNodeField(selectedNode.id, 'node_type', e.currentTarget.value)}>
							{#each nodeTypes as t}<option value={t}>{t}</option>{/each}
						</select>
					</label>
					<label><span>Model</span>
						<select value={selectedNode.model || ''} on:change={(e) => updateNodeField(selectedNode.id, 'model', e.currentTarget.value || null)}>
							<option value="">Default</option>
							{#each models as m}<option value={m.id}>{m.name}</option>{/each}
						</select>
					</label>
					<label><span>Prompt</span><textarea value={selectedNode.prompt || ''} on:input={(e) => updateNodeField(selectedNode.id, 'prompt', e.currentTarget.value || null)} rows="6"></textarea></label>
					<label><span>Tools</span>
						{#if availableTools.length > 0}
						<div class="tools-selector">
							{#each availableTools as tool}
								<label class="tool-checkbox" title={tool.description}>
									<input type="checkbox" checked={(selectedNode.tools || []).includes(tool.name)} on:change={() => toggleNodeTool(selectedNode.id, tool.name)} />
									<span class="tool-name">{tool.name}</span>
								</label>
							{/each}
						</div>
						{:else}
						<div class="no-tools-msg">No tools available. Set TAVILY_API_KEY to enable web search.</div>
						{/if}
					</label>
					<button class="delete-btn" on:click={() => removeNode(selectedNode.id)}>Delete Node</button>
				</div>
			{:else if selectedEdge}
				<div class="properties">
					<h4>Edge</h4>
					<label><span>From</span><input disabled value={Array.isArray(selectedEdge.from) ? selectedEdge.from.join(', ') : selectedEdge.from} /></label>
					<label><span>To</span><input disabled value={Array.isArray(selectedEdge.to) ? selectedEdge.to.join(', ') : selectedEdge.to} /></label>
					<label><span>Type</span>
						<select value={selectedEdge.edge_type || 'direct'} on:change={(e) => selectedEdgeIndex !== null && updateEdgeType(selectedEdgeIndex, e.currentTarget.value)}>
							{#each edgeTypes as t}<option value={t}>{t}</option>{/each}
						</select>
					</label>
					<button class="delete-btn" on:click={() => selectedEdgeIndex !== null && removeEdge(selectedEdgeIndex)}>Delete Edge</button>
				</div>
			{/if}
			{/if}
		</div>
		{:else}
		<button class="open-panel-btn" on:click={() => sidebarOpen = true}>☰</button>
		{/if}
	</div>
</div>

<style>
	.editor-container { position: fixed; inset: 0; background: var(--bg, #1a1a1a); z-index: 1000; display: flex; flex-direction: column; }
	.editor-header { display: flex; justify-content: space-between; align-items: center; padding: 0.75rem 1rem; border-bottom: 1px solid var(--border, #333); }
	.pipeline-name-input { flex: 1; min-width: 0; font-size: 1.125rem; font-weight: 600; background: transparent; border: 1px solid transparent; border-radius: 4px; color: var(--text, #fff); padding: 0.25rem 0.5rem; }
	.pipeline-name-input:hover, .pipeline-name-input:focus { border-color: var(--border, #333); outline: none; }
	.header-actions { display: flex; gap: 0.5rem; }
	.template-select, .add-btn, .save-btn { padding: 0.4rem 0.75rem; border-radius: 4px; border: 1px solid var(--border, #333); background: var(--bg-secondary, #2a2a2a); color: var(--text, #fff); cursor: pointer; font-size: 0.875rem; }
	.add-btn:hover { background: #3b82f6; }
	.save-btn:hover { background: #22c55e; }

	.editor-body { flex: 1; display: flex; overflow: hidden; position: relative; }
	.graph-panel { flex: 1; overflow: auto; background: linear-gradient(90deg, var(--border, #333) 1px, transparent 1px) 0 0 / 20px 20px, linear-gradient(var(--border, #333) 1px, transparent 1px) 0 0 / 20px 20px; padding: 1rem; }
	.graph-svg { display: block; }

	.node { cursor: grab; }
	.node:hover rect { filter: brightness(1.15); }
	.node.selected rect { filter: brightness(1.2); }
	.node.dragging { cursor: grabbing; }
	.graph-svg { user-select: none; }
	.edge { cursor: pointer; }
	.edge:hover { stroke-width: 3; }

	.side-panel { width: 420px; border-left: 1px solid var(--border, #333); background: var(--bg-secondary, #2a2a2a); display: flex; flex-direction: column; overflow-y: auto; }
	.panel-header { padding: 0.75rem 1rem; border-bottom: 1px solid var(--border, #333); display: flex; gap: 0.5rem; align-items: center; }
	.done-btn { padding: 0.4rem 0.75rem; border-radius: 4px; border: 1px solid var(--border, #333); background: #3b82f6; color: #fff; cursor: pointer; font-size: 0.75rem; font-weight: 500; white-space: nowrap; }
	.done-btn:hover { background: #2563eb; }
	.open-panel-btn { position: absolute; right: 1rem; top: 50%; transform: translateY(-50%); padding: 0.5rem 0.75rem; border-radius: 4px; border: 1px solid var(--border, #333); background: var(--bg-secondary, #2a2a2a); color: var(--text, #fff); cursor: pointer; font-size: 1.25rem; }
	.open-panel-btn:hover { background: #3b82f6; }
	.edge-controls { padding: 1rem; border-bottom: 1px solid var(--border, #333); }
	.edge-controls h4 { margin: 0 0 0.5rem; font-size: 0.8rem; color: var(--text-secondary, #888); text-transform: uppercase; }
	.edge-form { display: flex; flex-wrap: wrap; gap: 0.3rem; align-items: center; }
	.edge-form select, .edge-form button { padding: 0.3rem; border-radius: 4px; border: 1px solid var(--border, #333); background: var(--bg, #1a1a1a); color: var(--text, #fff); font-size: 0.75rem; }
	.edge-form button:disabled { opacity: 0.4; cursor: not-allowed; }

	.properties { padding: 1rem; }
	.properties h4 { margin: 0 0 1rem; font-size: 0.9rem; }
	.properties label { display: flex; flex-direction: column; gap: 0.25rem; margin-bottom: 0.75rem; }
	.properties label span { font-size: 0.7rem; color: var(--text-secondary, #888); text-transform: uppercase; }
	.properties input, .properties select, .properties textarea { padding: 0.5rem; border-radius: 4px; border: 1px solid var(--border, #333); background: var(--bg, #1a1a1a); color: var(--text, #fff); font-size: 0.875rem; }
	.properties input:disabled { opacity: 0.6; }
	.properties textarea { resize: vertical; min-height: 80px; }
	.delete-btn { width: 100%; padding: 0.5rem; border-radius: 4px; border: 1px solid #ef4444; background: transparent; color: #ef4444; cursor: pointer; margin-top: 0.5rem; }
	.delete-btn:hover { background: #ef4444; color: white; }

	.tools-selector { display: flex; flex-direction: column; gap: 0.25rem; padding: 0.5rem; border-radius: 4px; border: 1px solid var(--border, #333); background: var(--bg, #1a1a1a); max-height: 150px; overflow-y: auto; }
	.tool-checkbox { display: flex; align-items: center; gap: 0.5rem; cursor: pointer; padding: 0.25rem; border-radius: 4px; }
	.tool-checkbox:hover { background: var(--bg-secondary, #2a2a2a); }
	.tool-checkbox input[type="checkbox"] { width: 1rem; height: 1rem; cursor: pointer; }
	.tool-name { font-size: 0.875rem; color: var(--text, #fff); }
	.no-tools-msg { font-size: 0.75rem; color: var(--text-secondary, #888); padding: 0.5rem; border-radius: 4px; border: 1px solid var(--border, #333); background: var(--bg, #1a1a1a); }

	.sidebar-tabs { display: flex; gap: 0.25rem; }
	.sidebar-tabs button { flex: 1; padding: 0.4rem 0.5rem; border-radius: 4px; border: 1px solid var(--border, #333); background: var(--bg, #1a1a1a); color: var(--text-secondary, #888); cursor: pointer; font-size: 0.75rem; }
	.sidebar-tabs button.active { background: var(--bg-secondary, #2a2a2a); color: var(--text, #fff); border-color: #3b82f6; }
	.sidebar-tabs button:hover:not(.active) { background: var(--bg-secondary, #2a2a2a); }

	.tools-panel { padding: 1rem; }
	.tools-panel h4 { margin: 0 0 0.75rem; font-size: 0.8rem; color: var(--text-secondary, #888); text-transform: uppercase; }
	.tool-item { padding: 0.75rem; margin-bottom: 0.5rem; border-radius: 4px; border: 1px solid var(--border, #333); background: var(--bg, #1a1a1a); }
	.tool-item strong { font-size: 0.875rem; color: var(--text, #fff); }
	.tool-desc { font-size: 0.75rem; color: var(--text-secondary, #888); margin: 0.25rem 0; }
	.tool-usage { font-size: 0.7rem; color: #3b82f6; }
</style>
