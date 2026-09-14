//! Kokoro TTS client (Slice 2).
//!
//! Under Option B (Lark's decision, 2026-09-02): the server fetches WAV
//! bytes from Kokoro and forwards them as-is over the WebRTC data channel.
//! No demuxing, no transcoding — the client plays the WAV natively.

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


/// Kokoro (via ffmpeg/Lavf) writes a *streaming* WAV header with the RIFF
/// chunk size and the data-chunk size both set to 0xFFFFFFFF (unknown
/// length). Some WAV decoders (e.g. hound, used by rodio on the client)
/// reject this. This rewrites those two size fields to their correct values
/// so the WAV is a well-formed, seekable file.
fn fix_streaming_wav_header(bytes: &mut [u8]) {
    // Sanity: must be a RIFF/WAVE file.
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return;
    }
    let total = bytes.len() as u32;

    // RIFF chunk size = total file size - 8 (the "RIFF" + size fields).
    bytes[4..8].copy_from_slice(&(total - 8).to_le_bytes());

    // Walk chunks to find "data", then patch its size.
    let mut pos = 12usize;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32::from_le_bytes([
            bytes[pos + 4],
            bytes[pos + 5],
            bytes[pos + 6],
            bytes[pos + 7],
        ]) as usize;
        if id == b"data" {
            let data_size = total - (pos as u32 + 8);
            bytes[pos + 4..pos + 8].copy_from_slice(&data_size.to_le_bytes());
            return;
        }
        // Advance: 8-byte chunk header + size, padded to an even boundary.
        pos += 8 + size + (size & 1);
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

    /// Synthesize `text` into WAV audio bytes.
    ///
    /// Returns the full WAV stream as a
    /// byte buffer. The server forwards these bytes as-is.
    pub async fn synthesize(&self, text: &str) -> Result<Vec<u8>, TtsError> {
        let url = format!("{}/v1/audio/speech", self.base_url);
        let body = TtsRequest {
            model: &self.model,
            input: text,
            voice: &self.voice,
            response_format: "wav",
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
        let mut wav = bytes.to_vec();
        fix_streaming_wav_header(&mut wav);
        Ok(wav)
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
            response_format: "wav",
            stream: true,
        };
        let json = serde_json::to_value(&req).expect("should serialize");
        assert_eq!(json["model"], "kokoro");
        assert_eq!(json["input"], "hello");
        assert_eq!(json["voice"], "af_heart");
        assert_eq!(json["response_format"], "wav");
        assert_eq!(json["stream"], true);
    }

    #[test]
    fn fix_streaming_wav_header_patches_sizes() {
        // Build a minimal streaming WAV: RIFF(size=0xFFFFFFFF) WAVE
        // fmt(16, PCM) data(size=0xFFFFFFFF) + 4 sample bytes.
        let mut wav = vec![
            b'R', b'I', b'F', b'F', 0xFF, 0xFF, 0xFF, 0xFF, // RIFF + size
            b'W', b'A', b'V', b'E', // WAVE
            b'f', b'm', b't', b' ', 16, 0, 0, 0, // fmt + size
            1, 0, 1, 0, // PCM, 1 channel
            0x40, 0x1F, 0, 0, // 8000 Hz
            0x80, 0x3E, 0, 0, // 16000 byte rate
            2, 0, 16, 0, // block align 2, 16-bit
            b'd', b'a', b't', b'a', 0xFF, 0xFF, 0xFF, 0xFF, // data + size
            0, 0, 0, 0, // 4 sample bytes
        ];
        let total = wav.len() as u32;
        fix_streaming_wav_header(&mut wav);

        // RIFF size = total - 8.
        assert_eq!(&wav[4..8], &(total - 8).to_le_bytes());
        // data size = total - 44 (data chunk header at offset 36, data at 44).
        assert_eq!(&wav[40..44], &(total - 44).to_le_bytes());
    }
}
