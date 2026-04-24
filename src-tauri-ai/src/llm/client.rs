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
#[serde(rename_all = "camelCase")]
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

    /// Ask the provider for the live model catalogue available to this key.
    ///
    /// Returns chat-completable models only. Embeddings, image, audio,
    /// moderation, and other non-chat models are filtered out.
    pub async fn fetch_models(&self) -> Result<Vec<ModelInfo>, LlmError> {
        match self.provider {
            Provider::OpenAI => self.fetch_models_openai().await,
            Provider::Anthropic => self.fetch_models_anthropic().await,
            Provider::Google => self.fetch_models_google().await,
        }
    }

    async fn fetch_models_openai(&self) -> Result<Vec<ModelInfo>, LlmError> {
        let resp = self
            .http
            .get("https://api.openai.com/v1/models")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            return Err(LlmError::Api {
                status: status.as_u16(),
                message: resp.text().await.unwrap_or_default(),
            });
        }
        let json: serde_json::Value = resp.json().await?;
        let data = json
            .get("data")
            .and_then(|d| d.as_array())
            .ok_or_else(|| LlmError::Parse("no 'data' array in OpenAI /models response".into()))?;
        let mut out: Vec<ModelInfo> = data
            .iter()
            .filter_map(|m| m.get("id").and_then(|i| i.as_str()).map(str::to_string))
            .filter(|id| is_openai_chat_model(id))
            .map(|id| ModelInfo {
                tier: classify_tier(&id),
                name: id.clone(),
                description: openai_description(&id),
                id,
                provider: Provider::OpenAI,
            })
            .collect();
        sort_for_display(&mut out);
        Ok(out)
    }

    async fn fetch_models_anthropic(&self) -> Result<Vec<ModelInfo>, LlmError> {
        let resp = self
            .http
            .get("https://api.anthropic.com/v1/models")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            return Err(LlmError::Api {
                status: status.as_u16(),
                message: resp.text().await.unwrap_or_default(),
            });
        }
        let json: serde_json::Value = resp.json().await?;
        let data = json
            .get("data")
            .and_then(|d| d.as_array())
            .ok_or_else(|| LlmError::Parse("no 'data' array in Anthropic /models response".into()))?;
        let mut out: Vec<ModelInfo> = data
            .iter()
            .filter_map(|m| {
                let id = m.get("id").and_then(|i| i.as_str())?.to_string();
                let display = m
                    .get("display_name")
                    .and_then(|d| d.as_str())
                    .map(str::to_string)
                    .unwrap_or_else(|| id.clone());
                Some(ModelInfo {
                    tier: classify_tier(&id),
                    name: display,
                    description: String::new(),
                    id,
                    provider: Provider::Anthropic,
                })
            })
            .collect();
        sort_for_display(&mut out);
        Ok(out)
    }

    async fn fetch_models_google(&self) -> Result<Vec<ModelInfo>, LlmError> {
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models?key={}",
            self.api_key
        );
        let resp = self.http.get(&url).send().await?;
        let status = resp.status();
        if !status.is_success() {
            return Err(LlmError::Api {
                status: status.as_u16(),
                message: resp.text().await.unwrap_or_default(),
            });
        }
        let json: serde_json::Value = resp.json().await?;
        let data = json
            .get("models")
            .and_then(|d| d.as_array())
            .ok_or_else(|| LlmError::Parse("no 'models' array in Gemini response".into()))?;
        let mut out: Vec<ModelInfo> = data
            .iter()
            .filter_map(|m| {
                let supports_generate = m
                    .get("supportedGenerationMethods")
                    .and_then(|s| s.as_array())
                    .map(|a| a.iter().any(|v| v.as_str() == Some("generateContent")))
                    .unwrap_or(false);
                if !supports_generate {
                    return None;
                }
                // Gemini returns "models/<id>"; strip the prefix for the
                // API call shape used elsewhere in this client.
                let raw = m.get("name").and_then(|n| n.as_str())?;
                let id = raw.strip_prefix("models/").unwrap_or(raw).to_string();
                let display = m
                    .get("displayName")
                    .and_then(|d| d.as_str())
                    .map(str::to_string)
                    .unwrap_or_else(|| id.clone());
                Some(ModelInfo {
                    tier: classify_tier(&id),
                    name: display,
                    description: m
                        .get("description")
                        .and_then(|d| d.as_str())
                        .map(str::to_string)
                        .unwrap_or_default(),
                    id,
                    provider: Provider::Google,
                })
            })
            .collect();
        sort_for_display(&mut out);
        Ok(out)
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
    //
    // Uses the v1/responses endpoint (not legacy v1/chat/completions) so that
    // both classic chat models (gpt-4.x, gpt-5) and the newer reasoning /
    // pro variants work without a separate code path. Responses returns a
    // convenience `output_text` field plus a structured `output` array — we
    // try `output_text` first and fall back to the array for forward-compat.

    async fn complete_openai(
        &self,
        system: &str,
        user_msg: &str,
        max_tokens: u32,
    ) -> Result<String, LlmError> {
        let body = serde_json::json!({
            "model": self.model,
            "instructions": system,
            "input": user_msg,
            "max_output_tokens": max_tokens,
        });
        self.openai_responses_call(body).await
    }

    async fn chat_openai(
        &self,
        system: &str,
        messages: &[ChatMessage],
        max_tokens: u32,
    ) -> Result<String, LlmError> {
        let input: Vec<serde_json::Value> = messages
            .iter()
            .map(|m| serde_json::json!({"role": m.role, "content": m.content}))
            .collect();
        let body = serde_json::json!({
            "model": self.model,
            "instructions": system,
            "input": input,
            "max_output_tokens": max_tokens,
        });
        self.openai_responses_call(body).await
    }

    async fn openai_responses_call(&self, body: serde_json::Value) -> Result<String, LlmError> {
        let resp = self
            .http
            .post("https://api.openai.com/v1/responses")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            return Err(LlmError::Api {
                status: status.as_u16(),
                message: resp.text().await.unwrap_or_default(),
            });
        }
        let json: serde_json::Value = resp.json().await?;
        extract_openai_responses_text(&json)
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

