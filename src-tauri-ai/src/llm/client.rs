//! Multi-provider LLM client: Anthropic Claude, OpenAI, and Google Gemini.
//!
//! The caller picks a provider + model. The client adapts the request format
//! and response parsing per provider.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use super::types::LlmError;

// ---------------------------------------------------------------------------
// Provider + model registry
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Anthropic,
    OpenAI,
    Google,
}

impl std::fmt::Display for Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Anthropic => write!(f, "Anthropic"),
            Self::OpenAI => write!(f, "OpenAI"),
            Self::Google => write!(f, "Google"),
        }
    }
}

/// A model available for selection in the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub provider: Provider,
    pub tier: ModelTier,
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelTier {
    Reasoning, // strongest reasoning (o3, opus, 2.5 pro thinking)
    Balanced,  // good all-round (gpt-4o, sonnet, 2.5 flash)
    Fast,      // speed optimised (gpt-4o-mini, haiku, 2.0 flash)
}

/// All models the app knows about, grouped by provider.
pub fn available_models() -> Vec<ModelInfo> {
    vec![
        // --- OpenAI ---
        ModelInfo {
            id: "o3".into(),
            name: "o3".into(),
            provider: Provider::OpenAI,
            tier: ModelTier::Reasoning,
            description: "Strongest OpenAI reasoning model".into(),
        },
        ModelInfo {
            id: "o4-mini".into(),
            name: "o4-mini".into(),
            provider: Provider::OpenAI,
            tier: ModelTier::Reasoning,
            description: "Fast reasoning model, cost-effective".into(),
        },
        ModelInfo {
            id: "gpt-4.1".into(),
            name: "GPT-4.1".into(),
            provider: Provider::OpenAI,
            tier: ModelTier::Balanced,
            description: "Excellent for coding & structured output".into(),
        },
        ModelInfo {
            id: "gpt-4.1-mini".into(),
            name: "GPT-4.1 Mini".into(),
            provider: Provider::OpenAI,
            tier: ModelTier::Fast,
            description: "Fast and affordable".into(),
        },
        ModelInfo {
            id: "gpt-4.1-nano".into(),
            name: "GPT-4.1 Nano".into(),
            provider: Provider::OpenAI,
            tier: ModelTier::Fast,
            description: "Fastest OpenAI model".into(),
        },
        // --- Google ---
        ModelInfo {
            id: "gemini-2.5-pro".into(),
            name: "Gemini 2.5 Pro".into(),
            provider: Provider::Google,
            tier: ModelTier::Reasoning,
            description: "Google's strongest model with thinking".into(),
        },
        ModelInfo {
            id: "gemini-2.5-flash".into(),
            name: "Gemini 2.5 Flash".into(),
            provider: Provider::Google,
            tier: ModelTier::Balanced,
            description: "Fast with optional thinking, great value".into(),
        },
        ModelInfo {
            id: "gemini-2.0-flash".into(),
            name: "Gemini 2.0 Flash".into(),
            provider: Provider::Google,
            tier: ModelTier::Fast,
            description: "Speed optimised, 1M context".into(),
        },
        // --- Anthropic ---
        ModelInfo {
            id: "claude-opus-4-20250514".into(),
            name: "Claude Opus 4".into(),
            provider: Provider::Anthropic,
            tier: ModelTier::Reasoning,
            description: "Strongest Anthropic model".into(),
        },
        ModelInfo {
            id: "claude-sonnet-4-20250514".into(),
            name: "Claude Sonnet 4".into(),
            provider: Provider::Anthropic,
            tier: ModelTier::Balanced,
            description: "Balanced speed and capability".into(),
        },
        ModelInfo {
            id: "claude-haiku-4-20250514".into(),
            name: "Claude Haiku 4".into(),
            provider: Provider::Anthropic,
            tier: ModelTier::Fast,
            description: "Fastest Anthropic model".into(),
        },
    ]
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct LlmClient {
    api_key: String,
    http: reqwest::Client,
    provider: Provider,
    model: String,
}

impl LlmClient {
    pub fn new(provider: Provider, api_key: String, model: String) -> Self {
        Self {
            api_key,
            http: reqwest::Client::new(),
            provider,
            model,
        }
    }

    pub fn provider(&self) -> Provider {
        self.provider
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// Single-turn completion. Returns the assistant's text.
    pub async fn complete(
        &self,
        system: &str,
        user_message: &str,
        max_tokens: u32,
    ) -> Result<String, LlmError> {
        match self.provider {
            Provider::Anthropic => self.complete_anthropic(system, user_message, max_tokens).await,
            Provider::OpenAI => self.complete_openai(system, user_message, max_tokens).await,
            Provider::Google => self.complete_google(system, user_message, max_tokens).await,
        }
    }

    /// Single-turn completion, parsed as JSON into `T`.
    pub async fn complete_json<T: DeserializeOwned>(
        &self,
        system: &str,
        user_message: &str,
        max_tokens: u32,
    ) -> Result<T, LlmError> {
        let raw = self.complete(system, user_message, max_tokens).await?;
        let json_str = strip_code_fences(&raw);
        serde_json::from_str(json_str)
            .map_err(|e| LlmError::Parse(format!("json parse failed: {e}\nraw: {json_str}")))
    }

    /// Multi-turn chat. Returns the assistant's text.
    pub async fn chat(
        &self,
        system: &str,
        messages: &[ChatMessage],
        max_tokens: u32,
    ) -> Result<String, LlmError> {
        match self.provider {
            Provider::Anthropic => self.chat_anthropic(system, messages, max_tokens).await,
            Provider::OpenAI => self.chat_openai(system, messages, max_tokens).await,
            Provider::Google => self.chat_google(system, messages, max_tokens).await,
        }
    }

    // --- Anthropic -----------------------------------------------------------

    async fn complete_anthropic(&self, system: &str, user_msg: &str, max_tokens: u32) -> Result<String, LlmError> {
        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": max_tokens,
            "system": system,
            "messages": [{"role": "user", "content": user_msg}]
        });
        let resp = self.http
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            return Err(LlmError::Api { status: status.as_u16(), message: resp.text().await.unwrap_or_default() });
        }
        let json: serde_json::Value = resp.json().await?;
        extract_anthropic_text(&json)
    }

    async fn chat_anthropic(&self, system: &str, messages: &[ChatMessage], max_tokens: u32) -> Result<String, LlmError> {
        let msgs: Vec<serde_json::Value> = messages.iter().map(|m| serde_json::json!({"role": m.role, "content": m.content})).collect();
        let body = serde_json::json!({"model": self.model, "max_tokens": max_tokens, "system": system, "messages": msgs});
        let resp = self.http.post("https://api.anthropic.com/v1/messages").header("x-api-key", &self.api_key).header("anthropic-version", "2023-06-01").json(&body).send().await?;
        let status = resp.status();
        if !status.is_success() { return Err(LlmError::Api { status: status.as_u16(), message: resp.text().await.unwrap_or_default() }); }
        let json: serde_json::Value = resp.json().await?;
        extract_anthropic_text(&json)
    }

    // --- OpenAI --------------------------------------------------------------

    async fn complete_openai(&self, system: &str, user_msg: &str, max_tokens: u32) -> Result<String, LlmError> {
        let body = serde_json::json!({
            "model": self.model,
            "max_completion_tokens": max_tokens,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user_msg}
            ]
        });
        let resp = self.http
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            return Err(LlmError::Api { status: status.as_u16(), message: resp.text().await.unwrap_or_default() });
        }
        let json: serde_json::Value = resp.json().await?;
        extract_openai_text(&json)
    }

    async fn chat_openai(&self, system: &str, messages: &[ChatMessage], max_tokens: u32) -> Result<String, LlmError> {
        let mut msgs = vec![serde_json::json!({"role": "system", "content": system})];
        for m in messages {
            msgs.push(serde_json::json!({"role": m.role, "content": m.content}));
        }
        let body = serde_json::json!({"model": self.model, "max_completion_tokens": max_tokens, "messages": msgs});
        let resp = self.http.post("https://api.openai.com/v1/chat/completions").header("Authorization", format!("Bearer {}", self.api_key)).json(&body).send().await?;
        let status = resp.status();
        if !status.is_success() { return Err(LlmError::Api { status: status.as_u16(), message: resp.text().await.unwrap_or_default() }); }
        let json: serde_json::Value = resp.json().await?;
        extract_openai_text(&json)
    }

    // --- Google Gemini -------------------------------------------------------

    async fn complete_google(&self, system: &str, user_msg: &str, max_tokens: u32) -> Result<String, LlmError> {
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            self.model, self.api_key
        );
        let body = serde_json::json!({
            "systemInstruction": {"parts": [{"text": system}]},
            "contents": [{"role": "user", "parts": [{"text": user_msg}]}],
            "generationConfig": {"maxOutputTokens": max_tokens}
        });
        let resp = self.http.post(&url).json(&body).send().await?;
        let status = resp.status();
        if !status.is_success() {
            return Err(LlmError::Api { status: status.as_u16(), message: resp.text().await.unwrap_or_default() });
        }
        let json: serde_json::Value = resp.json().await?;
        extract_gemini_text(&json)
    }

    async fn chat_google(&self, system: &str, messages: &[ChatMessage], max_tokens: u32) -> Result<String, LlmError> {
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            self.model, self.api_key
        );
        let contents: Vec<serde_json::Value> = messages.iter().map(|m| {
            let role = if m.role == "assistant" { "model" } else { "user" };
            serde_json::json!({"role": role, "parts": [{"text": m.content}]})
        }).collect();
        let body = serde_json::json!({
            "systemInstruction": {"parts": [{"text": system}]},
            "contents": contents,
            "generationConfig": {"maxOutputTokens": max_tokens}
        });
        let resp = self.http.post(&url).json(&body).send().await?;
        let status = resp.status();
        if !status.is_success() { return Err(LlmError::Api { status: status.as_u16(), message: resp.text().await.unwrap_or_default() }); }
        let json: serde_json::Value = resp.json().await?;
        extract_gemini_text(&json)
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
// Response extractors
// ---------------------------------------------------------------------------

