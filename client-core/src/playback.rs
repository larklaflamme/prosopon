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
//!
//! For acoustic echo cancellation, [`Playback::start_with_reference`] also
//! decodes the WAV to 16 kHz mono and pushes it into a shared
//! [`ReferenceBuffer`], so the AEC can cancel the echo of the playback from
//! the mic.

use crate::aec::ReferenceBuffer;
use std::io::Cursor;
use std::sync::Arc;
use rodio::Source;

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

    /// Decode a WAV buffer, push a 16 kHz mono copy into the AEC reference
    /// buffer, and begin playing the audio (at its native rate). The reference
    /// is consumed by the AEC in real time, so it stays aligned with playback.
    pub fn start_with_reference(
        bytes: &[u8],
        reference: Arc<ReferenceBuffer>,
    ) -> Result<Self, String> {
        // Decode once to raw f32 samples.
        let cursor = Cursor::new(bytes.to_vec());
        let source = rodio::Decoder::new(cursor).map_err(|e| format!("decode WAV: {e}"))?;
        let sample_rate = source.sample_rate();
        let channels = source.channels();
        let samples: Vec<f32> = source.convert_samples().collect();

        // Reference: downmix to mono, resample to 16 kHz, push.
        let mono = downmix_mono(&samples, channels as usize);
        let mono_16k = resample_mono(&mono, sample_rate, 16_000);
        reference.push(&mono_16k);

        // Playback: native rate and channel count.
        let source = rodio::buffer::SamplesBuffer::new(channels, sample_rate, samples);
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

/// Downmix interleaved samples to mono by averaging channels.
fn downmix_mono(samples: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return samples.to_vec();
    }
    let mut mono = Vec::with_capacity(samples.len() / channels);
    for frame in samples.chunks_exact(channels) {
        let sum: f32 = frame.iter().sum();
        mono.push(sum / channels as f32);
    }
    mono
}

/// Resample mono f32 samples from `in_rate` to `out_rate` by linear
/// interpolation. Good enough for the AEC reference signal (AEC3 is robust to
/// minor resampling artifacts).
fn resample_mono(input: &[f32], in_rate: u32, out_rate: u32) -> Vec<f32> {
    if in_rate == out_rate || input.is_empty() {
        return input.to_vec();
    }
    let ratio = in_rate as f64 / out_rate as f64;
    let out_len = (input.len() as f64 / ratio).ceil() as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let pos = i as f64 * ratio;
        let idx = pos.floor() as usize;
        let frac = (pos - idx as f64) as f32;
        if idx + 1 < input.len() {
            out.push(input[idx] * (1.0 - frac) + input[idx + 1] * frac);
        } else if idx < input.len() {
            out.push(input[idx]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downmix_mono_averages_channels() {
        // Stereo: [1, 2], [3, 4] -> mono [1.5, 3.5].
        let stereo = vec![1.0, 2.0, 3.0, 4.0];
        assert_eq!(downmix_mono(&stereo, 2), vec![1.5, 3.5]);
    }

    #[test]
    fn downmix_mono_passthrough() {
        let mono = vec![1.0, 2.0, 3.0];
        assert_eq!(downmix_mono(&mono, 1), mono);
    }

    #[test]
    fn resample_mono_identity() {
        let samples = vec![0.1, 0.2, 0.3];
        assert_eq!(resample_mono(&samples, 16_000, 16_000), samples);
    }

    #[test]
    fn resample_mono_downsample_length() {
        // 48 kHz -> 16 kHz: 480 samples -> 160 samples.
        let samples: Vec<f32> = (0..480).map(|i| i as f32).collect();
        let out = resample_mono(&samples, 48_000, 16_000);
        assert_eq!(out.len(), 160);
    }
}
