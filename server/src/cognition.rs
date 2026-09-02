//! Ollama cognition client (Slice 3).
//!
//! Sends the conversation history to Ollama's `/api/chat` and returns the
//! assistant's reply text. The model is a config parameter — M0 ships the
//! fast-but-dumb `qwen2.5:3b`, and the smart tier (`qwen3:30b`) is a one-line
//! `config.yaml` swap. Both are non-thinking: the client always sends
//! `think: false` at the TOP LEVEL of the request body (not inside `options`),
//! which is the only placement Ollama honours.

use crate::config::CognitionConfig;
use serde::{Deserialize, Serialize};

/// A single chat message in the conversation history.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: content.into(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".into(),
            content: content.into(),
        }
    }
}

/// The JSON body POSTed to Ollama's `/api/chat`.
#[derive(Debug, Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    /// Top-level `think: false` — disables thinking tokens. This is the only
    /// placement Ollama honours (verified live: `options.think` is ignored).
    think: bool,
    messages: &'a [ChatMessage],
    stream: bool,
}

/// The JSON body returned by Ollama's `/api/chat` (non-streaming).
#[derive(Debug, Deserialize)]
struct ChatResponse {
    message: ChatResponseMessage,
}

#[derive(Debug, Deserialize)]
struct ChatResponseMessage {
    content: String,
}

/// HTTP client for the Ollama cognition service.
pub struct CognitionClient {
    client: reqwest::Client,
    base_url: String,
    model: String,
}

/// Errors that can occur while querying cognition.
#[derive(Debug)]
pub enum CognitionError {
    Http(reqwest::Error),
    EmptyReply,
}

impl std::fmt::Display for CognitionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CognitionError::Http(e) => write!(f, "cognition request failed: {e}"),
            CognitionError::EmptyReply => write!(f, "cognition returned an empty reply"),
        }
    }
}

impl std::error::Error for CognitionError {}

impl From<reqwest::Error> for CognitionError {
    fn from(e: reqwest::Error) -> Self {
        CognitionError::Http(e)
    }
}

impl CognitionClient {
    /// Build a client from cognition configuration.
    pub fn new(config: &CognitionConfig) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: config.base_url.trim_end_matches('/').to_string(),
            model: config.model.clone(),
        }
    }

    /// Send the conversation history and return the assistant's reply text.
    ///
    /// `messages` is the full history (system/user/assistant turns). The
    /// client appends nothing — the caller owns the conversation state.
    pub async fn chat(&self, messages: &[ChatMessage]) -> Result<String, CognitionError> {
        let url = format!("{}/api/chat", self.base_url);
        let body = ChatRequest {
            model: &self.model,
            think: false,
            messages,
            stream: false,
        };

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await?
            .error_for_status()?;

        let parsed: ChatResponse = resp.json().await?;
        if parsed.message.content.trim().is_empty() {
            return Err(CognitionError::EmptyReply);
        }
        Ok(parsed.message.content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_serializes_to_expected_shape() {
        let messages = vec![ChatMessage::user("hello")];
        let req = ChatRequest {
            model: "qwen2.5:3b",
            think: false,
            messages: &messages,
            stream: false,
        };
        let json = serde_json::to_value(&req).expect("should serialize");
        assert_eq!(json["model"], "qwen2.5:3b");
        // The critical contract: `think` is TOP-LEVEL, not nested in options.
        assert_eq!(json["think"], false);
        assert!(json.get("options").is_none(), "think must not be in options");
        assert_eq!(json["messages"][0]["role"], "user");
        assert_eq!(json["messages"][0]["content"], "hello");
        assert_eq!(json["stream"], false);
    }

    #[test]
    fn response_deserializes_content() {
        let raw = r#"{"model":"qwen2.5:3b","message":{"role":"assistant","content":"Hello!"},"done":true}"#;
        let parsed: ChatResponse = serde_json::from_str(raw).expect("should deserialize");
        assert_eq!(parsed.message.content, "Hello!");
    }
}
