export type WsMetadata = {
	input_tokens: number;
	output_tokens: number;
	elapsed_ms: number;
};

export type ChatMsg = {
	user: 'User' | 'Bot';
	msg: string;
	streaming?: boolean;
	metadata?: WsMetadata;
};

export type WsPayload = {
	uuid?: string;
	message?: string;
	init?: boolean;
	use_evaluator?: boolean;
};

export type WsResponse = {
	on_chat_model_stream?: string;
	on_chat_model_end?: boolean;
	metadata?: WsMetadata;
};
