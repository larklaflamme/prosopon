//! End-to-end pipeline (Slice 4).
//!
//! Composes the two HTTP clients into a single voice-loop turn:
//!
//! ```text
//! text (conversation history) → cognition (Ollama) → reply text
//!                              → TTS (Kokoro)      → Ogg Opus bytes
//! ```
//!
//! The pipeline is **stateless** by design (Lark's decision, 2026-09-02):
//! the caller owns the conversation history and passes the full message list
//! on every turn. Conversation state is a later version — M0's goal is a fast
//! baseline, and a stateless pipeline is the simplest thing that delivers it.

use crate::cognition::{ChatMessage, CognitionClient, CognitionError};
use crate::config::Config;
use crate::tts::{TtsClient, TtsError};

/// The result of one pipeline turn: the assistant's reply text and its
/// synthesized Ogg Opus audio.
#[derive(Debug)]
pub struct PipelineOutput {
    /// The assistant's reply text (from cognition).
    pub reply: String,
    /// The full Ogg Opus stream (header + tags + audio pages) for `reply`.
    pub audio: Vec<u8>,
}

/// Errors that can occur while running a pipeline turn.
#[derive(Debug)]
pub enum PipelineError {
    Cognition(CognitionError),
    Tts(TtsError),
}

impl std::fmt::Display for PipelineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PipelineError::Cognition(e) => write!(f, "cognition stage failed: {e}"),
            PipelineError::Tts(e) => write!(f, "tts stage failed: {e}"),
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

/// The voice loop pipeline: cognition + TTS composed into one turn.
pub struct Pipeline {
    cognition: CognitionClient,
    tts: TtsClient,
}

impl Pipeline {
    /// Build a pipeline from the full server configuration.
    pub fn new(config: &Config) -> Self {
        Self {
            cognition: CognitionClient::new(&config.cognition),
            tts: TtsClient::new(&config.tts),
        }
    }

    /// Run one turn: reply to `messages`, then synthesize the reply to audio.
    ///
    /// Stateless — `messages` is the full conversation history, supplied by
    /// the caller. The pipeline stores nothing between turns.
    pub async fn run(&self, messages: &[ChatMessage]) -> Result<PipelineOutput, PipelineError> {
        let reply = self.cognition.chat(messages).await?;
        let audio = self.tts.synthesize(&reply).await?;
        Ok(PipelineOutput { reply, audio })
    }
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
}
