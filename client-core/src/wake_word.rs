//! Wake-word detection via the openWakeWord Python sidecar.
//!
//! The detector spawns the Python sidecar (`sidecar/wake_word.py`), streams
//! the mic's 16 kHz mono f32 PCM to its stdin (converted to int16), and reads
//! its stdout. Each ``WAKE`` line becomes a wake event on the returned channel.
//!
//! The mic capture and the sidecar I/O are both blocking, so they run on
//! dedicated threads. Dropping the detector (or calling [`WakeWordDetector::stop`])
//! kills the sidecar, which closes the pipes and lets both threads exit.
//!
//! Note: `cpal::Stream` is `!Send` on CoreAudio (macOS), so the [`Mic`] is
//! created *inside* the writer thread and never crosses a thread boundary.

use crate::config::WakeWordConfig;
use crate::mic::Mic;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WakeWordError {
    #[error("failed to spawn wake-word sidecar: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("mic error: {0}")]
    Mic(#[from] crate::mic::MicError),
}

/// A running wake-word detector.
///
/// Holds the sidecar child process and the two worker threads. Wake events
/// are delivered on the `mpsc::Receiver<()>` returned by [`WakeWordDetector::start`].
pub struct WakeWordDetector {
    child: Child,
    writer: Option<thread::JoinHandle<()>>,
    reader: Option<thread::JoinHandle<()>>,
}

impl WakeWordDetector {
    /// Spawn the sidecar, start the mic, and begin streaming audio to it.
    ///
    /// Returns the detector (for lifecycle) and a receiver that yields one
    /// `()` per wake-word detection. Blocks until the mic is confirmed open
    /// (or fails), so a missing device / denied permission surfaces as an
    /// error here rather than silently.
    pub fn start(
        cfg: &WakeWordConfig,
    ) -> Result<(Self, mpsc::Receiver<()>), WakeWordError> {
        let mut child = Command::new(&cfg.python)
            .arg(&cfg.sidecar_path)
            .arg("--model")
            .arg(&cfg.model)
            .arg("--threshold")
            .arg(&cfg.threshold.to_string())
            .arg("--debug")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;

        let stdin = child.stdin.take().ok_or_else(|| {
            WakeWordError::Spawn(std::io::Error::new(
                std::io::ErrorKind::Other,
                "sidecar stdin unavailable",
            ))
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            WakeWordError::Spawn(std::io::Error::new(
                std::io::ErrorKind::Other,
                "sidecar stdout unavailable",
            ))
        })?;

        let (wake_tx, wake_rx) = mpsc::channel::<()>();

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
            let mut level_samples: u64 = 0;
            let mut level_sum_sq: f64 = 0.0;
            let mut level_peak: f32 = 0.0;
            while let Ok(chunk) = mic.next_chunk() {
                // Level meter: accumulate RMS/peak, log once per second.
                for &s in &chunk {
                    level_sum_sq += (s as f64) * (s as f64);
                    if s.abs() > level_peak {
                        level_peak = s.abs();
                    }
                }
                level_samples += chunk.len() as u64;
                if level_samples >= 16_000 {
                    let rms = (level_sum_sq / level_samples as f64).sqrt();
                    eprintln!(
                        "[prosopon] mic level: rms={:.6} peak={:.6} samples={}",
                        rms, level_peak, level_samples
                    );
                    level_samples = 0;
                    level_sum_sq = 0.0;
                    level_peak = 0.0;
                }

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
                return Err(WakeWordError::Mic(e));
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(WakeWordError::Spawn(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "mic thread exited before reporting readiness",
                )));
            }
        }

        // Reader thread: sidecar stdout lines -> wake events.
        let reader = thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(l) if l.trim() == "WAKE" => {
                        if wake_tx.send(()).is_err() {
                            break;
                        }
                    }
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
        });

        Ok((
            Self {
                child,
                writer: Some(writer),
                reader: Some(reader),
            },
            wake_rx,
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
    }
}

impl Drop for WakeWordDetector {
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
