//! Shared LLM error types.

/// Errors that can occur when interacting with the Claude API.
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("api error: status={status} message={message}")]
    Api { status: u16, message: String },

    #[error("parse error: {0}")]
    Parse(String),

    #[error("no api key set — call set_api_key first")]
    NoApiKey,
}
