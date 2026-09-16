# AEC Integration — WebRTC AEC3 in the mic bus

**Status:** Implemented, compiles clean, unit-tested, live-tested (passed).
**Date:** 2026-09-15

## What this is

Acoustic echo cancellation (AEC) wired into the client's mic bus. When the
assistant speaks, its own TTS playback is picked up by the microphone as echo.
AEC3 (WebRTC's echo canceller) removes that echo, so the wake-word detector and
STT don't hear the assistant's own voice.

## Architecture

Two new pieces, one shared buffer:

1. **`ReferenceBuffer`** (`client-core/src/aec.rs`) — a shared ring of 16 kHz
   mono f32 samples. Playback pushes into it; the AEC drains it.

2. **`AecProcessor`** (`client-core/src/aec.rs`) — wraps the
   `webrtc-audio-processing` crate's `Processor` at 16 kHz, configured with
   AEC3 (Full) + high-pass filter + moderate noise suppression. Lives in the
   mic bus capture thread.

The flow:

```
playback (TTS WAV) ──decode→ 16 kHz mono ──push→ ReferenceBuffer
                                                      │ (drained in lockstep)
mic capture ──→ AecProcessor.process_capture ──→ cleaned chunks ──→ subscribers
                                                      │ (wake word + STT)
```

## Key design decisions

- **16 kHz everywhere.** The mic bus already runs at 16 kHz (the wake word and
  STT expect it). The AEC processor is created at 16 kHz, and the reference is
  resampled to 16 kHz mono. This keeps the reference and capture on the same
  clock domain (macOS CoreAudio uses a single master clock), so the reference
  is drained at the same rate it is played.

- **Reference consumed in lockstep with capture.** The capture thread drains
  160 reference samples (10 ms) per 160-sample capture frame. Since capture
  runs in real time, the reference is consumed in real time, matching playback
  duration. When there's no playback, the buffer is empty and drains silence
  (correctly telling AEC3 "no echo").

- **Delay estimation, not a fixed delay.** `EchoCanceller::Full {
  stream_delay_ms: None }` lets AEC3 estimate the render→capture delay itself
  (output latency + acoustic path), rather than hard-coding it.

- **Playback plays at native rate; reference is a 16 kHz copy.** The reference
  is a resampled copy of the same audio, so it spans the same wall-clock
  duration as playback. No need to resample the audible playback.

- **Barge-in clears the reference.** When the user barges in, playback stops
  early but the reference buffer still holds the unplayed tail. We clear it so
  AEC3 doesn't try to cancel echo that never happened (which would attenuate
  the user's speech).

## Files changed

- `client-core/src/aec.rs` (new) — `ReferenceBuffer` + `AecProcessor`.
- `client-core/src/mic_bus.rs` — `MicBus::start_with_aec`.
- `client-core/src/playback.rs` — `Playback::start_with_reference` + downmix +
  resample helpers.
- `client-core/src/lib.rs` — `pub mod aec`.
- `client-core/Cargo.toml` — `webrtc-audio-processing = { version = "2.1.0",
  features = ["bundled"] }`.
- `src-tauri/src/lib.rs` — wires reference + AEC into the conversation loop.

## Verification

- `cargo test` in client-core: **10 passed, 0 failed**, including
  `aec3_reduces_echo`, which feeds a 440 Hz reference tone mixed into the
  capture and asserts the processed capture's residual power is < 10% of the
  echo power. This exercises the real WebRTC AEC3, not a mock.
- `cargo build` in src-tauri: clean, zero warnings.

## Not yet done

- **Live testing (done).** Verified on the real acoustic path (speaker → room
  → mic): no false wake during playback, clean barge-in under echo, no STT
  self-trigger. AEC3 is confirmed working end-to-end.
- **Config toggle.** AEC is always-on in the conversation loop. A
  `conversation.aec_enabled` flag would let us A/B test on/off.
- **Tuning.** Noise-suppression level (currently Moderate) and whether to add
  AGC are open questions.

## Toolchain note

The `webrtc-audio-processing` crate's `bundled` build needs meson + ninja +
pkg-config (not cmake, as the earlier design doc said), plus a
`CPLUS_INCLUDE_PATH` fix for macOS CommandLineTools. See
`data/notes/prosopon-aec-toolchain-fix.md` (server) for the full story.
