//! Speech-to-text via the Moonshine Voice Python sidecar.
//!
//! The detector spawns the Python sidecar (`sidecar/stt.py`), streams the
//! mic's 16 kHz mono f32 PCM to its stdin (converted to int16), and reads
//! its stdout. Each `FINAL <text>` line becomes a completed utterance on the
//! returned channel; `PARTIAL <text>` lines become an activity signal (see
//! [`SttEvent::Partial`]) so the caller can keep a warm session alive while
//! the user is still speaking.
//!
//! The detector consumes a [`MicSubscription`] from the shared [`MicBus`]
//! rather than opening its own mic, so it can run simultaneously with the
//! wake-word detector off the same audio. The sidecar I/O is blocking, so it
//! runs on a dedicated writer thread.
//!
//! # Feed gating
//!
//! The detector is meant to stay *warm* for a whole session (model loaded,
//! no cold start). To keep it from transcribing the wake word or the agent's
//! own playback, the writer thread is gated: when the gate is closed it
//! streams silence (int16 zeros) instead of real audio, which keeps the
//! sidecar's VAD calibrated without producing any transcript. The caller
//! opens the gate ([`SttDetector::set_listening`]) exactly when the state
//! machine enters `Listening`, and closes it when the utterance completes.

use crate::config::SttConfig;
use crate::mic_bus::MicSubscription;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SttError {
    #[error("failed to spawn STT sidecar: {0}")]
    Spawn(#[from] std::io::Error),
}

/// An event emitted by the STT sidecar.
#[derive(Debug, Clone)]
pub enum SttEvent {
    /// A completed utterance (the text after `FINAL `).
    Final(String),
    /// Speech is in progress (a `PARTIAL` line was emitted). Used as an
    /// activity signal so the caller can reset an inactivity deadline while
    /// the user is still speaking.
    Partial,
}

/// A running STT detector.
///
/// Holds the sidecar child process and the worker threads. Events are
/// delivered on the `mpsc::Receiver<SttEvent>` returned by
/// [`SttDetector::start`].
pub struct SttDetector {
    child: Child,
    writer: Option<thread::JoinHandle<()>>,
    reader: Option<thread::JoinHandle<()>>,
    stderr_reader: Option<thread::JoinHandle<()>>,
    /// Feed gate: `false` = stream silence, `true` = stream real audio.
    gate: Arc<AtomicBool>,
}

impl SttDetector {
    /// Spawn the sidecar and begin streaming audio to it from `sub`.
    ///
    /// Returns the detector (for lifecycle) and a receiver that yields
    /// [`SttEvent`]s: `Final` for each completed utterance, `Partial` for
    /// each in-progress transcript update. The mic itself is owned by the
    /// shared [`MicBus`], which is already confirmed open before this is
    /// called.
    ///
    /// The detector starts with the feed gate **closed** (streaming silence),
    /// so nothing is transcribed until the caller opens it with
    /// [`SttDetector::set_listening`].
    ///
    /// `on_log` is invoked (from the sidecar's stderr reader thread) with
    /// each log line, so the caller can surface sidecar diagnostics in the UI.
    pub fn start(
        cfg: &SttConfig,
        sub: MicSubscription,
        on_log: impl Fn(String) + Send + Clone + 'static,
    ) -> Result<(Self, mpsc::Receiver<SttEvent>), SttError> {
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

        let (event_tx, event_rx) = mpsc::channel::<SttEvent>();

        // Feed gate: starts closed (silence) so the wake word never reaches
        // STT. The conversation loop opens it when the machine enters
        // Listening.
        let gate = Arc::new(AtomicBool::new(false));

        // Writer thread: streams f32 chunks -> int16 PCM bytes -> sidecar
        // stdin. When the gate is closed it writes silence instead of real
        // audio, keeping the sidecar's VAD calibrated without transcribing.
        let gate_for_writer = gate.clone();
        let writer = thread::spawn(move || {
            let mut stdin = stdin;
            while let Ok(chunk) = sub.next_chunk() {
                let bytes = if gate_for_writer.load(Ordering::Relaxed) {
                    f32_to_i16_bytes(&chunk)
                } else {
                    vec![0u8; chunk.len() * 2] // silence (int16 zeros)
                };
                if stdin.write_all(&bytes).is_err() {
                    // Sidecar died (or was killed) — stop streaming.
                    break;
                }
            }
        });

        // Reader thread: sidecar stdout lines -> events.
        let reader = thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(l) => {
                        if let Some(text) = l.strip_prefix("FINAL ") {
                            if event_tx
                                .send(SttEvent::Final(text.trim().to_string()))
                                .is_err()
                            {
                                break;
                            }
                        } else if l.starts_with("PARTIAL ") {
                            if event_tx.send(SttEvent::Partial).is_err() {
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
                gate,
            },
            event_rx,
        ))
    }

    /// Open or close the feed gate. When `listening` is true, real mic audio
    /// reaches the sidecar; when false, silence is streamed instead.
    pub fn set_listening(&self, listening: bool) {
        self.gate.store(listening, Ordering::Relaxed);
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
