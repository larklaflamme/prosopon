//! Acoustic echo cancellation (AEC) via WebRTC's AEC3.
//!
//! When the assistant speaks, its own TTS playback is picked up by the
//! microphone as echo. AEC3 cancels that echo: we feed it the playback
//! (render/reference) signal and the mic (capture) signal, and it removes the
//! echo of the reference from the capture.
//!
//! Architecture:
//! - Playback decodes the WAV to 16 kHz mono and pushes it into a shared
//!   [`ReferenceBuffer`] (the "render" side).
//! - The mic bus capture thread owns the [`AecProcessor`] and, for each mic
//!   chunk, drains the reference buffer in lockstep and runs AEC3, fanning out
//!   the cleaned signal.
//!
//! Both sides are consumed at 16 kHz in real time, so the reference is drained
//! over the same wall-clock duration as playback. AEC3's delay estimation
//! absorbs the constant offset between the render and capture paths (output
//! latency + acoustic path).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use webrtc_audio_processing::config::{
    EchoCanceller, HighPassFilter, NoiseSuppression, NoiseSuppressionLevel,
};
use webrtc_audio_processing::{Config, Processor};

/// The sample rate the AEC operates at (Hz). Matches the mic bus (16 kHz).
pub const SAMPLE_RATE: u32 = 16_000;

/// One frame is 10 ms (WebRTC's fixed frame size).
const FRAME_SAMPLES: usize = SAMPLE_RATE as usize / 100; // 160

/// A shared reference (render) buffer. Playback pushes 16 kHz mono samples;
/// the AEC drains them in lockstep with capture processing.
#[derive(Default)]
pub struct ReferenceBuffer {
    inner: Mutex<VecDeque<f32>>,
}

impl ReferenceBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Push rendered (playback) samples, 16 kHz mono.
    pub fn push(&self, samples: &[f32]) {
        self.inner.lock().unwrap().extend(samples.iter().copied());
    }

    /// Drop all pending reference samples (e.g. on barge-in, when playback
    /// stops before the buffered reference has been consumed).
    pub fn clear(&self) {
        self.inner.lock().unwrap().clear();
    }

    /// Drain up to `n` samples; pad with silence if the buffer runs dry.
    fn drain(&self, n: usize) -> Vec<f32> {
        let mut buf = self.inner.lock().unwrap();
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            out.push(buf.pop_front().unwrap_or(0.0));
        }
        out
    }
}

/// The AEC processor. Owns the WebRTC [`Processor`] and the shared reference
/// buffer. `Send + Sync` (the underlying `Processor` is), so it can be shared
/// between the playback and capture threads via `Arc`.
pub struct AecProcessor {
    processor: Processor,
    reference: Arc<ReferenceBuffer>,
    /// Leftover capture samples that didn't fill a complete 10 ms frame.
    capture_buffer: Mutex<Vec<f32>>,
}

impl AecProcessor {
    /// Create an AEC processor at 16 kHz with AEC3 (full) + high-pass filter +
    /// moderate noise suppression.
    pub fn new(reference: Arc<ReferenceBuffer>) -> Result<Self, String> {
        let processor = Processor::new(SAMPLE_RATE).map_err(|e| format!("AEC init: {e}"))?;
        processor.set_config(Config {
            echo_canceller: Some(EchoCanceller::Full { stream_delay_ms: None }),
            high_pass_filter: Some(HighPassFilter { apply_in_full_band: true }),
            noise_suppression: Some(NoiseSuppression {
                level: NoiseSuppressionLevel::Moderate,
                analyze_linear_aec_output: false,
            }),
            ..Default::default()
        });
        Ok(Self {
            processor,
            reference,
            capture_buffer: Mutex::new(Vec::new()),
        })
    }

    /// Process a capture (mic) chunk in place, removing echo. The chunk is
    /// 16 kHz mono f32 of arbitrary length; partial frames are buffered
    /// internally so no samples are lost.
    pub fn process_capture(&self, chunk: &mut Vec<f32>) {
        // Extract complete 10 ms frames from the internal buffer + new chunk.
        let frames: Vec<Vec<f32>> = {
            let mut buf = self.capture_buffer.lock().unwrap();
            buf.extend_from_slice(chunk);
            let mut frames = Vec::new();
            while buf.len() >= FRAME_SAMPLES {
                frames.push(buf.drain(..FRAME_SAMPLES).collect());
            }
            frames
        };

        // Process each frame: feed the matching reference frame, then cancel.
        let mut out = Vec::with_capacity(frames.len() * FRAME_SAMPLES);
        for mut frame in frames {
            let reference = self.reference.drain(FRAME_SAMPLES);
            let _ = self.processor.analyze_render_frame([reference.as_slice()]);
            let _ = self.processor.process_capture_frame([frame.as_mut_slice()]);
            out.extend_from_slice(&frame);
        }
        *chunk = out;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The reference buffer pads with silence when drained past its contents.
    #[test]
    fn reference_buffer_pads_with_silence() {
        let buf = ReferenceBuffer::new();
        buf.push(&[1.0, 2.0, 3.0]);
        let drained = buf.drain(5);
        assert_eq!(drained, vec![1.0, 2.0, 3.0, 0.0, 0.0]);
    }

    /// `clear` empties the buffer.
    #[test]
    fn reference_buffer_clear() {
        let buf = ReferenceBuffer::new();
        buf.push(&[1.0, 2.0, 3.0]);
        buf.clear();
        assert_eq!(buf.drain(3), vec![0.0, 0.0, 0.0]);
    }

    /// AEC3 actually reduces echo: feed a reference tone, mix it into the
    /// capture, and confirm the processed capture has less of the tone.
    /// This exercises the real WebRTC AEC3 (not a mock).
    #[test]
    fn aec3_reduces_echo() {
        let reference = Arc::new(ReferenceBuffer::new());
        let aec = AecProcessor::new(reference.clone()).expect("AEC should init");

        // A 440 Hz reference tone.
        let render: Vec<f32> = (0..FRAME_SAMPLES)
            .map(|i| (i as f32 / FRAME_SAMPLES as f32 * 440.0 * 2.0 * std::f32::consts::PI).sin() * 0.5)
            .collect();

        // Capture = the same tone (pure echo) + a faint 220 Hz "speech".
        let mut capture: Vec<f32> = (0..FRAME_SAMPLES)
            .map(|i| {
                let echo = (i as f32 / FRAME_SAMPLES as f32 * 440.0 * 2.0 * std::f32::consts::PI).sin() * 0.5;
                let speech = (i as f32 / FRAME_SAMPLES as f32 * 220.0 * 2.0 * std::f32::consts::PI).sin() * 0.05;
                echo + speech
            })
            .collect();

        // Feed many frames so AEC3's adaptive filter converges.
        for _ in 0..200 {
            reference.push(&render);
            let mut frame = capture.clone();
            aec.process_capture(&mut frame);
            capture = frame;
        }

        // After convergence, the 440 Hz echo should be strongly attenuated.
        // Measure the residual power of the processed capture vs the raw echo.
        let residual_power: f32 = capture.iter().map(|x| x * x).sum::<f32>() / capture.len() as f32;
        let echo_power: f32 = render.iter().map(|x| x * x).sum::<f32>() / render.len() as f32;

        // The residual should be far below the echo power (the echo is gone,
        // leaving only the faint speech). Allow a generous margin for the
        // filter's convergence and the speech component.
        assert!(
            residual_power < echo_power * 0.1,
            "residual {residual_power:.6} not << echo {echo_power:.6}"
        );
    }
}
