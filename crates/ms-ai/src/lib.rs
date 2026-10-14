//! Bring-your-own-key AI for MediaSmith (see docs/ai.md).
//!
//! - [`config`]: provider settings saved as JSON, never containing secrets.
//! - [`keys`]: API keys in the OS keychain (Windows Credential Manager,
//!   macOS Keychain, Linux kernel keyring) behind a [`keys::KeyStore`] trait.
//! - [`client`]: one OpenAI-compatible chat client. It covers OpenAI, Ollama
//!   and any compatible server, plus Gemini and Anthropic through their
//!   OpenAI-compatible endpoints.
//! - [`rewrite`]: caption rewrite that keeps every timestamp.

pub mod client;
pub mod config;
pub mod keys;
pub mod rewrite;

#[derive(Debug, thiserror::Error)]
pub enum AiError {
    #[error("network error: {0}")]
    Http(String),
    #[error("{provider} returned HTTP {status}: {body}")]
    Status {
        provider: String,
        status: u16,
        body: String,
    },
    #[error("unexpected response: {0}")]
    BadResponse(String),
    #[error("keychain error: {0}")]
    Keychain(String),
    #[error("no API key saved for {0}")]
    NoKey(String),
    #[error("unknown provider {0}")]
    UnknownProvider(String),
    #[error("invalid settings: {0}")]
    Config(String),
}

/// Remove anything key-shaped from a message before it reaches the UI/logs.
pub fn redact(msg: &str, secret: Option<&str>) -> String {
    match secret {
        Some(s) if s.len() >= 4 => msg.replace(s, "[redacted]"),
        _ => msg.to_string(),
    }
}
