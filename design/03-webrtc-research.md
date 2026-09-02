# 03 — WebRTC / Streaming Stack Research

## Problem

Stream the avatar's audio (and, depending on the architecture, video or facial
state) to a client in real time, with low latency.

## The two candidate stacks

### Option A: webrtc-rs (raw WebRTC, P2P)

`webrtc-rs` is a pure-Rust WebRTC implementation (a Pion rewrite).

- **v0.21** (current): async-friendly, runtime-agnostic (Tokio/smol), built on a
  Sans-I/O core (`rtc` crate) with ~95% W3C API compliance.
- **Peer-to-peer**: one peer connection between the Rust backend and the
  browser. No server in the middle.
- **License**: MIT/Apache-2.0.
- **Fit**: ideal for a single-user avatar — one human meets one Skye.

### Option B: LiveKit (SFU)

LiveKit is a full WebRTC SFU (selective forwarding unit) with a Rust SDK.

- **Multi-user**: rooms, many participants, simulcast, dynacast, hardware
  enc/dec (H.264/H.265/AV1 on NVIDIA/AMD/Jetson).
- **Heavier**: requires running a LiveKit server (self-hosted or cloud).
- **Fit**: overkill for one user, but the right call if we ever want many
  people to meet Skye simultaneously, or want production-grade scaling.

## Recommendation

**Start with webrtc-rs (Option A).** For v1 — one Lark, one Skye — a single
peer connection is all we need, and it keeps the whole system self-contained
(no external SFU to run). LiveKit is the natural upgrade path if/when we go
multi-user; the WebRTC layer should be isolated behind a small trait so we can
swap the transport without rewriting the avatar logic.

## What actually flows over the wire

Depends on the rendering decision (04). Two shapes:

**If rendering in browser (recommended v1):**
- **Audio track** (Opus) — TTS output, Rust → browser.
- **Data channel** — facial state: ARKit blendshape weights + lip-sync viseme
  timing + blink/gaze, Rust → browser (or browser-local if tracking is in-browser).
- **Data channel (reverse)** — user input / control, browser → Rust.

**If rendering in Rust:**
- **Video track** (H.264/VP8) — rendered frames, Rust → browser.
- **Audio track** (Opus) — TTS output.
- **Data channel (reverse)** — webcam frames or blendshapes, browser → Rust.

## Signaling

WebRTC needs a signaling channel to exchange SDP offers/answers and ICE
candidates. For a local/single-user system, a tiny WebSocket signaling server
(or even a manual copy-paste of the SDP) suffices. webrtc-rs ships examples for
this. No TURN server needed on a LAN; a STUN server (public, e.g. Google's) is
enough for NAT traversal.

## Codec notes

- **Audio**: Opus is the WebRTC default and ideal for speech (low bitrate, low
  latency). Kokoro outputs 24 kHz PCM; encode to Opus before sending.
- **Video** (only if Rust-rendered): H.264 for broad compatibility, or VP8/VP9.
  Hardware encode on the laptop GPU if available.

## Open questions for Lark

- Is **multi-user** (many people meeting Skye at once) a near-term goal? If so,
  we should consider LiveKit from the start rather than migrating later.
- Is the client always a **browser**, or do we also want a native client?

*Sources: github.com/webrtc-rs/webrtc (README, fetched 2026-09-01);
github.com/livekit/rust-sdks (README, fetched 2026-09-01).*
