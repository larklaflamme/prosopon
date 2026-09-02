//! Prosopon client core — the transport, audio, and speech subsystems shared
//! by the Tauri shell.
//!
//! This crate is deliberately **pure Rust with no Tauri dependency**, so it
//! can be compiled and tested independently of the GUI (and on a headless
//! Linux box, where the Tauri shell cannot build). The Tauri app
//! (`src-tauri/`) depends on this crate and adds the window, tray, and
//! state-machine wiring.
//!
//! The transport protocol (data channel) is:
//!
//! - **Client → Server:** a text message carrying the user's utterance.
//! - **Server → Client:** a text message `audio:<total_bytes>` followed by
//!   `ceil(total_bytes / 16 KiB)` binary messages carrying the Ogg Opus
//!   chunks. The client reassembles by concatenation (the channel is ordered
//!   and reliable).

pub mod config;
pub mod signaling;
pub mod webrtc_client;

use thiserror::Error;

/// Errors that can occur in the client core.
#[derive(Debug, Error)]
pub enum ClientError {
    #[error("webrtc error: {0}")]
    WebRtc(#[from] webrtc::error::Error),
    #[error("signaling error: {0}")]
    Signaling(#[from] reqwest::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("data channel closed before opening")]
    ChannelNotOpen,
    #[error("timed out waiting for the data channel to open")]
    ChannelOpenTimeout,
    #[error("data channel closed unexpectedly")]
    ChannelClosed,
    #[error("invalid audio header: {0}")]
    InvalidAudioHeader(String),
}
