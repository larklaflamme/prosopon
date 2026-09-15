//! Audio playback — decode a WAV buffer and play it on the default output
//! device.
//!
//! The server returns WAV (PCM) audio from Kokoro. rodio's built-in `Decoder`
//! handles WAV natively via the `hound` crate (a default rodio feature), so
//! playback is a straight decode-and-play with no external codec dependency.
//!
//! `Playback` exposes *interruptible* playback (for barge-in): `stop()` halts
//! the sink immediately and `is_finished()` reports whether the source has
//! been exhausted. `play_wav` remains as a blocking convenience wrapper.

use std::io::Cursor;

/// An in-flight playback. Holds the output stream and sink alive; `stop()`
/// interrupts it, `is_finished()` reports completion.
pub struct Playback {
    _stream: rodio::OutputStream,
    sink: rodio::Sink,
}

impl Playback {
    /// Decode a WAV buffer and begin playing it. Returns immediately; the
    /// caller drives completion via `is_finished()` / `wait()`.
    pub fn start(bytes: &[u8]) -> Result<Self, String> {
        let cursor = Cursor::new(bytes.to_vec());
        let source = rodio::Decoder::new(cursor).map_err(|e| format!("decode WAV: {e}"))?;

        let (_stream, handle) = rodio::OutputStream::try_default()
            .map_err(|e| format!("open output stream: {e}"))?;

        let sink = rodio::Sink::try_new(&handle).map_err(|e| format!("sink: {e}"))?;
        sink.append(source);
        Ok(Self { _stream, sink })
    }

    /// Halt playback immediately (barge-in).
    pub fn stop(&self) {
        self.sink.stop();
    }

    /// True once the source has been fully played (or stopped).
    pub fn is_finished(&self) -> bool {
        self.sink.empty()
    }

    /// Block until playback completes.
    pub fn wait(&self) {
        self.sink.sleep_until_end();
    }
}

/// Decode a WAV buffer and play it to completion on the default output
/// device. Blocks until playback finishes (or fails), so callers can rely on
/// it as a natural "wait for the response to finish" barrier.
pub fn play_wav(bytes: &[u8]) -> Result<(), String> {
    let playback = Playback::start(bytes)?;
    playback.wait();
    Ok(())
}
