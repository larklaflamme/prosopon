//! Client configuration: `config.yaml` loader with defaults.
//!
//! The client is the *initiating* peer, so it needs no fixed WebRTC port —
//! it binds an ephemeral UDP socket. The only thing it must know is where to
//! POST its SDP offer (the server's HTTP signaling endpoint).

use serde::Deserialize;
use std::path::Path;

/// Top-level client configuration.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default)]
pub struct ClientConfig {
    pub signaling: SignalingConfig,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            signaling: SignalingConfig::default(),
        }
    }
}

/// HTTP signaling settings (Option B — the SDP offer/answer exchange).
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default)]
pub struct SignalingConfig {
    /// The HTTP endpoint the client POSTs its SDP offer to.
    pub url: String,
}

impl Default for SignalingConfig {
    fn default() -> Self {
        Self {
            url: "http://localhost:29435/offer".into(),
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
    }

    #[test]
    fn partial_override_applies_defaults_for_missing_keys() {
        let cfg = ClientConfig::from_str("signaling:\n  url: \"http://example:9999/offer\"\n")
            .expect("partial config should parse");
        assert_eq!(cfg.signaling.url, "http://example:9999/offer");
    }
}
