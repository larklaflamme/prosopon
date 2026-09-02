//! Server configuration: `config.yaml` loader with defaults.
//!
//! Every field is optional in the YAML. Missing keys fall back to the
//! `Default` impl for the containing struct, so a bare `{}` config is valid
//! and yields the M0 defaults (Kokoro `af_heart`, Ollama `qwen2.5:3b`,
//! WebRTC port 29434).

use serde::Deserialize;
use std::path::Path;

/// Top-level server configuration.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub tts: TtsConfig,
    pub cognition: CognitionConfig,
    pub webrtc: WebrtcConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            tts: TtsConfig::default(),
            cognition: CognitionConfig::default(),
            webrtc: WebrtcConfig::default(),
        }
    }
}

/// Kokoro TTS settings.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default)]
pub struct TtsConfig {
    pub base_url: String,
    pub model: String,
    pub voice: String,
}

impl Default for TtsConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:21802".into(),
            model: "kokoro".into(),
            voice: "af_heart".into(),
        }
    }
}

/// Ollama cognition settings.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default)]
pub struct CognitionConfig {
    pub base_url: String,
    pub model: String,
}

impl Default for CognitionConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:11434".into(),
            model: "qwen2.5:3b".into(),
        }
    }
}

/// WebRTC transport settings.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default)]
pub struct WebrtcConfig {
    pub listen_port: u16,
}

impl Default for WebrtcConfig {
    fn default() -> Self {
        Self { listen_port: 29434 }
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

impl Config {
    /// Load configuration from a YAML file on disk.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let raw = std::fs::read_to_string(path)?;
        Self::from_str(&raw)
    }

    /// Parse configuration from a YAML string.
    pub fn from_str(s: &str) -> Result<Self, ConfigError> {
        let cfg: Config = serde_yaml::from_str(s)?;
        Ok(cfg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_config_yields_all_defaults() {
        let cfg = Config::from_str("{}").expect("empty config should parse");
        assert_eq!(cfg, Config::default());
        assert_eq!(cfg.tts.voice, "af_heart");
        assert_eq!(cfg.cognition.model, "qwen2.5:3b");
        assert_eq!(cfg.webrtc.listen_port, 29434);
    }

    #[test]
    fn partial_override_applies_defaults_for_missing_keys() {
        let cfg = Config::from_str("tts:\n  voice: af_sky\n").expect("partial config should parse");
        // Overridden key.
        assert_eq!(cfg.tts.voice, "af_sky");
        // Missing keys fall back to defaults.
        assert_eq!(cfg.tts.base_url, "http://localhost:21802");
        assert_eq!(cfg.tts.model, "kokoro");
        assert_eq!(cfg.cognition.model, "qwen2.5:3b");
        assert_eq!(cfg.webrtc.listen_port, 29434);
    }

    #[test]
    fn missing_section_uses_section_default() {
        let cfg = Config::from_str("cognition:\n  model: qwen3:30b\n").expect("should parse");
        // tts and webrtc sections are entirely absent -> full defaults.
        assert_eq!(cfg.tts, TtsConfig::default());
        assert_eq!(cfg.webrtc, WebrtcConfig::default());
        // cognition model overridden.
        assert_eq!(cfg.cognition.model, "qwen3:30b");
        assert_eq!(cfg.cognition.base_url, "http://localhost:11434");
    }

    #[test]
    fn full_override_applies_every_value() {
        let yaml = r#"
tts:
  base_url: "http://example:9999"
  model: "tts-1"
  voice: "af_sky"
cognition:
  base_url: "http://example:8888"
  model: "qwen3:30b"
webrtc:
  listen_port: 40000
"#;
        let cfg = Config::from_str(yaml).expect("full config should parse");
        assert_eq!(cfg.tts.base_url, "http://example:9999");
        assert_eq!(cfg.tts.model, "tts-1");
        assert_eq!(cfg.tts.voice, "af_sky");
        assert_eq!(cfg.cognition.base_url, "http://example:8888");
        assert_eq!(cfg.cognition.model, "qwen3:30b");
        assert_eq!(cfg.webrtc.listen_port, 40000);
    }

    #[test]
    fn load_from_disk_matches_inline() {
        // The shipped config.yaml should parse to the same defaults as `{}`.
        let from_disk = Config::load(concat!(env!("CARGO_MANIFEST_DIR"), "/config.yaml"))
            .expect("shipped config.yaml should parse");
        assert_eq!(from_disk, Config::default());
    }
}