fn extract_anthropic_text(json: &serde_json::Value) -> Result<String, LlmError> {
    let content = json.get("content").and_then(|c| c.as_array())
        .ok_or_else(|| LlmError::Parse("missing 'content' array".into()))?;
    for block in content {
        if block.get("type").and_then(|t| t.as_str()) == Some("text") {
            if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                return Ok(text.to_string());
            }
        }
    }
    Err(LlmError::Parse("no text block in Anthropic response".into()))
}

fn extract_openai_text(json: &serde_json::Value) -> Result<String, LlmError> {
    json.get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| LlmError::Parse("no content in OpenAI response".into()))
}

fn extract_gemini_text(json: &serde_json::Value) -> Result<String, LlmError> {
    json.get("candidates")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("content"))
        .and_then(|c| c.get("parts"))
        .and_then(|p| p.get(0))
        .and_then(|p| p.get("text"))
        .and_then(|t| t.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| LlmError::Parse("no text in Gemini response".into()))
}

fn strip_code_fences(s: &str) -> &str {
    let trimmed = s.trim();
    if let Some(inner) = trimmed.strip_prefix("```json").or_else(|| trimmed.strip_prefix("```")) {
        if let Some(stripped) = inner.trim_start_matches('\n').strip_suffix("```") {
            return stripped.trim_end();
        }
    }
    trimmed
}
