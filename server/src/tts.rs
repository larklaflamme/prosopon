//! Kokoro TTS client (Slice 2).
//!
//! Under Option B (Lark's decision, 2026-09-02): the server fetches Ogg Opus
//! bytes from Kokoro and forwards them as-is over the WebRTC data channel.
//! No demuxing, no transcoding — the client plays the Ogg Opus natively.

use crate::config::TtsConfig;
use serde::Serialize;

/// The JSON body POSTed to Kokoro's `/v1/audio/speech`.
#[derive(Debug, Serialize)]
struct TtsRequest<'a> {
    model: &'a str,
    input: &'a str,
    voice: &'a str,
    response_format: &'a str,
    stream: bool,
}

/// HTTP client for the Kokoro TTS service.
pub struct TtsClient {
    client: reqwest::Client,
    base_url: String,
    model: String,
    voice: String,
}

/// Errors that can occur while synthesizing speech.
#[derive(Debug)]
pub enum TtsError {
    Http(reqwest::Error),
    EmptyAudio,
}

impl std::fmt::Display for TtsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TtsError::Http(e) => write!(f, "TTS request failed: {e}"),
            TtsError::EmptyAudio => write!(f, "TTS returned empty audio"),
        }
    }
}

impl std::error::Error for TtsError {}

impl From<reqwest::Error> for TtsError {
    fn from(e: reqwest::Error) -> Self {
        TtsError::Http(e)
    }
}

impl TtsClient {
    /// Build a client from TTS configuration.
    pub fn new(config: &TtsConfig) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: config.base_url.trim_end_matches('/').to_string(),
            model: config.model.clone(),
            voice: config.voice.clone(),
        }
    }

    /// Synthesize `text` into Ogg Opus audio bytes.
    ///
    /// Returns the full Ogg Opus stream (header + tags + audio pages) as a
    /// byte buffer. Under Option B the server forwards these bytes as-is.
    pub async fn synthesize(&self, text: &str) -> Result<Vec<u8>, TtsError> {
        let url = format!("{}/v1/audio/speech", self.base_url);
        let body = TtsRequest {
            model: &self.model,
            input: text,
            voice: &self.voice,
            response_format: "opus",
            stream: true,
        };

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await?
            .error_for_status()?;

        let bytes = resp.bytes().await?;
        if bytes.is_empty() {
            return Err(TtsError::EmptyAudio);
        }
        Ok(bytes.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_serializes_to_expected_shape() {
        let req = TtsRequest {
            model: "kokoro",
            input: "hello",
            voice: "af_heart",
            response_format: "opus",
            stream: true,
        };
        let json = serde_json::to_value(&req).expect("should serialize");
        assert_eq!(json["model"], "kokoro");
        assert_eq!(json["input"], "hello");
        assert_eq!(json["voice"], "af_heart");
        assert_eq!(json["response_format"], "opus");
        assert_eq!(json["stream"], true);
    }
}
