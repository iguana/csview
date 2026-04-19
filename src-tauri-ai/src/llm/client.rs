//! HTTP client for the Anthropic Claude API.
//!
//! Provides blocking-free async methods for text and structured (JSON) completion.

use serde::de::DeserializeOwned;

use super::types::LlmError;

const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// A lightweight async wrapper around the Anthropic Messages API.
#[derive(Clone)]
pub struct LlmClient {
    api_key: String,
    http: reqwest::Client,
    model: String,
}

impl LlmClient {
    /// Create a new client with the given API key.
    ///
    /// Uses `claude-sonnet-4-20250514` by default.
    #[must_use]
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            http: reqwest::Client::new(),
            model: "claude-sonnet-4-20250514".to_string(),
        }
    }

    /// Override the model identifier.
    #[must_use]
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// Send a single-turn request and return the assistant's text response.
    ///
    /// - `system` — the system prompt (sets context / format instructions).
    /// - `user_message` — the user turn content.
    /// - `max_tokens` — upper token budget for the response.
    pub async fn complete(
        &self,
        system: &str,
        user_message: &str,
        max_tokens: u32,
    ) -> Result<String, LlmError> {
        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": max_tokens,
            "system": system,
            "messages": [
                { "role": "user", "content": user_message }
            ]
        });

        let resp = self
            .http
            .post(ANTHROPIC_API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let message = resp.text().await.unwrap_or_default();
            return Err(LlmError::Api {
                status: status.as_u16(),
                message,
            });
        }

        let json: serde_json::Value = resp.json().await?;
        extract_text_content(&json)
    }

    /// Send a request and deserialise the response JSON into `T`.
    ///
    /// Strips leading/trailing Markdown code fences (` ```json … ``` `) before
    /// parsing so prompts can ask the model to format output as a fenced block.
    pub async fn complete_json<T: DeserializeOwned>(
        &self,
        system: &str,
        user_message: &str,
        max_tokens: u32,
    ) -> Result<T, LlmError> {
        let raw = self.complete(system, user_message, max_tokens).await?;
        let json_str = strip_code_fences(&raw);
        serde_json::from_str(json_str)
            .map_err(|e| LlmError::Parse(format!("json deserialise failed: {e}\nraw: {json_str}")))
    }

    /// Send a conversation (multi-turn) and return the assistant text.
    pub async fn chat(
        &self,
        system: &str,
        messages: &[ChatMessage],
        max_tokens: u32,
    ) -> Result<String, LlmError> {
        let msg_json: Vec<serde_json::Value> = messages
            .iter()
            .map(|m| {
                serde_json::json!({
                    "role": m.role,
                    "content": m.content
                })
            })
            .collect();

        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": max_tokens,
            "system": system,
            "messages": msg_json
        });

        let resp = self
            .http
            .post(ANTHROPIC_API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let message = resp.text().await.unwrap_or_default();
            return Err(LlmError::Api {
                status: status.as_u16(),
                message,
            });
        }

        let json: serde_json::Value = resp.json().await?;
        extract_text_content(&json)
    }
}

/// A single turn in a conversation.
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    pub fn user(content: impl Into<String>) -> Self {
        Self { role: "user".into(), content: content.into() }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: "assistant".into(), content: content.into() }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn extract_text_content(json: &serde_json::Value) -> Result<String, LlmError> {
    let content = json
        .get("content")
        .and_then(|c| c.as_array())
        .ok_or_else(|| LlmError::Parse("missing 'content' array in response".into()))?;

    for block in content {
        if block.get("type").and_then(|t| t.as_str()) == Some("text") {
            if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                return Ok(text.to_string());
            }
        }
    }

    Err(LlmError::Parse("no text block found in response content".into()))
}

/// Strip optional Markdown code fences from LLM output.
///
/// Handles both ` ```json\n…\n``` ` and ` ```\n…\n``` `.
fn strip_code_fences(s: &str) -> &str {
    let trimmed = s.trim();
    if let Some(inner) = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
    {
        if let Some(stripped) = inner.trim_start_matches('\n').strip_suffix("```") {
            return stripped.trim_end();
        }
    }
    trimmed
}
