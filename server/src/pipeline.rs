//! End-to-end pipeline (Slice 4).
//!
//! Composes the three clients into a single voice-loop turn:
//!
//! ```text
//! text (conversation history) → cognition (Ollama) → reply text
//!                              → web search (Tavily/Brave, on demand)
//!                              → TTS (Kokoro)      → WAV bytes
//! ```
//!
//! ## Agent loop (2026-09-14)
//!
//! The pipeline is still **stateless** — the caller owns the conversation
//! history and passes the full message list on every turn. What changed is
//! that a single turn is now an *agent loop*: the model is offered a
//! `web_search` tool, and if it requests a search, the pipeline executes the
//! search, feeds the results back, and re-queries the model for a final
//! answer. The loop is bounded (`MAX_TOOL_ROUNDS`) to prevent runaway
//! tool-calling.

use crate::a2f::A2fClient;
use crate::cognition::{ChatMessage, ChatReply, CognitionClient, CognitionError, Tool, ToolCall, ToolFunction};
use crate::config::Config;
use crate::tts::{TtsClient, TtsError};
use crate::web_search::{SearchResult, WebSearchClient, WebSearchError};

/// Maximum number of tool-calling rounds per turn before giving up.
const MAX_TOOL_ROUNDS: usize = 3;

/// The system prompt that frames Skye's persona and her tool use.
const SYSTEM_PROMPT: &str = "\
You are Skye, a warm, sharp voice assistant. You answer conversationally and \
concisely, in a way that sounds natural when spoken aloud. Your training data \
has a knowledge cutoff of mid-2024. You have a `web_search` tool that gives \
you live, up-to-date information from the web.\n\
\n\
Use `web_search` whenever the user asks for anything time-sensitive or after \
your cutoff — for example: \"What is the latest weather report for...\", \
\"What is the current price of...\", \"What are the latest news briefs about...\", \
or any question about recent events, current conditions, or facts you are not \
certain about. In these cases, go straight to `web_search` and answer from the \
results. Never say \"I do not have up-to-date information\" or \"I don't know\" \
when the answer is available via web search — search instead.\n\
\n\
For everything else, answer directly from what you know. Never mention the \
tool or the search itself — just answer the question.";

/// The result of one pipeline turn: the assistant's reply text and its
/// synthesized WAV audio.
#[derive(Debug)]
pub struct PipelineOutput {
    /// The assistant's reply text (from cognition).
    pub reply: String,
    /// The full WAV stream for `reply`.
    pub audio: Vec<u8>,
    /// The ARKit blendshape track (NDJSON) for `reply`, or `None` if A2F is
    /// disabled or failed. The avatar degrades to audio-only when `None`.
    pub blendshapes: Option<String>,
}

/// Errors that can occur while running a pipeline turn.
#[derive(Debug)]
pub enum PipelineError {
    Cognition(CognitionError),
    Tts(TtsError),
    WebSearch(WebSearchError),
    /// The agent loop exhausted its tool rounds without a final answer.
    NoReply,
}

impl std::fmt::Display for PipelineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PipelineError::Cognition(e) => write!(f, "cognition stage failed: {e}"),
            PipelineError::Tts(e) => write!(f, "tts stage failed: {e}"),
            PipelineError::WebSearch(e) => write!(f, "web search stage failed: {e}"),
            PipelineError::NoReply => write!(f, "agent loop produced no final reply"),
        }
    }
}

impl std::error::Error for PipelineError {}

impl From<CognitionError> for PipelineError {
    fn from(e: CognitionError) -> Self {
        PipelineError::Cognition(e)
    }
}

impl From<TtsError> for PipelineError {
    fn from(e: TtsError) -> Self {
        PipelineError::Tts(e)
    }
}

impl From<WebSearchError> for PipelineError {
    fn from(e: WebSearchError) -> Self {
        PipelineError::WebSearch(e)
    }
}

/// The voice loop pipeline: cognition + web search + TTS composed into one turn.
pub struct Pipeline {
    cognition: CognitionClient,
    tts: TtsClient,
    web_search: WebSearchClient,
    a2f: A2fClient,
}

