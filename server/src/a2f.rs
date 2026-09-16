//! Audio2Face-3D bridge client (Phase 2).
//!
//! Writes the TTS WAV to a temp file, spawns the `a2f-bridge` binary, and
//! captures its NDJSON stdout (a header line naming the 52 ARKit blendshapes,
//! then one `{"t":..,"v":[..]}` line per animation frame). Returns `None` on
//! any failure so the avatar degrades to audio-only — the voice loop is never
//! broken by the avatar layer (non-negotiable principle from the design doc).

use crate::config::A2fConfig;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

/// Client for the `a2f-bridge` subprocess.
pub struct A2fClient {
    enabled: bool,
    bridge_path: String,
    endpoint: String,
    timeout: Duration,
}

impl A2fClient {
    /// Build a client from A2F configuration.
    pub fn new(config: &A2fConfig) -> Self {
        Self {
            enabled: config.enabled,
            bridge_path: config.bridge_path.clone(),
            endpoint: config.endpoint.clone(),
            timeout: Duration::from_secs(config.timeout_secs),
        }
    }

    /// Run the bridge on `wav` and return the raw NDJSON blendshape track, or
    /// `None` if A2F is disabled or anything fails.
    pub async fn synthesize_blendshapes(&self, wav: &[u8]) -> Option<String> {
        if !self.enabled {
            return None;
        }

        // Write the WAV to a temp file for the bridge to read.
        let tmp = std::env::temp_dir().join(format!("prosopon-a2f-{}.wav", std::process::id()));
        if let Err(e) = tokio::fs::write(&tmp, wav).await {
            eprintln!("a2f: failed to write temp wav: {e}");
            return None;
        }

        // Scale the bridge timeout to the audio length. The bridge streams
        // audio to the NIM and reads back animation frames at ~3.6x real-time,
        // so a fixed 3s timeout is too tight for longer TTS responses.
        let duration = wav_duration_secs(wav);
        let timeout = Duration::from_secs_f64(
            (duration / 2.0 + 3.0).max(self.timeout.as_secs_f64()).min(30.0),
        );
        let result = self.run_bridge(&tmp, timeout).await;

        let _ = tokio::fs::remove_file(&tmp).await;
        result
    }

    /// Spawn `a2f-bridge <wav> <endpoint>` and capture its stdout, bounded by
    /// `self.timeout`.
    async fn run_bridge(&self, wav_path: &Path, timeout: Duration) -> Option<String> {
        let mut child = Command::new(&self.bridge_path)
            .arg(wav_path)
            .arg(&self.endpoint)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| eprintln!("a2f: failed to spawn bridge: {e}"))
            .ok()?;

        let mut stdout = child.stdout.take()?;
        let mut buf = String::new();
        let read_fut = stdout.read_to_string(&mut buf);

        match tokio::time::timeout(timeout, read_fut).await {
            Ok(Ok(_)) => {
                // Reap the child (stdout EOF means it already exited).
                let _ = child.wait().await;
                if buf.trim().is_empty() {
                    None
                } else {
                    Some(buf)
                }
            }
            Ok(Err(e)) => {
                eprintln!("a2f: failed to read bridge stdout: {e}");
                let _ = child.kill().await;
                None
            }
            Err(_) => {
                eprintln!("a2f: bridge timed out after {:?}", self.timeout);
                let _ = child.kill().await;
                None
            }
        }
    }
}

/// Estimate the duration (seconds) of a PCM WAV from its header.
fn wav_duration_secs(wav: &[u8]) -> f64 {
    if wav.len() < 44 {
        return 0.0;
    }
    let byte_rate = u32::from_le_bytes([wav[28], wav[29], wav[30], wav[31]]) as f64;
    if byte_rate <= 0.0 {
        return 0.0;
    }
    // Find the "data" chunk size (skip any metadata chunks like ISFT).
    let mut data_size = 0u32;
    let mut i = 12usize;
    while i + 8 <= wav.len() {
        if &wav[i..i + 4] == b"data" {
            data_size = u32::from_le_bytes([wav[i + 4], wav[i + 5], wav[i + 6], wav[i + 7]]);
            break;
        }
        let sz = u32::from_le_bytes([wav[i + 4], wav[i + 5], wav[i + 6], wav[i + 7]]) as usize;
        i += 8 + sz + (sz & 1);
    }
    if data_size == 0 {
        data_size = (wav.len() - 44) as u32;
    }
    data_size as f64 / byte_rate
}
