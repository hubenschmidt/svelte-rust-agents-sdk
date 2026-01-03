use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct WsPayload {
    pub uuid: Option<String>,
    pub message: Option<String>,
    #[serde(default)]
    pub init: bool,
    #[serde(default = "default_use_evaluator")]
    pub use_evaluator: bool,
}

fn default_use_evaluator() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct WsMetadata {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub elapsed_ms: u64,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum WsResponse {
    Stream { on_chat_model_stream: String },
    End { on_chat_model_end: bool, metadata: Option<WsMetadata> },
}

impl WsResponse {
    pub fn stream(content: &str) -> Self {
        Self::Stream {
            on_chat_model_stream: content.to_string(),
        }
    }

    pub fn end() -> Self {
        Self::End {
            on_chat_model_end: true,
            metadata: None,
        }
    }

    pub fn end_with_metadata(metadata: WsMetadata) -> Self {
        Self::End {
            on_chat_model_end: true,
            metadata: Some(metadata),
        }
    }
}
