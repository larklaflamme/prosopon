//! Ollama cognition client (Slice 3).
//!
//! Sends the conversation history to Ollama's `/api/chat` and returns the
//! assistant's reply text. The model is a config parameter — M0 ships the
//! fast-but-dumb `qwen2.5:3b`, and the smart tier (`qwen3:30b`) is a one-line
//! `config.yaml` swap. Both are non-thinking: the client always sends
//! `think: false` at the TOP LEVEL of the request body (not inside `options`),
//! which is the only placement Ollama honours.
//!
//! ## Tool calling (2026-09-14)
//!
//! `chat_with_tools` offers the model a set of tools (e.g. `web_search`) via
//! Ollama's native function-calling support. When the model decides to call a
//! tool, the response carries `tool_calls` instead of a final answer; the
//! caller executes the tool and feeds the result back as a `tool`-role
//! message. See `pipeline.rs` for the orchestration loop.

use crate::config::CognitionConfig;
use serde::{Deserialize, Serialize};

/// A single chat message in the conversation history.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    /// Tool calls requested by the model (assistant messages only). Omitted
    /// from serialization when absent so plain history stays clean.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

impl ChatMessage {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: content.into(),
            tool_calls: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".into(),
            content: content.into(),
            tool_calls: None,
        }
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".into(),
            content: content.into(),
            tool_calls: None,
        }
    }

    /// A tool result message (role `tool`), carrying the tool's output.
    pub fn tool(content: impl Into<String>) -> Self {
        Self {
            role: "tool".into(),
            content: content.into(),
            tool_calls: None,
        }
    }

    /// An assistant message that requested tool calls (content may be empty).
    pub fn assistant_with_tool_calls(content: impl Into<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: "assistant".into(),
            content: content.into(),
            tool_calls: Some(tool_calls),
        }
    }
}

/// A tool call requested by the model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCall {
    pub function: ToolCallFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCallFunction {
    pub name: String,
    /// Arguments as a JSON object (or, from some models, a JSON string).
    pub arguments: serde_json::Value,
}

/// A tool definition offered to the model.
#[derive(Debug, Clone, Serialize)]
pub struct Tool {
    #[serde(rename = "type")]
    pub kind: String,
    pub function: ToolFunction,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolFunction {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// The reply from a chat call: text plus any tool calls the model requested.
#[derive(Debug)]
pub struct ChatReply {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<&'a [Tool]>,
}

/// The JSON body returned by Ollama's `/api/chat` (non-streaming).
#[derive(Debug, Deserialize)]
struct ChatResponse {
    message: ChatResponseMessage,
}

#[derive(Debug, Deserialize)]
struct ChatResponseMessage {
    #[serde(default)]
    content: String,
    #[serde(default)]
    tool_calls: Option<Vec<ToolCall>>,
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
        let reply = self.chat_with_tools(messages, &[]).await?;
        if reply.content.trim().is_empty() {
            return Err(CognitionError::EmptyReply);
        }
        Ok(reply.content)
    }

    /// Send the conversation history with a set of tools offered, and return
    /// the reply (text plus any tool calls the model requested).
    pub async fn chat_with_tools(
        &self,
        messages: &[ChatMessage],
        tools: &[Tool],
    ) -> Result<ChatReply, CognitionError> {
        let url = format!("{}/api/chat", self.base_url);
        let body = ChatRequest {
            model: &self.model,
            think: false,
            messages,
            stream: false,
            tools: if tools.is_empty() { None } else { Some(tools) },
        };

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await?
            .error_for_status()?;

        let parsed: ChatResponse = resp.json().await?;
        Ok(ChatReply {
            content: parsed.message.content,
            tool_calls: parsed.message.tool_calls.unwrap_or_default(),
        })
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
            tools: None,
        };
        let json = serde_json::to_value(&req).expect("should serialize");
        assert_eq!(json["model"], "qwen2.5:3b");
        // The critical contract: `think` is TOP-LEVEL, not nested in options.
        assert_eq!(json["think"], false);
        assert!(json.get("options").is_none(), "think must not be in options");
        assert_eq!(json["messages"][0]["role"], "user");
        assert_eq!(json["messages"][0]["content"], "hello");
        // No tools -> no `tools` key at all.
        assert!(json.get("tools").is_none());
    }

    #[test]
    fn request_serializes_tools_when_present() {
        let messages = vec![ChatMessage::user("what's the weather")];
        let tools = vec![Tool {
            kind: "function".into(),
            function: ToolFunction {
                name: "web_search".into(),
                description: "search the web".into(),
                parameters: serde_json::json!({"type": "object", "properties": {}}),
            },
        }];
        let req = ChatRequest {
            model: "qwen2.5:3b",
            think: false,
            messages: &messages,
            stream: false,
            tools: Some(&tools),
        };
        let json = serde_json::to_value(&req).expect("should serialize");
        assert_eq!(json["tools"][0]["type"], "function");
        assert_eq!(json["tools"][0]["function"]["name"], "web_search");
    }

    #[test]
    fn assistant_tool_call_message_serializes_tool_calls() {
        let call = ToolCall {
            function: ToolCallFunction {
                name: "web_search".into(),
                arguments: serde_json::json!({"query": "rust"}),
            },
        };
        let msg = ChatMessage::assistant_with_tool_calls("", vec![call]);
        let json = serde_json::to_value(&msg).expect("serialize");
        assert_eq!(json["role"], "assistant");
        assert_eq!(json["tool_calls"][0]["function"]["name"], "web_search");
    }

    #[test]
    fn plain_message_omits_tool_calls() {
        let msg = ChatMessage::user("hi");
        let json = serde_json::to_value(&msg).expect("serialize");
        assert!(json.get("tool_calls").is_none());
    }
}
