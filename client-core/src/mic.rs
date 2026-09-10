//! Mic capture via cpal — 16 kHz mono f32 PCM.
//!
//! This is the exact format openWakeWord (wake word) and Moonshine (STT)
//! expect. cpal does NOT resample: requesting 16 kHz directly fails on
//! devices that don't natively support it (macOS CoreAudio defaults to
//! 44.1/48 kHz). So we capture at the device's NATIVE config and resample
//! to 16 kHz mono f32 in the callback.
//!
//! The capture runs on cpal's internal audio thread and forwards samples to
//! a channel. The consumer (wake-word detector, STT, or a test harness)
//! pulls chunks with [`Mic::next_chunk`]. Dropping the [`Mic`] stops capture.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleFormat};
use std::sync::mpsc;
use thiserror::Error;

/// Sample rate the wake word and STT expect (Hz).
pub const SAMPLE_RATE: u32 = 16_000;
/// Channel count (mono).
pub const CHANNELS: u16 = 1;

#[derive(Debug, Error)]
pub enum MicError {
    #[error("no default input device found")]
    NoInputDevice,
    #[error("failed to get the device's default input config: {0}")]
    DefaultConfig(#[from] cpal::DefaultStreamConfigError),
    #[error("failed to build the input stream: {0}")]
    BuildStream(#[from] cpal::BuildStreamError),
    #[error("failed to start the input stream: {0}")]
    PlayStream(#[from] cpal::PlayStreamError),
    #[error("unsupported sample format: {0:?}")]
    UnsupportedFormat(cpal::SampleFormat),
    #[error("mic stream stopped unexpectedly")]
    StreamStopped,
}

/// A live microphone capture stream.
///
/// Samples arrive as `Vec<f32>` chunks (mono, 16 kHz) over an internal
/// channel. Dropping the [`Mic`] stops capture.
pub struct Mic {
    _stream: cpal::Stream,
    rx: mpsc::Receiver<Vec<f32>>,
}

/// Streaming linear resampler from `in_rate` to `out_rate` (mono f32).
///
/// Maintains state across chunks so fractional sample positions carry over
/// correctly. Handles arbitrary ratios; when `in_rate` is an exact multiple
/// of `out_rate` (e.g. 48k → 16k) it degenerates to clean decimation.
struct LinearResampler {
    step: f64,       // input samples per output sample = in_rate / out_rate
    pos: f64,        // absolute position of the next output sample (input units)
    prev: f32,       // last input sample seen
    have_prev: bool, // whether `prev` is valid
    next_index: u64, // absolute index of the next input sample
}

impl LinearResampler {
    fn new(in_rate: u32, out_rate: u32) -> Self {
        Self {
            step: in_rate as f64 / out_rate as f64,
            pos: 0.0,
            prev: 0.0,
            have_prev: false,
            next_index: 0,
        }
    }

    fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        for &s in input {
            let i = self.next_index;
            self.next_index += 1;
            if !self.have_prev {
                self.prev = s;
                self.have_prev = true;
                continue;
            }
            // Interval [i-1, i] with values [prev, s].
            while self.pos < i as f64 {
                let frac = self.pos - (i as f64 - 1.0);
                let v = self.prev * (1.0 - frac as f32) + s * (frac as f32);
                out.push(v);
                self.pos += self.step;
            }
            self.prev = s;
        }
    }
}

/// Downmix interleaved samples to mono f32.
fn to_mono_f32<T: cpal::Sample>(data: &[T], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return data.iter().map(|s| s.to_float_sample().to_sample::<f32>()).collect();
    }
    let mut mono = Vec::with_capacity(data.len() / channels);
    for frame in data.chunks_exact(channels) {
        let sum: f32 = frame.iter().map(|s| s.to_float_sample().to_sample::<f32>()).sum();
        mono.push(sum / channels as f32);
    }
    mono
}

impl Mic {
    /// Open the default input device and start capturing at 16 kHz mono f32.
    pub fn start() -> Result<Self, MicError> {
        let host = cpal::default_host();
        let device = host.default_input_device().ok_or(MicError::NoInputDevice)?;

        // Capture at the device's native config, then resample in the callback.
        let supported = device.default_input_config()?;
        let native_rate = supported.sample_rate().0;
        let sample_format = supported.sample_format();
        let channels = supported.channels() as usize;
        let config: cpal::StreamConfig = supported.config();

        let (tx, rx) = mpsc::channel::<Vec<f32>>();

        let err_fn = |err| eprintln!("[prosopon] mic stream error: {err}");

        let stream = match sample_format {
            SampleFormat::F32 => {
                let mut resampler = LinearResampler::new(native_rate, SAMPLE_RATE);
                device.build_input_stream(
                    &config,
                    move |data: &[f32], _| {
                        let mono = to_mono_f32(data, channels);
                        let mut out = Vec::new();
                        resampler.process(&mono, &mut out);
                        if !out.is_empty() {
                            let _ = tx.send(out);
                        }
                    },
                    err_fn,
                    None,
                )?
            }
            SampleFormat::I16 => {
                let mut resampler = LinearResampler::new(native_rate, SAMPLE_RATE);
                device.build_input_stream(
                    &config,
                    move |data: &[i16], _| {
                        let mono = to_mono_f32(data, channels);
                        let mut out = Vec::new();
                        resampler.process(&mono, &mut out);
                        if !out.is_empty() {
                            let _ = tx.send(out);
                        }
                    },
                    err_fn,
                    None,
                )?
            }
            SampleFormat::U16 => {
                let mut resampler = LinearResampler::new(native_rate, SAMPLE_RATE);
                device.build_input_stream(
                    &config,
                    move |data: &[u16], _| {
                        let mono = to_mono_f32(data, channels);
                        let mut out = Vec::new();
                        resampler.process(&mono, &mut out);
                        if !out.is_empty() {
                            let _ = tx.send(out);
                        }
                    },
                    err_fn,
                    None,
                )?
            }
            other => return Err(MicError::UnsupportedFormat(other)),
        };

        stream.play()?;

        Ok(Mic { _stream: stream, rx })
    }

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

/// List the names of all available input devices (for debugging).
pub fn list_input_devices() -> Vec<String> {
    let host = cpal::default_host();
    match host.input_devices() {
        Ok(devices) => devices.filter_map(|d| d.name().ok()).collect(),
        Err(_) => Vec::new(),
    }
}
