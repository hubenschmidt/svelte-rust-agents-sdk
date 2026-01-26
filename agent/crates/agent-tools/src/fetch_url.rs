use async_trait::async_trait;
use serde::Serialize;
use serde_json::json;

use crate::{Tool, ToolError};

/// Fetch URL tool - retrieves and extracts structured content from web pages
pub struct FetchUrlTool {
    client: reqwest::Client,
}

impl FetchUrlTool {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("Mozilla/5.0 (compatible; AgentBot/1.0)")
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
        }
    }
}

impl Default for FetchUrlTool {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Serialize)]
struct PageContent {
    url: String,
    title: Option<String>,
    description: Option<String>,
    content: String,
    content_type: String,
    truncated: bool,
}

/// Extract title from HTML using simple string matching
fn extract_title(html: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let start = lower.find("<title>")?;
    let end = lower.find("</title>")?;
    let title_start = start + 7; // len("<title>")
    if title_start < end {
        Some(html[title_start..end].trim().to_string())
    } else {
        None
    }
}

/// Extract meta description from HTML
fn extract_description(html: &str) -> Option<String> {
    let lower = html.to_lowercase();

    // Look for <meta name="description" content="...">
    let patterns = [
        r#"name="description""#,
        r#"name='description'"#,
        r#"property="og:description""#,
    ];

    for pattern in patterns {
        if let Some(pos) = lower.find(pattern) {
            // Find the content attribute nearby
            let search_region = &html[pos.saturating_sub(100)..std::cmp::min(pos + 500, html.len())];

            if let Some(content_start) = search_region.to_lowercase().find("content=") {
                let after_content = &search_region[content_start + 8..];
                let quote = after_content.chars().next()?;
                if quote == '"' || quote == '\'' {
                    let content_text = &after_content[1..];
                    if let Some(end) = content_text.find(quote) {
                        return Some(content_text[..end].to_string());
                    }
                }
            }
        }
    }
    None
}

#[async_trait]
impl Tool for FetchUrlTool {
    fn name(&self) -> &str {
        "fetch_url"
    }

    fn description(&self) -> &str {
        "Fetch and parse content from a URL. Returns structured data with title, description, and main text content."
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to fetch content from"
                },
                "max_length": {
                    "type": "integer",
                    "description": "Maximum characters for content (default: 8000)",
                    "default": 8000
                }
            },
            "required": ["url"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<String, ToolError> {
        let url = args
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArguments("Missing 'url' parameter".to_string()))?;

        let max_length = args
            .get("max_length")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(8000);

        let response = self.client.get(url).send().await?;

        if !response.status().is_success() {
            return Err(ToolError::ExecutionFailed(format!(
                "HTTP error: {}",
                response.status()
            )));
        }

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("text/plain")
            .to_string();

        let is_html = content_type.contains("text/html");
        let body = response.text().await?;

        // Extract metadata from HTML
        let (title, description) = if is_html {
            (extract_title(&body), extract_description(&body))
        } else {
            (None, None)
        };

        // Convert HTML to readable text
        let text = if is_html {
            html2text::from_read(body.as_bytes(), 80)
        } else {
            body
        };

        // Truncate if needed
        let (content, truncated) = if text.len() > max_length {
            (text[..max_length].to_string(), true)
        } else {
            (text, false)
        };

        let page = PageContent {
            url: url.to_string(),
            title,
            description,
            content,
            content_type,
            truncated,
        };

        serde_json::to_string_pretty(&page).map_err(|e| {
            ToolError::ExecutionFailed(format!("Failed to serialize response: {}", e))
        })
    }
}
