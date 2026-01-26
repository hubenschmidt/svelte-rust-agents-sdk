use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{Tool, ToolError};

/// Web search tool using Tavily API
pub struct WebSearchTool {
    api_key: String,
    client: reqwest::Client,
}

impl WebSearchTool {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: reqwest::Client::new(),
        }
    }
}

#[derive(Debug, Serialize)]
struct TavilyRequest {
    api_key: String,
    query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_results: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    search_depth: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TavilyResponse {
    results: Vec<TavilyResult>,
    #[serde(default)]
    answer: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TavilyResult {
    title: String,
    url: String,
    content: String,
    #[serde(default)]
    score: f64,
}

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "Search the web for information. Returns relevant results with titles, URLs, and content snippets."
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of results to return (default: 5)",
                    "default": 5
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<String, ToolError> {
        let query = args
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArguments("Missing 'query' parameter".to_string()))?;

        let max_results = args
            .get("max_results")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32)
            .unwrap_or(5);

        let request = TavilyRequest {
            api_key: self.api_key.clone(),
            query: query.to_string(),
            max_results: Some(max_results),
            search_depth: Some("basic".to_string()),
        };

        let response = self
            .client
            .post("https://api.tavily.com/search")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ToolError::ExecutionFailed(format!(
                "Tavily API error: {} - {}",
                status, body
            )));
        }

        let tavily_response: TavilyResponse = response.json().await.map_err(|e| {
            ToolError::ExecutionFailed(format!("Failed to parse Tavily response: {}", e))
        })?;

        // Format results as readable text
        let mut output = String::new();

        if let Some(answer) = &tavily_response.answer {
            output.push_str(&format!("**Summary:** {}\n\n", answer));
        }

        output.push_str("**Search Results:**\n\n");

        for (i, result) in tavily_response.results.iter().enumerate() {
            output.push_str(&format!(
                "{}. **{}**\n   URL: {}\n   {}\n\n",
                i + 1,
                result.title,
                result.url,
                result.content
            ));
        }

        Ok(output)
    }
}