/// Extract assistant text from a v1/responses payload. Tries the convenience
/// `output_text` first, then walks the structured `output` array.
fn extract_openai_responses_text(json: &serde_json::Value) -> Result<String, LlmError> {
    if let Some(t) = json.get("output_text").and_then(|t| t.as_str()) {
        if !t.is_empty() {
            return Ok(t.to_string());
        }
    }
    let output = json
        .get("output")
        .and_then(|o| o.as_array())
        .ok_or_else(|| LlmError::Parse("no 'output' array in OpenAI Responses payload".into()))?;
    let mut buf = String::new();
    for item in output {
        if item.get("type").and_then(|t| t.as_str()) != Some("message") {
            continue;
        }
        if let Some(content) = item.get("content").and_then(|c| c.as_array()) {
            for part in content {
                let kind = part.get("type").and_then(|t| t.as_str()).unwrap_or("");
                if kind == "output_text" || kind == "text" {
                    if let Some(s) = part.get("text").and_then(|t| t.as_str()) {
                        buf.push_str(s);
                    }
                }
            }
        }
    }
    if buf.is_empty() {
        // Reasoning models consume tokens silently before producing text;
        // surface that case explicitly so the caller can bump max_tokens
        // instead of staring at an opaque "no text" error.
        let status = json
            .get("status")
            .and_then(|s| s.as_str())
            .unwrap_or("unknown");
        let usage = json.get("usage").map(|u| u.to_string()).unwrap_or_default();
        return Err(LlmError::Parse(format!(
            "OpenAI Responses returned no text (status={status}, usage={usage}). \
             For reasoning models, raise max_output_tokens."
        )));
    }
    Ok(buf)
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

/// True for OpenAI model ids that the chat-completions endpoint accepts.
/// Excludes embeddings, dall-e, whisper/tts, moderation, fine-tuned base
/// snapshots, and other non-chat heads.
fn is_openai_chat_model(id: &str) -> bool {
    let l = id.to_lowercase();
    // Hard excludes first.
    let excludes = [
        "embedding",
        "embed",
        "dall-e",
        "dalle",
        "whisper",
        "tts",
        "moderation",
        "babbage",
        "davinci-002",
        "image",
        "audio",
        "sora",
        "transcribe",
        "search",
        "computer-use",
        "realtime",
    ];
    if excludes.iter().any(|s| l.contains(s)) {
        return false;
    }
    // Allow gpt-*, o<digit>, chatgpt-*. Reject anything else.
    l.starts_with("gpt-")
        || l.starts_with("chatgpt-")
        || l.starts_with("o1")
        || l.starts_with("o3")
        || l.starts_with("o4")
        || l.starts_with("o5")
}

/// Best-effort tier classification from the model id.
fn classify_tier(id: &str) -> ModelTier {
    let l = id.to_lowercase();
    if ["mini", "nano", "haiku", "flash", "lite"]
        .iter()
        .any(|s| l.contains(s))
    {
        ModelTier::Fast
    } else if ["o1", "o3", "o4", "o5", "opus", "pro", "thinking", "reasoning"]
        .iter()
        .any(|s| l.contains(s))
    {
        ModelTier::Reasoning
    } else {
        ModelTier::Balanced
    }
}

fn openai_description(id: &str) -> String {
    let tier = classify_tier(id);
    match tier {
        ModelTier::Reasoning => "Reasoning-class OpenAI model".into(),
        ModelTier::Balanced => "OpenAI chat model".into(),
        ModelTier::Fast => "Fast / cost-optimised OpenAI model".into(),
    }
}

/// Sort by tier (Reasoning, Balanced, Fast), then name within each tier.
/// Keeps the strongest models near the top of the picker.
fn sort_for_display(models: &mut [ModelInfo]) {
    fn rank(t: ModelTier) -> u8 {
        match t {
            ModelTier::Reasoning => 0,
            ModelTier::Balanced => 1,
            ModelTier::Fast => 2,
        }
    }
    models.sort_by(|a, b| rank(a.tier).cmp(&rank(b.tier)).then(a.id.cmp(&b.id)));
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
