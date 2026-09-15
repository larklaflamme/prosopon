//! Client configuration: `config.yaml` loader with defaults.
//!
//! The client is the *initiating* peer, so it needs no fixed WebRTC port —
//! it binds an ephemeral UDP socket. It must know where to POST its SDP offer
//! (the server's HTTP signaling endpoint) and which STUN servers to use for
//! ICE candidate gathering.

use serde::Deserialize;
use std::path::Path;

/// Top-level client configuration.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default)]
pub struct ClientConfig {
    pub signaling: SignalingConfig,
    pub webrtc: WebrtcConfig,
    pub wake_word: WakeWordConfig,
    pub stt: SttConfig,
    pub conversation: ConversationConfig,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            signaling: SignalingConfig::default(),
            webrtc: WebrtcConfig::default(),
            wake_word: WakeWordConfig::default(),
            stt: SttConfig::default(),
            conversation: ConversationConfig::default(),
        }
    }
}

/// HTTP signaling settings (Option B — the SDP offer/answer exchange).
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default)]
pub struct SignalingConfig {
    /// The HTTP endpoint the client POSTs its SDP offer to.
    pub url: String,
    /// Shared secret presented to the server as `Authorization: Bearer
    /// <token>`. Empty = no auth (localhost dev).
    pub auth_token: String,
}

impl Default for SignalingConfig {
    fn default() -> Self {
        Self {
            url: "http://localhost:29435/offer".into(),
            auth_token: String::new(),
        }
    }
}

/// WebRTC transport settings.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default)]
pub struct WebrtcConfig {
    /// STUN servers used for ICE candidate gathering. Host-only candidates
    /// are not reachable across NAT, so a public STUN server is required for
    /// the client to reach the server over the internet (server-reflexive
    /// candidates). Defaults to Google's public STUN.
    pub stun_servers: Vec<String>,
}

impl Default for WebrtcConfig {
    fn default() -> Self {
        Self {
            stun_servers: vec!["stun:stun.l.google.com:19302".into()],
        }
    }
}

/// Wake-word detection settings (openWakeWord sidecar).
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default)]
pub struct WakeWordConfig {
    /// The Python interpreter that runs the sidecar. Must be the venv's
    /// Python (which has `openwakeword` installed), e.g.
    /// `client/.venv/bin/python` — not the system `python3`.
    pub python: String,
    /// The openWakeWord model: a bundled name (e.g. `hey_jarvis`) or a path
    /// to a custom `.tflite`. "Hey Skye" requires a custom-trained model;
    /// M0 uses a placeholder until that model exists.
    pub model: String,
    /// Detection threshold (0.0–1.0). Higher = fewer false positives.
    pub threshold: f32,
    /// Path to the Python sidecar script, relative to the working directory.
    pub sidecar_path: String,
    /// Whether to start the wake-word listener automatically at app launch.
    /// When false, it must be started manually via the `start_wake_word`
    /// command.
    pub auto_start: bool,
}

impl Default for WakeWordConfig {
    fn default() -> Self {
        Self {
            python: "python3".into(),
            model: "hey_jarvis".into(),
            threshold: 0.5,
            sidecar_path: "sidecar/wake_word.py".into(),
            auto_start: true,
        }
    }
}

/// Speech-to-text settings (Moonshine Voice sidecar).
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default)]
pub struct SttConfig {
    /// The Python interpreter that runs the sidecar. Must be the venv's
    /// Python (which has `moonshine-voice` installed).
    pub python: String,
    /// Path to the Python sidecar script, relative to the working directory.
    pub sidecar_path: String,
    /// Language code passed to the sidecar (`--language`).
    pub language: String,
    /// Moonshine streaming model arch (`--model-arch`): one of
    /// `tiny-streaming`, `small-streaming`, `medium-streaming`.
    pub model_arch: String,
}

impl Default for SttConfig {
    fn default() -> Self {
        Self {
            python: "python3".into(),
            sidecar_path: "sidecar/stt.py".into(),
            language: "en".into(),
            model_arch: "small-streaming".into(),
        }
    }
}

/// Conversation-loop settings (the wake → STT → respond orchestration).
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default)]
pub struct ConversationConfig {
    /// Whether to start the full conversation loop automatically at launch.
    /// When true, the loop owns the wake-word + STT lifecycles (the mic is
    /// handed off between them). When false, the wake word runs standalone.
    pub auto_start: bool,
    /// Silence timeout (seconds): if no utterance completes within this
    /// window after the wake word, return to idle.
    pub silence_timeout_secs: u64,
    /// Inactivity timeout (seconds): how long of silence after the agent
    /// finishes speaking before the warm multi-turn session ends and the
    /// client returns to cold (wake word required again). Distinct from
    /// `silence_timeout_secs`, which bounds a single in-flight utterance.
    pub inactivity_timeout_secs: u64,
}

impl Default for ConversationConfig {
    fn default() -> Self {
        Self {
            auto_start: false,
            silence_timeout_secs: 15,
            inactivity_timeout_secs: 5,
        }
    }
}

/// Errors that can occur while loading configuration.
#[derive(Debug)]
pub enum ConfigError {
    Io(std::io::Error),
    Parse(serde_yaml::Error),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Io(e) => write!(f, "failed to read config file: {e}"),
            ConfigError::Parse(e) => write!(f, "failed to parse config: {e}"),
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<std::io::Error> for ConfigError {
    fn from(e: std::io::Error) -> Self {
        ConfigError::Io(e)
    }
}

impl From<serde_yaml::Error> for ConfigError {
    fn from(e: serde_yaml::Error) -> Self {
        ConfigError::Parse(e)
    }
}

impl ClientConfig {
    /// Load configuration from a YAML file on disk.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let raw = std::fs::read_to_string(path)?;
        Self::from_str(&raw)
    }

    /// Parse configuration from a YAML string.
    pub fn from_str(s: &str) -> Result<Self, ConfigError> {
        let cfg: ClientConfig = serde_yaml::from_str(s)?;
        Ok(cfg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_config_yields_all_defaults() {
        let cfg = ClientConfig::from_str("{}").expect("empty config should parse");
        assert_eq!(cfg.signaling.url, "http://localhost:29435/offer");
        assert_eq!(cfg.wake_word.model, "hey_jarvis");
        assert_eq!(cfg.stt.model_arch, "small-streaming");
        assert!(!cfg.conversation.auto_start);
        assert_eq!(cfg.conversation.silence_timeout_secs, 15);
        assert_eq!(cfg.conversation.inactivity_timeout_secs, 5);
    }
}
