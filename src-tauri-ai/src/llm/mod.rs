//! LLM subsystem — Claude API client, prompt templates, and shared types.

pub mod client;
pub mod prompts;
pub mod types;

pub use client::{ChatMessage, LlmClient};
pub use types::LlmError;
