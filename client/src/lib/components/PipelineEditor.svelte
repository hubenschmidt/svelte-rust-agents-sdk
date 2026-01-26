<script lang="ts">
	import { onMount } from 'svelte';
	import type { PipelineInfo, NodeInfo, EdgeInfo, ModelConfig } from '$lib/types';

	export let config: PipelineInfo;
	export let models: ModelConfig[];
	export let templates: PipelineInfo[] = [];
	export let onUpdate: (config: PipelineInfo) => void;
	export let onClose: () => void;
	export let onSave: (config: PipelineInfo) => void = () => {};

	const nodeTypes = ['llm', 'worker', 'coordinator', 'aggregator', 'orchestrator', 'synthesizer', 'router', 'gate', 'evaluator'];
	const edgeTypes = ['direct', 'conditional', 'dynamic'];

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
	let newEdgeFrom = '';
	let newEdgeTo = '';
	let newEdgeType = 'direct';
	let dagreReady = false;
	// eslint-disable-next-line @typescript-eslint/no-explicit-any
	let dagreLib: any = null;

	// Drag state
	let draggingNodeId: string | null = null;
	let dragOffset = { x: 0, y: 0 };
	let nodePositionOverrides: Record<string, { x: number; y: number }> = {};

	// Sidebar state
	let sidebarOpen = true;

	$: selectedNode = selectedNodeId ? config.nodes.find(n => n.id === selectedNodeId) : null;
	$: selectedEdge = selectedEdgeIndex !== null ? config.edges[selectedEdgeIndex] : null;

	type LayoutNode = { id: string; x: number; y: number; type: string };
	type LayoutEdge = { from: string; to: string; points: {x: number; y: number}[]; edgeType: string; index: number };
	type Layout = { nodes: LayoutNode[]; edges: LayoutEdge[]; width: number; height: number };

	let layout: Layout = { nodes: [], edges: [], width: 500, height: 400 };

	onMount(async () => {
		try {
			const mod = await import('@dagrejs/dagre');
			dagreLib = mod.default || mod;
			dagreReady = true;
		} catch (e) {
			console.error('Failed to load dagre:', e);
			dagreReady = true; // Use fallback
		}
	});

	// Reactively rebuild layout and apply position overrides
	$: if (dagreReady) {
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

		if (dagreLib) {
			try {
				const g = new dagreLib.graphlib.Graph();
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

				dagreLib.layout(g);

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
			}
		}

		// Fallback: grid layout with simple straight edges
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

		// Compute simple straight-line edges
		const fallbackEdges: LayoutEdge[] = [];
		edgeList.forEach((e, idx) => {
			const froms = Array.isArray(e.from) ? e.from : [e.from];
			const tos = Array.isArray(e.to) ? e.to : [e.to];
			froms.forEach(f => {
				tos.forEach(t => {
					const fromPos = nodePositions[f];
					const toPos = nodePositions[t];
					if (fromPos && toPos) {
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
					}
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
		return '';
	}

	function applyTemplate(templateId: string) {
		const tpl = templates.find(t => t.id === templateId);
		if (!tpl) return;
		onUpdate({ ...config, nodes: structuredClone(tpl.nodes), edges: structuredClone(tpl.edges) });
	}

	function selectNode(id: string) {
		selectedNodeId = id;
		selectedEdgeIndex = null;
	}

	function selectEdge(idx: number) {
		selectedEdgeIndex = idx;
		selectedNodeId = null;
	}

	function updateNodeField(nodeId: string, field: string, value: string | null) {
		const nodes = config.nodes.map(n => n.id === nodeId ? { ...n, [field]: value } : n);
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
			<select class="template-select" on:change={(e) => { applyTemplate(e.currentTarget.value); e.currentTarget.value = ''; }}>
				<option value="">Apply template...</option>
				{#each templates as tpl}
					<option value={tpl.id}>{tpl.name}</option>
				{/each}
			</select>
			<button class="add-btn" on:click={addNode}>+ Add Node</button>
			<button class="save-btn" on:click={() => onSave(config)}>Save</button>
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
				</defs>

				{#each layout.edges as edge}
					<path
						d={pathD(edge.points)}
						fill="none"
						stroke={selectedEdgeIndex === edge.index ? '#3b82f6' : '#888'}
						stroke-width={selectedEdgeIndex === edge.index ? 2.5 : 2}
						stroke-dasharray={edgeDash(edge.edgeType)}
						marker-end={selectedEdgeIndex === edge.index ? 'url(#arrowhead-sel)' : 'url(#arrowhead)'}
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
					</g>
				{/each}
			</svg>
		</div>

		{#if sidebarOpen}
		<div class="side-panel">
			<div class="panel-header">
				<button class="done-btn" on:click={() => sidebarOpen = false}>Done</button>
			</div>
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
		</div>
		{:else}
		<button class="open-panel-btn" on:click={() => sidebarOpen = true}>☰</button>
		{/if}
	</div>
</div>

<style>
	.editor-container { position: fixed; inset: 0; background: var(--bg, #1a1a1a); z-index: 1000; display: flex; flex-direction: column; }
	.editor-header { display: flex; justify-content: space-between; align-items: center; padding: 0.75rem 1rem; border-bottom: 1px solid var(--border, #333); }
	.pipeline-name-input { font-size: 1.125rem; font-weight: 600; background: transparent; border: 1px solid transparent; border-radius: 4px; color: var(--text, #fff); padding: 0.25rem 0.5rem; }
	.pipeline-name-input:hover, .pipeline-name-input:focus { border-color: var(--border, #333); outline: none; }
	.header-actions { display: flex; gap: 0.5rem; }
	.template-select, .add-btn, .close-btn, .save-btn { padding: 0.4rem 0.75rem; border-radius: 4px; border: 1px solid var(--border, #333); background: var(--bg-secondary, #2a2a2a); color: var(--text, #fff); cursor: pointer; font-size: 0.875rem; }
	.add-btn:hover { background: #3b82f6; }
	.save-btn:hover { background: #22c55e; }
	.close-btn:hover { background: var(--border, #333); }

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

	.side-panel { width: 340px; border-left: 1px solid var(--border, #333); background: var(--bg-secondary, #2a2a2a); display: flex; flex-direction: column; overflow-y: auto; }
	.panel-header { padding: 1rem; border-bottom: 1px solid var(--border, #333); }
	.done-btn { width: 100%; padding: 0.5rem; border-radius: 4px; border: 1px solid var(--border, #333); background: #3b82f6; color: #fff; cursor: pointer; font-size: 0.875rem; font-weight: 500; }
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
</style>
