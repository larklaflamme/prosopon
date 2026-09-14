//! Speech-to-text via the Moonshine Voice Python sidecar.
//!
//! The detector spawns the Python sidecar (`sidecar/stt.py`), streams the
//! mic's 16 kHz mono f32 PCM to its stdin (converted to int16), and reads
//! its stdout. Each `FINAL <text>` line becomes a completed utterance on the
//! returned channel; `PARTIAL <text>` lines are forwarded to `on_log` for
//! live display.
//!
//! This mirrors `wake_word.rs` exactly: the mic capture and the sidecar I/O
//! are both blocking, so they run on dedicated threads. `cpal::Stream` is
//! `!Send` on CoreAudio (macOS), so the [`Mic`] is created *inside* the
//! writer thread and never crosses a thread boundary.

use crate::config::SttConfig;
use crate::mic::Mic;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SttError {
    #[error("failed to spawn STT sidecar: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("mic error: {0}")]
    Mic(#[from] crate::mic::MicError),
}

/// A running STT detector.
///
/// Holds the sidecar child process and the worker threads. Completed
/// utterances are delivered on the `mpsc::Receiver<String>` returned by
/// [`SttDetector::start`].
pub struct SttDetector {
    child: Child,
    writer: Option<thread::JoinHandle<()>>,
    reader: Option<thread::JoinHandle<()>>,
    stderr_reader: Option<thread::JoinHandle<()>>,
}

impl SttDetector {
    /// Spawn the sidecar, start the mic, and begin streaming audio to it.
    ///
    /// Returns the detector (for lifecycle) and a receiver that yields one
    /// `String` per completed utterance (the text after `FINAL `). Blocks
    /// until the mic is confirmed open (or fails), so a missing device /
    /// denied permission surfaces as an error here rather than silently.
    ///
    /// `on_log` is invoked (from the sidecar's stderr reader thread) with
    /// each log line, so the caller can surface sidecar diagnostics in the UI.
    pub fn start(
        cfg: &SttConfig,
        on_log: impl Fn(String) + Send + Clone + 'static,
    ) -> Result<(Self, mpsc::Receiver<String>), SttError> {
        let mut child = Command::new(&cfg.python)
            .arg(&cfg.sidecar_path)
            .arg("--language")
            .arg(&cfg.language)
            .arg("--model-arch")
            .arg(&cfg.model_arch)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let stdin = child.stdin.take().ok_or_else(|| {
            SttError::Spawn(std::io::Error::new(
                std::io::ErrorKind::Other,
                "sidecar stdin unavailable",
            ))
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            SttError::Spawn(std::io::Error::new(
                std::io::ErrorKind::Other,
                "sidecar stdout unavailable",
            ))
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            SttError::Spawn(std::io::Error::new(
                std::io::ErrorKind::Other,
                "sidecar stderr unavailable",
            ))
        })?;

        let (final_tx, final_rx) = mpsc::channel::<String>();

        // Writer thread: creates the Mic *here* (so it stays on this thread),
        // then streams f32 chunks -> int16 PCM bytes -> sidecar stdin.
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), crate::mic::MicError>>();
        let writer = thread::spawn(move || {
            let mic = match Mic::start() {
                Ok(m) => m,
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                    return;
                }
            };
            let _ = ready_tx.send(Ok(()));

            let mut stdin = stdin;
            while let Ok(chunk) = mic.next_chunk() {
                let bytes = f32_to_i16_bytes(&chunk);
                if stdin.write_all(&bytes).is_err() {
                    // Sidecar died (or was killed) — stop streaming.
                    break;
                }
            }
        });

        // Wait for the mic to open (or fail) before returning.
        match ready_rx.recv() {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(SttError::Mic(e));
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(SttError::Spawn(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "mic thread exited before reporting readiness",
                )));
            }
        }

        // Reader thread: sidecar stdout lines -> completed utterances.
        let reader = thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(l) => {
                        if let Some(text) = l.strip_prefix("FINAL ") {
                            if final_tx.send(text.trim().to_string()).is_err() {
                                break;
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        // Stderr reader thread: sidecar diagnostics -> on_log.
        let stderr_reader = thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                match line {
                    Ok(l) => on_log(l),
                    Err(_) => break,
                }
            }
        });

        Ok((
            Self {
                child,
                writer: Some(writer),
                reader: Some(reader),
                stderr_reader: Some(stderr_reader),
            },
            final_rx,
        ))
    }

    /// Kill the sidecar and join the worker threads.
    pub fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(w) = self.writer.take() {
            let _ = w.join();
        }
        if let Some(r) = self.reader.take() {
            let _ = r.join();
        }
        if let Some(s) = self.stderr_reader.take() {
            let _ = s.join();
        }
    }
}

impl Drop for SttDetector {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Convert f32 samples in [-1, 1] to little-endian int16 PCM bytes.
fn f32_to_i16_bytes(samples: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len() * 2);
    for &s in samples {
        let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}
