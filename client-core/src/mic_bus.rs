//! A shared microphone bus — one capture thread, many subscribers.
//!
//! The wake-word detector and the STT detector both need the same 16 kHz
//! mono f32 PCM stream. Before the bus, each opened its own [`Mic`], which
//! meant only one could own the device at a time (hence the sequential
//! handoff in the conversation loop). The bus fixes that: a single capture
//! thread owns the [`Mic`] and fans every chunk out to all subscribers, so
//! the wake-word sidecar and the STT sidecar can run *simultaneously* off the
//! same audio.
//!
//! The critical constraint is `cpal::Stream` being `!Send` on CoreAudio
//! (macOS). The [`Mic`] therefore never leaves the capture thread. What
//! *does* cross threads is the [`MicSubscription`] — a plain
//! `mpsc::Receiver<Vec<f32>>`, which is `Send` — so subscribers can be
//! created on any thread and consumed on any other thread.

use crate::mic::{Mic, MicError};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

/// A shared microphone bus.
///
/// One capture thread owns the [`Mic`] and fans every chunk out to all
/// registered subscribers. Subscribers register with [`MicBus::subscribe`],
/// which returns a [`MicSubscription`] that yields a copy of every chunk.
///
/// Dropping the bus (or calling [`MicBus::stop`]) stops capture and
/// disconnects every subscriber, so their `next_chunk()` returns
/// [`MicError::StreamStopped`].
pub struct MicBus {
    subscribers: Arc<Mutex<Vec<mpsc::Sender<Vec<f32>>>>>,
    stop_tx: mpsc::Sender<()>,
    capture: Option<thread::JoinHandle<()>>,
}

/// A subscription to the mic bus. Yields a copy of every captured chunk.
///
/// Dropping the subscription unregisters it: the capture thread notices the
/// dead sender on the next chunk and removes it.
pub struct MicSubscription {
    rx: mpsc::Receiver<Vec<f32>>,
}

impl MicSubscription {
    /// Block until the next chunk of samples arrives.
    pub fn next_chunk(&self) -> Result<Vec<f32>, MicError> {
        self.rx.recv().map_err(|_| MicError::StreamStopped)
    }

    /// Return the next chunk if one is already available, without blocking.
    pub fn try_next_chunk(&self) -> Result<Option<Vec<f32>>, MicError> {
        match self.rx.try_recv() {
            Ok(chunk) => Ok(Some(chunk)),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => Err(MicError::StreamStopped),
        }
    }
}

impl MicBus {
    /// Open the default input device on a dedicated capture thread and begin
    /// fanning chunks out to subscribers. Blocks until the mic is confirmed
    /// open (or fails), so a missing device / denied permission surfaces as
    /// an error here rather than silently.
    pub fn start() -> Result<Self, MicError> {
        let subscribers: Arc<Mutex<Vec<mpsc::Sender<Vec<f32>>>>> =
            Arc::new(Mutex::new(Vec::new()));
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), MicError>>();

        let subs = Arc::clone(&subscribers);
        let capture = thread::spawn(move || {
            // The Mic is created *here* and never leaves this thread.
            let mic = match Mic::start() {
                Ok(m) => m,
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                    return;
                }
            };
            let _ = ready_tx.send(Ok(()));

            loop {
                // Stop signal is checked once per chunk (~20 ms granularity),
                // which is fine — the mic delivers chunks continuously.
                if stop_rx.try_recv().is_ok() {
                    break;
                }
                match mic.next_chunk() {
                    Ok(chunk) => {
                        let mut subs = subs.lock().unwrap();
                        // Fan out a copy to every live subscriber; drop dead ones.
                        subs.retain(|tx| tx.send(chunk.clone()).is_ok());
                    }
                    Err(_) => break, // mic stopped unexpectedly
                }
            }

            // On exit, drop every subscriber so their `next_chunk()` returns
            // `StreamStopped` rather than blocking forever.
            subs.lock().unwrap().clear();
        });

        // Wait for the mic to open (or fail) before returning.
        match ready_rx.recv() {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                let _ = capture.join();
                return Err(e);
            }
            Err(_) => return Err(MicError::StreamStopped),
        }

        Ok(Self {
            subscribers,
            stop_tx,
            capture: Some(capture),
        })
    }

    /// Register a new subscriber. Returns a [`MicSubscription`] that yields a
    /// copy of every chunk captured from now on.
    pub fn subscribe(&self) -> MicSubscription {
        let (tx, rx) = mpsc::channel::<Vec<f32>>();
        self.subscribers.lock().unwrap().push(tx);
        MicSubscription { rx }
    }

    /// Stop capture and disconnect all subscribers. Idempotent.
    pub fn stop(&mut self) {
        let _ = self.stop_tx.send(());
        if let Some(handle) = self.capture.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for MicBus {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bus cannot be exercised without a real mic, but the subscription
    /// plumbing (register → disconnect on drop) is testable in isolation.
    #[test]
    fn subscription_disconnects_when_bus_stops() {
        // Build the internals by hand to avoid needing a real audio device.
        let subscribers: Arc<Mutex<Vec<mpsc::Sender<Vec<f32>>>>> =
            Arc::new(Mutex::new(Vec::new()));
        let (tx, rx) = mpsc::channel::<Vec<f32>>();
        subscribers.lock().unwrap().push(tx);

        let sub = MicSubscription { rx };

        // Simulate the capture thread exiting: clear the subscriber list.
        subscribers.lock().unwrap().clear();

        // The subscription's receiver is now disconnected.
        assert!(matches!(
            sub.next_chunk(),
            Err(MicError::StreamStopped)
        ));
    }
}
