//! Audio playback — decode a WAV buffer and play it on the default output
//! device.
//!
//! The server returns WAV (PCM) audio from Kokoro. rodio's built-in `Decoder`
//! handles WAV natively via the `hound` crate (a default rodio feature), so
//! playback is a straight decode-and-play with no external codec dependency.

use std::io::Cursor;

/// Decode a WAV buffer and play it to completion on the default output
/// device. Blocks until playback finishes (or fails), so callers can rely on
/// it as a natural "wait for the response to finish" barrier.
pub fn play_wav(bytes: &[u8]) -> Result<(), String> {
    let cursor = Cursor::new(bytes.to_vec());
    let source = rodio::Decoder::new(cursor).map_err(|e| format!("decode WAV: {e}"))?;

    // The stream must stay alive for the duration of playback; the Sink
    // blocks until the source is exhausted, so binding it here is enough.
    let (_stream, handle) = rodio::OutputStream::try_default()
        .map_err(|e| format!("open output stream: {e}"))?;

    let sink = rodio::Sink::try_new(&handle).map_err(|e| format!("sink: {e}"))?;
    sink.append(source);
    sink.sleep_until_end();
    Ok(())
}
