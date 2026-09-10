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
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            signaling: SignalingConfig::default(),
            webrtc: WebrtcConfig::default(),
            wake_word: WakeWordConfig::default(),
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
}

impl Default for WakeWordConfig {
    fn default() -> Self {
        Self {
            python: "python3".into(),
            model: "hey_jarvis".into(),
            threshold: 0.5,
            sidecar_path: "sidecar/wake_word.py".into(),
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
        assert_eq!(cfg, ClientConfig::default());
        assert_eq!(cfg.signaling.url, "http://localhost:29435/offer");
        assert_eq!(cfg.webrtc.stun_servers, vec!["stun:stun.l.google.com:19302"]);
        assert_eq!(cfg.wake_word.model, "hey_jarvis");
        assert_eq!(cfg.wake_word.threshold, 0.5);
        assert_eq!(cfg.wake_word.python, "python3");
    }

    #[test]
    fn partial_override_applies_defaults_for_missing_keys() {
        let cfg = ClientConfig::from_str("signaling:\n  url: http://example:29435/offer\n")
            .expect("partial config should parse");
        assert_eq!(cfg.signaling.url, "http://example:29435/offer");
        assert_eq!(cfg.webrtc.stun_servers, vec!["stun:stun.l.google.com:19302"]);
        assert_eq!(cfg.wake_word.model, "hey_jarvis");
    }

    #[test]
    fn full_override_applies_every_value() {
        let yaml = r#"
signaling:
  url: "http://example:29435/offer"
  auth_token: "hunter2"
webrtc:
  stun_servers:
    - "stun:stun.nvidia.com:3478"
wake_word:
  python: "client/.venv/bin/python"
  model: "hey_skyte"
  threshold: 0.7
  sidecar_path: "sidecar/wake_word.py"
"#;
        let cfg = ClientConfig::from_str(yaml).expect("full config should parse");
        assert_eq!(cfg.signaling.url, "http://example:29435/offer");
        assert_eq!(cfg.signaling.auth_token, "hunter2");
        assert_eq!(cfg.webrtc.stun_servers, vec!["stun:stun.nvidia.com:3478"]);
        assert_eq!(cfg.wake_word.python, "client/.venv/bin/python");
        assert_eq!(cfg.wake_word.model, "hey_skyte");
        assert_eq!(cfg.wake_word.threshold, 0.7);
    }
}
