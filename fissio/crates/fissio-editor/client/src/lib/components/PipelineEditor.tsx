import { createSignal, createMemo, createEffect, onMount, onCleanup, For, Show } from 'solid-js';
import type { PipelineInfo, NodeInfo, EdgeInfo, ModelConfig, ToolSchema } from '../types';

type Props = {
  config: PipelineInfo;
  models: ModelConfig[];
  templates?: PipelineInfo[];
  availableTools?: ToolSchema[];
  onUpdate: (config: PipelineInfo) => void;
  onSave?: (config: PipelineInfo) => void;
};

const NODE_TYPES = ['llm', 'worker', 'coordinator', 'aggregator', 'orchestrator', 'synthesizer', 'router', 'gate', 'evaluator'];
const EDGE_TYPES = ['direct', 'conditional', 'dynamic', 'feedback'];

const NODE_COLORS: Record<string, string> = {
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

type LayoutNode = { id: string; x: number; y: number; type: string };
type LayoutEdge = { from: string; to: string; points: { x: number; y: number }[]; edgeType: string; index: number };
type Layout = { nodes: LayoutNode[]; edges: LayoutEdge[]; width: number; height: number };

// eslint-disable-next-line @typescript-eslint/no-explicit-any
let dagre: any = null;

export default function PipelineEditor(props: Props) {
  const [dagreReady, setDagreReady] = createSignal(false);
  const [selectedNodeId, setSelectedNodeId] = createSignal<string | null>(null);
  const [selectedEdgeIndex, setSelectedEdgeIndex] = createSignal<number | null>(null);
  const [selectedTemplate, setSelectedTemplate] = createSignal('');
  const [newEdgeFrom, setNewEdgeFrom] = createSignal('');
  const [newEdgeTo, setNewEdgeTo] = createSignal('');
  const [newEdgeType, setNewEdgeType] = createSignal('direct');
  const [draggingNodeId, setDraggingNodeId] = createSignal<string | null>(null);
  const [dragOffset, setDragOffset] = createSignal({ x: 0, y: 0 });
  const [nodePositionOverrides, setNodePositionOverrides] = createSignal<Record<string, { x: number; y: number }>>({});
  const [sidebarOpen, setSidebarOpen] = createSignal(true);
  const [sidebarTab, setSidebarTab] = createSignal<'properties' | 'tools'>('properties');
  const [layout, setLayout] = createSignal<Layout>({ nodes: [], edges: [], width: 500, height: 400 });

  let svgElement: SVGSVGElement | undefined;

  const selectedNode = createMemo(() => {
    const id = selectedNodeId();
    return id ? props.config.nodes.find(n => n.id === id) : null;
  });

  const selectedEdge = createMemo(() => {
    const idx = selectedEdgeIndex();
    return idx !== null ? props.config.edges[idx] : null;
  });

  const svgWidth = createMemo(() => {
    const l = layout();
    return Math.max(l.width, ...l.nodes.map(n => n.x + NODE_WIDTH)) + 100;
  });

  const svgHeight = createMemo(() => {
    const l = layout();
    return Math.max(l.height, ...l.nodes.map(n => n.y + NODE_HEIGHT)) + 100;
  });

  const loadPositionsFromConfig = () => {
    const positions: Record<string, { x: number; y: number }> = {};
    props.config.nodes.forEach(n => {
      if (n.x !== undefined && n.y !== undefined) {
        positions[n.id] = { x: n.x, y: n.y };
      }
    });
    if (props.config.layout) {
      Object.entries(props.config.layout).forEach(([id, pos]) => {
        positions[id] = pos;
      });
    }
    setNodePositionOverrides(positions);
  };

  const recomputeEdges = (nodes: LayoutNode[], edges: EdgeInfo[]): LayoutEdge[] => {
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
          if (!fromPos || !toPos) return;
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
        });
      });
    });
    return result;
  };

  const computeFallbackLayout = (nodeList: NodeInfo[], edgeList: EdgeInfo[], hasInput: boolean, hasOutput: boolean): Layout => {
    const nodePositions: Record<string, { x: number; y: number }> = {};
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

    return { nodes: allNodes, edges: fallbackEdges, width: cols * 180 + 100, height: Math.ceil((nodeList.length + 2) / cols) * 100 + 100 };
  };

  const computeLayout = (nodes: NodeInfo[], edges: EdgeInfo[]): Layout => {
    const nodeList = nodes || [];
    const edgeList = edges || [];
    const hasInput = edgeList.some(e => (Array.isArray(e.from) ? e.from : [e.from]).includes('input'));
    const hasOutput = edgeList.some(e => (Array.isArray(e.to) ? e.to : [e.to]).includes('output'));

    if (!dagre) return computeFallbackLayout(nodeList, edgeList, hasInput, hasOutput);

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
            if (edge?.points) {
              layoutEdges.push({ from: f, to: t, points: edge.points, edgeType: e.edge_type || 'direct', index: idx });
            }
          });
        });
      });

      const info = g.graph();
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
  };

  const rebuildLayout = () => {
    const base = computeLayout(props.config.nodes, props.config.edges);
    const overrides = nodePositionOverrides();
    base.nodes = base.nodes.map(n => {
      const override = overrides[n.id];
      return override ? { ...n, x: override.x, y: override.y } : n;
    });
    if (Object.keys(overrides).length > 0) {
      base.edges = recomputeEdges(base.nodes, props.config.edges);
    }
    setLayout(base);
  };

  onMount(async () => {
    loadPositionsFromConfig();
    // @ts-ignore
    const mod = await import('https://esm.sh/@dagrejs/dagre@1.1.4');
    dagre = mod.default;
    setDagreReady(true);
  });

  createEffect(() => {
    if (!dagreReady()) return;
    props.config.nodes;
    props.config.edges;
    nodePositionOverrides();
    rebuildLayout();
  });

  const pathD = (points: { x: number; y: number }[]): string => {
    if (points.length < 2) return '';
    return `M ${points[0].x} ${points[0].y} ` + points.slice(1).map(p => `L ${p.x} ${p.y}`).join(' ');
  };

  const EDGE_DASH: Record<string, string> = {
    conditional: '8,4',
    dynamic: '2,4',
    feedback: '6,3,2,3'
  };

  const edgeDash = (type: string) => EDGE_DASH[type] ?? '';

  const edgeColor = (type: string, selected: boolean): string => {
    if (selected) return '#3b82f6';
    if (type === 'feedback') return '#ef4444';
    return '#888';
  };

  const applyTemplate = (templateId: string) => {
    const tpl = props.templates?.find(t => t.id === templateId);
    if (!tpl) return;
    props.onUpdate({ ...props.config, nodes: structuredClone(tpl.nodes), edges: structuredClone(tpl.edges) });
  };

  const selectNode = (id: string) => {
    setSelectedNodeId(id);
    setSelectedEdgeIndex(null);
    setSidebarOpen(true);
  };

  const selectEdge = (idx: number) => {
    setSelectedEdgeIndex(idx);
    setSelectedNodeId(null);
    setSidebarOpen(true);
  };

  const updateNodeField = (nodeId: string, field: string, value: string | null) => {
    const nodes = props.config.nodes.map(n => n.id === nodeId ? { ...n, [field]: value } : n);
    props.onUpdate({ ...props.config, nodes });
  };

  const toggleNodeTool = (nodeId: string, toolName: string) => {
    const nodes = props.config.nodes.map(n => {
      if (n.id !== nodeId) return n;
      const currentTools = n.tools || [];
      const hasIt = currentTools.includes(toolName);
      const newTools = hasIt ? currentTools.filter(t => t !== toolName) : [...currentTools, toolName];
      return { ...n, tools: newTools.length > 0 ? newTools : undefined };
    });
    props.onUpdate({ ...props.config, nodes });
  };

  const updateEdgeType = (idx: number, type: string) => {
    const edges = props.config.edges.map((e, i) => i === idx ? { ...e, edge_type: type } : e);
    props.onUpdate({ ...props.config, edges });
  };

  const addNode = () => {
    const id = `node_${props.config.nodes.length + 1}`;
    props.onUpdate({ ...props.config, nodes: [...props.config.nodes, { id, node_type: 'worker', model: null, prompt: 'You are a helpful assistant.' }] });
    setSelectedNodeId(id);
  };

  const removeNode = (id: string) => {
    const nodes = props.config.nodes.filter(n => n.id !== id);
    const edges = props.config.edges.filter(e => {
      const f = Array.isArray(e.from) ? e.from : [e.from];
      const t = Array.isArray(e.to) ? e.to : [e.to];
      return !f.includes(id) && !t.includes(id);
    });
    if (selectedNodeId() === id) setSelectedNodeId(null);
    props.onUpdate({ ...props.config, nodes, edges });
  };

  const addEdge = () => {
    if (!newEdgeFrom() || !newEdgeTo()) return;
    props.onUpdate({ ...props.config, edges: [...props.config.edges, { from: newEdgeFrom(), to: newEdgeTo(), edge_type: newEdgeType() }] });
    setNewEdgeFrom('');
    setNewEdgeTo('');
  };

  const removeEdge = (idx: number) => {
    const edges = props.config.edges.filter((_, i) => i !== idx);
    if (selectedEdgeIndex() === idx) setSelectedEdgeIndex(null);
    props.onUpdate({ ...props.config, edges });
  };

  const saveWithPositions = () => {
    const overrides = nodePositionOverrides();
    const nodesWithPositions = props.config.nodes.map(n => {
      const pos = overrides[n.id];
      return pos ? { ...n, x: pos.x, y: pos.y } : n;
    });

    const layoutPositions: Record<string, { x: number; y: number }> = {
      ...(overrides['input'] && { input: overrides['input'] }),
      ...(overrides['output'] && { output: overrides['output'] })
    };

    const configWithPositions = {
      ...props.config,
      nodes: nodesWithPositions,
      layout: Object.keys(layoutPositions).length > 0 ? layoutPositions : undefined
    };

    props.onSave?.(configWithPositions);
  };

  const startDrag = (e: MouseEvent, nodeId: string) => {
    e.preventDefault();
    e.stopPropagation();
    setDraggingNodeId(nodeId);
    const node = layout().nodes.find(n => n.id === nodeId);
    if (!node || !svgElement) return;

    const pt = svgElement.createSVGPoint();
    pt.x = e.clientX;
    pt.y = e.clientY;
    const ctm = svgElement.getScreenCTM();
    if (!ctm) return;

    const svgP = pt.matrixTransform(ctm.inverse());
    setDragOffset({ x: svgP.x - node.x, y: svgP.y - node.y });
    window.addEventListener('mousemove', onDrag);
    window.addEventListener('mouseup', endDrag);
  };

  const onDrag = (e: MouseEvent) => {
    if (!draggingNodeId() || !svgElement) return;
    const pt = svgElement.createSVGPoint();
    pt.x = e.clientX;
    pt.y = e.clientY;
    const ctm = svgElement.getScreenCTM();
    if (!ctm) return;

    const svgP = pt.matrixTransform(ctm.inverse());
    const offset = dragOffset();
    setNodePositionOverrides({
      ...nodePositionOverrides(),
      [draggingNodeId()!]: { x: svgP.x - offset.x, y: svgP.y - offset.y }
    });
  };

  const endDrag = () => {
    setDraggingNodeId(null);
    window.removeEventListener('mousemove', onDrag);
    window.removeEventListener('mouseup', endDrag);
  };

  onCleanup(() => {
    window.removeEventListener('mousemove', onDrag);
    window.removeEventListener('mouseup', endDrag);
  });

  const getNodeTools = (nodeId: string) => props.config.nodes.find(n => n.id === nodeId)?.tools;

  return (
    <div class="editor-container">
      <div class="editor-header">
        <input
          class="pipeline-name-input"
          type="text"
          value={props.config.name}
          onInput={(e) => props.onUpdate({ ...props.config, name: e.currentTarget.value })}
          placeholder="Pipeline name..."
        />
        <div class="header-actions">
          <select
            class="template-select"
            value={selectedTemplate()}
            onChange={(e) => { setSelectedTemplate(e.currentTarget.value); applyTemplate(e.currentTarget.value); }}
          >
            <option value="">Apply template...</option>
            <For each={props.templates}>{(tpl) => <option value={tpl.id}>{tpl.name}</option>}</For>
          </select>
          <button class="add-btn" onClick={addNode}>+ Add Node</button>
          <button class="save-btn" onClick={saveWithPositions}>Save</button>
        </div>
      </div>

      <div class="editor-body">
        <div class="graph-panel">
          <svg ref={svgElement} width={svgWidth()} height={svgHeight()} class="graph-svg" role="img">
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

            <For each={layout().edges}>{(edge) => (
              <path
                d={pathD(edge.points)}
                fill="none"
                stroke={edgeColor(edge.edgeType, selectedEdgeIndex() === edge.index)}
                stroke-width={selectedEdgeIndex() === edge.index ? 2.5 : 2}
                stroke-dasharray={edgeDash(edge.edgeType)}
                marker-end={selectedEdgeIndex() === edge.index ? 'url(#arrowhead-sel)' : edge.edgeType === 'feedback' ? 'url(#arrowhead-feedback)' : 'url(#arrowhead)'}
                class="edge"
                onClick={() => selectEdge(edge.index)}
                onKeyDown={(e) => e.key === 'Enter' && selectEdge(edge.index)}
                role="button"
                tabIndex={0}
              />
            )}</For>

            <For each={layout().nodes}>{(node) => (
              <g
                transform={`translate(${node.x - NODE_WIDTH / 2}, ${node.y - NODE_HEIGHT / 2})`}
                class="node"
                classList={{ selected: selectedNodeId() === node.id, dragging: draggingNodeId() === node.id }}
                onMouseDown={(e) => startDrag(e, node.id)}
                onClick={() => selectNode(node.id)}
                onKeyDown={(e) => e.key === 'Enter' && selectNode(node.id)}
                role="button"
                tabIndex={0}
              >
                <rect
                  width={NODE_WIDTH}
                  height={NODE_HEIGHT}
                  rx="8"
                  fill={NODE_COLORS[node.type] || '#6b7280'}
                  stroke={selectedNodeId() === node.id ? '#fff' : 'none'}
                  stroke-width="2"
                />
                <text x={NODE_WIDTH / 2} y={NODE_HEIGHT / 2 + 5} text-anchor="middle" fill="#fff" font-size="13" font-weight="500">{node.id}</text>
                <Show when={getNodeTools(node.id)?.length}>
                  <title>Tools: {getNodeTools(node.id)?.join(', ')}</title>
                  <text x={NODE_WIDTH / 2} y={NODE_HEIGHT + 14} text-anchor="middle" fill="#fbbf24" font-size="12">🔧</text>
                </Show>
              </g>
            )}</For>
          </svg>
        </div>

        <Show when={sidebarOpen()} fallback={<button class="open-panel-btn" onClick={() => setSidebarOpen(true)}>☰</button>}>
          <div class="side-panel">
            <div class="panel-header">
              <div class="sidebar-tabs">
                <button classList={{ active: sidebarTab() === 'properties' }} onClick={() => setSidebarTab('properties')}>Properties</button>
                <button classList={{ active: sidebarTab() === 'tools' }} onClick={() => setSidebarTab('tools')}>Tools</button>
              </div>
              <button class="done-btn" onClick={() => setSidebarOpen(false)}>Done</button>
            </div>

            <Show when={sidebarTab() === 'tools'}>
              <div class="tools-panel">
                <h4>Available Tools</h4>
                <Show when={(props.availableTools?.length ?? 0) > 0} fallback={<p class="no-tools-msg">No tools available. Set TAVILY_API_KEY to enable web search.</p>}>
                  <For each={props.availableTools}>{(tool) => (
                    <div class="tool-item">
                      <strong>{tool.name}</strong>
                      <p class="tool-desc">{tool.description}</p>
                      <span class="tool-usage">Used by: {props.config.nodes.filter(n => n.tools?.includes(tool.name)).map(n => n.id).join(', ') || 'none'}</span>
                    </div>
                  )}</For>
                </Show>
              </div>
            </Show>

            <Show when={sidebarTab() === 'properties'}>
              <div class="edge-controls">
                <h4>Add Edge</h4>
                <div class="edge-form">
                  <select value={newEdgeFrom()} onChange={(e) => setNewEdgeFrom(e.currentTarget.value)}>
                    <option value="">from...</option>
                    <option value="input">input</option>
                    <For each={props.config.nodes}>{(n) => <option value={n.id}>{n.id}</option>}</For>
                  </select>
                  <span>→</span>
                  <select value={newEdgeTo()} onChange={(e) => setNewEdgeTo(e.currentTarget.value)}>
                    <option value="">to...</option>
                    <For each={props.config.nodes}>{(n) => <option value={n.id}>{n.id}</option>}</For>
                    <option value="output">output</option>
                  </select>
                  <select value={newEdgeType()} onChange={(e) => setNewEdgeType(e.currentTarget.value)}>
                    <For each={EDGE_TYPES}>{(t) => <option value={t}>{t}</option>}</For>
                  </select>
                  <button onClick={addEdge} disabled={!newEdgeFrom() || !newEdgeTo()}>+</button>
                </div>
              </div>

              <Show when={selectedNode()}>
                {(node) => (
                  <div class="properties">
                    <h4>Node: {node().id}</h4>
                    <label>
                      <span>ID</span>
                      <input type="text" value={node().id} onChange={(e) => updateNodeField(node().id, 'id', e.currentTarget.value)} />
                    </label>
                    <label>
                      <span>Type</span>
                      <select value={node().node_type} onChange={(e) => updateNodeField(node().id, 'node_type', e.currentTarget.value)}>
                        <For each={NODE_TYPES}>{(t) => <option value={t}>{t}</option>}</For>
                      </select>
                    </label>
                    <label>
                      <span>Model</span>
                      <select value={node().model || ''} onChange={(e) => updateNodeField(node().id, 'model', e.currentTarget.value || null)}>
                        <option value="">Default</option>
                        <For each={props.models}>{(m) => <option value={m.id}>{m.name}</option>}</For>
                      </select>
                    </label>
                    <label>
                      <span>Prompt</span>
                      <textarea value={node().prompt || ''} onInput={(e) => updateNodeField(node().id, 'prompt', e.currentTarget.value || null)} rows="6" />
                    </label>
                    <label>
                      <span>Tools</span>
                      <Show when={(props.availableTools?.length ?? 0) > 0} fallback={<div class="no-tools-msg">No tools available. Set TAVILY_API_KEY to enable web search.</div>}>
                        <div class="tools-selector">
                          <For each={props.availableTools}>{(tool) => (
                            <label class="tool-checkbox" title={tool.description}>
                              <input
                                type="checkbox"
                                checked={(node().tools || []).includes(tool.name)}
                                onChange={() => toggleNodeTool(node().id, tool.name)}
                              />
                              <span class="tool-name">{tool.name}</span>
                            </label>
                          )}</For>
                        </div>
                      </Show>
                    </label>
                    <button class="delete-btn" onClick={() => removeNode(node().id)}>Delete Node</button>
                  </div>
                )}
              </Show>

              <Show when={selectedEdge()}>
                {(edge) => (
                  <div class="properties">
                    <h4>Edge</h4>
                    <label>
                      <span>From</span>
                      <input disabled value={Array.isArray(edge().from) ? edge().from.join(', ') : edge().from} />
                    </label>
                    <label>
                      <span>To</span>
                      <input disabled value={Array.isArray(edge().to) ? edge().to.join(', ') : edge().to} />
                    </label>
                    <label>
                      <span>Type</span>
                      <select value={edge().edge_type || 'direct'} onChange={(e) => selectedEdgeIndex() !== null && updateEdgeType(selectedEdgeIndex()!, e.currentTarget.value)}>
                        <For each={EDGE_TYPES}>{(t) => <option value={t}>{t}</option>}</For>
                      </select>
                    </label>
                    <button class="delete-btn" onClick={() => selectedEdgeIndex() !== null && removeEdge(selectedEdgeIndex()!)}>Delete Edge</button>
                  </div>
                )}
              </Show>
            </Show>
          </div>
        </Show>
      </div>
    </div>
  );
}
