export type ToolSchema = {
	name: string;
	description: string;
	parameters: Record<string, unknown>;
};

export type WsMetadata = {
	input_tokens: number;
	output_tokens: number;
	elapsed_ms: number;
	load_duration_ms?: number;
	prompt_eval_ms?: number;
	eval_ms?: number;
	tokens_per_sec?: number;
};

export type ModelConfig = {
	id: string;
	name: string;
	model: string;
	api_base: string | null;
};

export type NodeInfo = {
	id: string;
	node_type: string;
	model: string | null;
	prompt: string | null;
	tools?: string[];
	x?: number;
	y?: number;
};

export type EdgeInfo = {
	from: string | string[];
	to: string | string[];
	edge_type?: string;
};

export type PipelineInfo = {
	id: string;
	name: string;
	description: string;
	nodes: NodeInfo[];
	edges: EdgeInfo[];
	layout?: Record<string, { x: number; y: number }>; // positions for input/output virtual nodes
};

export type RuntimeNodeConfig = {
	id: string;
	type: string;
	model?: string | null;
	prompt?: string | null;
	tools?: string[];
};

export type RuntimeEdgeConfig = {
	from: string | string[];
	to: string | string[];
	edge_type?: string;
};

export type RuntimePipelineConfig = {
	nodes: RuntimeNodeConfig[];
	edges: RuntimeEdgeConfig[];
};

export type ChatMsg = {
	user: 'User' | 'Bot';
	msg: string;
	streaming?: boolean;
	metadata?: WsMetadata;
};

export type HistoryMessage = {
	role: 'user' | 'assistant';
	content: string;
};

export type WsPayload = {
	uuid?: string;
	message?: string;
	model_id?: string;
	pipeline_id?: string;
	node_models?: Record<string, string>;
	pipeline_config?: RuntimePipelineConfig;
	init?: boolean;
	verbose?: boolean;
	wake_model_id?: string;
	unload_model_id?: string;
	history?: HistoryMessage[];
	system_prompt?: string;
};

export type WsResponse = {
	on_chat_model_stream?: string;
	on_chat_model_end?: boolean;
	metadata?: WsMetadata;
	models?: ModelConfig[];
	templates?: PipelineInfo[];
	configs?: PipelineInfo[];
	model_status?: string;
};
