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

        let result = self.run_bridge(&tmp).await;

        let _ = tokio::fs::remove_file(&tmp).await;
        result
    }

    /// Spawn `a2f-bridge <wav> <endpoint>` and capture its stdout, bounded by
    /// `self.timeout`.
    async fn run_bridge(&self, wav_path: &Path) -> Option<String> {
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

        match tokio::time::timeout(self.timeout, read_fut).await {
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
