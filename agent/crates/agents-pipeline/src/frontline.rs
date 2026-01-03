use agents_core::{AgentError, FrontlineDecision, Message};
use agents_llm::{LlmClient, LlmStream};
use serde::Deserialize;
use tracing::info;

use crate::prompts::{FRONTLINE_DECISION_PROMPT, FRONTLINE_PROMPT, FRONTLINE_RESPONSE_PROMPT};

#[derive(Deserialize)]
struct QuickDecision {
    should_route: bool,
}

pub struct Frontline {
    client: LlmClient,
}

impl Frontline {
    pub fn new(model: &str) -> Self {
        Self {
            client: LlmClient::new(model),
        }
    }

    pub async fn process(
        &self,
        user_input: &str,
        history: &[Message],
    ) -> Result<(bool, String), AgentError> {
        info!("FRONTLINE: Processing request");

        let history_context = if history.is_empty() {
            String::new()
        } else {
            let recent: Vec<_> = history.iter().rev().take(4).rev().collect();
            recent
                .iter()
                .map(|m| format!("{:?}: {}", m.role, m.content))
                .collect::<Vec<_>>()
                .join("\n")
        };

        let context = format!(
            "Recent conversation:\n{history_context}\n\nCurrent user message: {user_input}\n\nDecide whether to handle this directly or route to the orchestrator."
        );

        let (response, _metrics) = self.client.structured::<FrontlineDecision>(FRONTLINE_PROMPT, &context).await?;

        if response.should_route {
            info!("FRONTLINE: Routing to orchestrator ({})", response.response);
            return Ok((true, response.response));
        }

        info!("FRONTLINE: Handled directly");
        Ok((false, response.response))
    }

    /// Returns Some(stream) if frontline handles directly, None if should route to orchestrator
    pub async fn process_stream(
        &self,
        user_input: &str,
        history: &[Message],
    ) -> Result<Option<LlmStream>, AgentError> {
        info!("FRONTLINE: Processing request (streaming)");

        let history_context = if history.is_empty() {
            String::new()
        } else {
            let recent: Vec<_> = history.iter().rev().take(4).rev().collect();
            recent
                .iter()
                .map(|m| format!("{:?}: {}", m.role, m.content))
                .collect::<Vec<_>>()
                .join("\n")
        };

        let context = format!("Recent conversation:\n{history_context}\n\nUser: {user_input}");

        let (decision, _metrics) = self
            .client
            .structured::<QuickDecision>(FRONTLINE_DECISION_PROMPT, &context)
            .await?;

        if decision.should_route {
            info!("FRONTLINE: Routing to orchestrator");
            return Ok(None);
        }

        info!("FRONTLINE: Streaming direct response");
        let stream = self.client.chat_stream(FRONTLINE_RESPONSE_PROMPT, &context).await?;
        Ok(Some(stream))
    }
}
