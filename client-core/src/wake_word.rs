//! Wake-word detection via the openWakeWord Python sidecar.
//!
//! The detector spawns the Python sidecar (`sidecar/wake_word.py`), streams
//! the mic's 16 kHz mono f32 PCM to its stdin (converted to int16), and reads
//! its stdout. Each ``WAKE`` line becomes a wake event on the returned channel.
//!
//! The sidecar's stderr (debug scores, errors) is captured and forwarded to
//! the caller via the `on_log` callback, so it can be surfaced in the app's
//! logs panel rather than lost to the terminal.
//!
//! The detector consumes a [`MicSubscription`] from the shared [`MicBus`]
//! rather than opening its own mic, so it can run simultaneously with the STT
//! detector off the same audio. The sidecar I/O is blocking, so it runs on a
//! dedicated writer thread. Dropping the detector (or calling
//! [`WakeWordDetector::stop`]) kills the sidecar, which closes the pipes and
//! lets all threads exit.

use crate::config::WakeWordConfig;
use crate::mic_bus::MicSubscription;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WakeWordError {
    #[error("failed to spawn wake-word sidecar: {0}")]
    Spawn(#[from] std::io::Error),
}

/// A running wake-word detector.
///
/// Holds the sidecar child process and the worker threads. Wake events
/// are delivered on the `mpsc::Receiver<()>` returned by [`WakeWordDetector::start`].
pub struct WakeWordDetector {
    child: Child,
    writer: Option<thread::JoinHandle<()>>,
    reader: Option<thread::JoinHandle<()>>,
    stderr_reader: Option<thread::JoinHandle<()>>,
}

impl WakeWordDetector {
    /// Spawn the sidecar and begin streaming audio to it from `sub`.
    ///
    /// Returns the detector (for lifecycle) and a receiver that yields one
    /// `()` per wake-word detection. The mic itself is owned by the shared
    /// [`MicBus`], which is already confirmed open before this is called.
    ///
    /// `on_log` is invoked (from the sidecar's stderr reader thread and the
    /// writer thread) with each log line, so the caller can surface sidecar
    /// diagnostics in the UI.
    pub fn start(
        cfg: &WakeWordConfig,
        sub: MicSubscription,
        on_log: impl Fn(String) + Send + Clone + 'static,
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
            .stderr(Stdio::piped())
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
        let stderr = child.stderr.take().ok_or_else(|| {
            WakeWordError::Spawn(std::io::Error::new(
                std::io::ErrorKind::Other,
                "sidecar stderr unavailable",
            ))
        })?;

        let (wake_tx, wake_rx) = mpsc::channel::<()>();

        // Writer thread: streams f32 chunks -> int16 PCM bytes -> sidecar
        // stdin. The subscription is `Send`, so it can move onto this thread.
        let on_log_writer = on_log.clone();
        let writer = thread::spawn(move || {
            let mut stdin = stdin;
            let mut level_samples: u64 = 0;
            let mut level_sum_sq: f64 = 0.0;
            let mut level_peak: f32 = 0.0;
            while let Ok(chunk) = sub.next_chunk() {
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
                    on_log_writer(format!(
                        "mic level: rms={:.6} peak={:.6} samples={}",
                        rms, level_peak, level_samples
                    ));
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
        if let Some(s) = self.stderr_reader.take() {
            let _ = s.join();
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