impl Pipeline {
    /// Build a pipeline from the full server configuration.
    pub fn new(config: &Config) -> Self {
        Self {
            cognition: CognitionClient::new(&config.cognition),
            tts: TtsClient::new(&config.tts),
            web_search: WebSearchClient::new(&config.web_search),
            a2f: A2fClient::new(&config.a2f),
        }
    }

    /// Run one turn: reply to `history`, then synthesize the reply to audio.
    ///
    /// Stateless — `history` is the full conversation (including the just-added
    /// user message), supplied by the caller. The pipeline stores nothing
    /// between turns. Within a turn it runs the agent loop: offer the model
    /// the `web_search` tool, execute any requested search, and re-query until
    /// the model produces a final answer (or the round cap is hit).
    pub async fn run(&self, history: &[ChatMessage]) -> Result<PipelineOutput, PipelineError> {
        // Working copy: system prompt + full history. The caller's history is
        // untouched; tool-call scaffolding lives only in this local copy.
        let mut working = Vec::with_capacity(history.len() + 1);
        working.push(ChatMessage::system(SYSTEM_PROMPT));
        working.extend_from_slice(history);

        let tools = vec![self.web_search_tool()];
        let mut reply = String::new();

        for _ in 0..MAX_TOOL_ROUNDS {
            let ChatReply { content, tool_calls } =
                self.cognition.chat_with_tools(&working, &tools).await?;

            if tool_calls.is_empty() {
                reply = content;
                break;
            }

            // Record the assistant's tool-call request, then execute each call.
            working.push(ChatMessage::assistant_with_tool_calls(content, tool_calls.clone()));
            for call in &tool_calls {
                if call.function.name == "web_search" {
                    let query = extract_query(&call);
                    let results = self.web_search.search(&query).await?;
                    working.push(ChatMessage::tool(format_search_results(&results)));
                }
            }
        }

        if reply.trim().is_empty() {
            return Err(PipelineError::NoReply);
        }

        let audio = self.tts.synthesize(&reply).await?;
        let blendshapes = self.a2f.synthesize_blendshapes(&audio).await;
        Ok(PipelineOutput { reply, audio, blendshapes })
    }

    /// The `web_search` tool definition offered to the model.
    fn web_search_tool(&self) -> Tool {
        Tool {
            kind: "function".into(),
            function: ToolFunction {
                name: "web_search".into(),
                description: "Search the web for current or factual information.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "The search query."
                        }
                    },
                    "required": ["query"]
                }),
            },
        }
    }
}

/// Extract the search query from a tool call's arguments, tolerating both
/// object (`{"query": "..."}`) and string (`"..."`) argument encodings.
fn extract_query(call: &ToolCall) -> String {
    match &call.function.arguments {
        serde_json::Value::Object(map) => map
            .get("query")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        serde_json::Value::String(s) => s.clone(),
        _ => String::new(),
    }
}

/// Render search results as a compact text block for the model to read.
fn format_search_results(results: &[SearchResult]) -> String {
    if results.is_empty() {
        return "No results found.".to_string();
    }
    results
        .iter()
        .enumerate()
        .map(|(i, r)| format!("[{i}] {}\n{}\n{}", r.title, r.url, r.snippet))
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn pipeline_builds_from_default_config() {
        let config = Config::default();
        let _pipeline = Pipeline::new(&config);
    }

    #[test]
    fn extract_query_handles_object_and_string() {
        let obj = ToolCall {
            function: crate::cognition::ToolCallFunction {
                name: "web_search".into(),
                arguments: serde_json::json!({"query": "rust lang"}),
            },
        };
        assert_eq!(extract_query(&obj), "rust lang");

        let str_args = ToolCall {
            function: crate::cognition::ToolCallFunction {
                name: "web_search".into(),
                arguments: serde_json::json!("rust lang"),
            },
        };
        assert_eq!(extract_query(&str_args), "rust lang");
    }

    #[test]
    fn format_search_results_renders_entries() {
        let results = vec![SearchResult {
            title: "T".into(),
            url: "https://x".into(),
            snippet: "body".into(),
        }];
        let text = format_search_results(&results);
        assert!(text.contains("[0] T"));
        assert!(text.contains("https://x"));
        assert!(text.contains("body"));
    }

    #[test]
    fn format_search_results_empty() {
        assert_eq!(format_search_results(&[]), "No results found.");
    }
}
