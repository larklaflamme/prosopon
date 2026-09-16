# Avatar — Audio2Face-3D ARKit blendshape avatar (design)

**Status:** Design — reviewed, rulings folded in
**Date:** 2026-09-16

## Goal

Give the Prosopon client a talking face: a female VRM avatar whose mouth,
eyes, and idle motion are driven by the assistant's own TTS audio, via
NVIDIA Audio2Face-3D (A2F-3D) running on the H100.

The demo is judged on **how well the avatar behaves** — whether the mouth
moves with the words (lip sync), whether it blinks naturally, and whether it
reads as alive rather than a static puppet.

## Decisions (locked)

1. **Sync strategy: Option A (batch).** Buffer the full TTS response, run it
   through A2F once, get the complete blendshape track, then play audio +
   animation in lockstep. +1–2s latency, perfect sync. **Option B (streaming)
   is the ideal production version** — revisit when we build production.
2. **Scope: Phase 1** = lip sync + blink + idle motion. **Emotional reflection
   (Audio2Emotion) is ideal-for-production**, deferred.
3. **Transport: WebRTC, second data channel.** The blendshape track rides a
   dedicated `RTCDataChannel` over the existing `RTCPeerConnection`. WebRTC
   supports multiple data channels per connection, so no new network surface.
4. **Existing robot avatar: disconnected, code kept.** The SVG/CSS robot
   avatar is removed from the active UI but its code stays in the repo for
   rollback. The VRM face replaces it.

## Architecture

```
TTS (Kokoro) ──WAV──▶ A2F-3D NIM (H100) ──▶ 52 ARKit blendshapes (timecoded)
                                                    │
                                                    ▼
                              client: three.js + @pixiv/three-vrm
                              (VRoid female head, all 52 blendshapes)
```

Three new pieces:

1. **A2F bridge (server)** — a small service that takes a WAV, calls A2F-3D's
   `ProcessAudioStream` gRPC, and returns the blendshape track (timecoded
   52-float frames).
2. **Client renderer** — three.js + `@pixiv/three-vrm` in the Tauri webview,
   loading the VRoid female head.
3. **Animation driver** — plays the blendshape track in lockstep with the
   audio, plus procedural blink + idle motion.

## The gating unknown — RESOLVED (verified this turn)

The prior research left open whether the self-hosted NIM exposes the same
gRPC surface as the cloud NVCF endpoint. Verified against the actual repo
(`NVIDIA/Audio2Face-3D-Samples`):

- **`quick-start/docker-compose.yml`** runs the service with
  `network_mode: "host"` — the gRPC service binds directly to the host
  network, reachable on localhost, no port mapping.
- **Two-stage init:** `a2f-3d-init` generates TRT models (stylization +
  advanced config), then `a2f-3d-service` runs `a2f_pipeline.run`.
- **Client library:** `nvidia_ace` Python wheel (v1.2.0), installable from
  `proto/sample_wheel/` — this is the gRPC client for `ProcessAudioStream`.
- **Image:** `nvcr.io/nim/nvidia/audio2face-3d:1.3` (note: compose pins 1.3,
  not 2.0).

So: self-hosted is viable, and the bridge is a Python gRPC client using the
`nvidia_ace` wheel. No cloud dependency.

## Data flow (Option A, batch)

1. Server generates TTS WAV (already done in the pipeline).
2. Bridge sends the WAV to A2F-3D → receives timecoded blendshape frames
   (52 floats each).
3. Bridge returns both the WAV and the blendshape track to the client.
4. Client plays the WAV and drives the VRM blendshapes in lockstep (shared
   clock — see below).

## Clock synchronization (the load-bearing design)

This is the part that determines whether the mouth stays on the words. Two
runtimes are involved, and they do **not** share a clock by default:

- **Audio** plays in **Rust** (cpal/rodio) — its own sample clock.
- **Animation** renders in **JavaScript** (`requestAnimationFrame`) — the
  browser's frame clock.

These drift apart. The fix is a **single source of truth for "how far into
the utterance are we," and that source is the Rust audio position** — because
the audio is the thing the user hears, and the mouth must follow the sound,
never the other way around.

### Protocol

1. **Rust is the master clock.** When playback starts, Rust records `t0`
   (monotonic) and begins playing the WAV. It knows the exact sample position
   at all times.
2. **Rust pushes position updates** over the dedicated data channel at ~30 Hz:
   a small message `{ utterance_id, position_ms }`.
3. **JS indexes the track by position.** On each `requestAnimationFrame`, JS
   takes the latest `position_ms`, finds the two nearest A2F timecodes, and
   interpolates the 52 weights. JS never uses its own elapsed time for
   animation — it only uses the position Rust reports.
4. **End of utterance:** Rust sends `{ utterance_id, position_ms, done: true }`
   when the audio finishes; JS eases the face back to idle.

### Why this is robust

- **No drift:** JS time is anchored to audio position, not to `performance.now()`.
- **No clock negotiation:** we don't try to synchronize two clocks; we make
  one clock authoritative and have the other *follow* it.
- **Tolerant of jitter:** 30 Hz position updates are far faster than any
  visible mouth movement; interpolation smooths the gaps.
- **Batch mode makes it easy:** the full track is known up front, so JS can
  pre-index it and just look up by position.

### Fallback (if position push is too chatty)

If 30 Hz pushes are undesirable, Rust can send `t0` once and JS computes
`position = (now - t0)` — but this reintroduces drift (audio clock ≠ JS clock).
**Not recommended.** The 30 Hz push is ~a few hundred bytes/sec; it's free.

## Blink gating (resolved)

A2F already outputs `EyeBlinkLeft`/`EyeBlinkRight` as two of the 52
blendshapes — so during speech, A2F is *already* driving the blink from the
audio. Procedural blink must not fight it:

- **During speech (A2F active):** A2F's blink wins. Procedural blink is
  suppressed.
- **During idle/silence (no A2F output):** procedural blink runs (random
  2–6s intervals, ~100ms close, occasional double-blink).

This avoids double-blinking and jitter.

## Transport (resolved)

The blendshape track rides a **dedicated `RTCDataChannel`** over the existing
`RTCPeerConnection`. WebRTC supports multiple data channels per connection.

- **Ordered + reliable** (the default) is fine for batch mode — we send the
  complete track as a single payload (or a few chunks), not a live stream.
- Payload is tiny: 52 floats/frame ≈ 208 bytes/frame; a 5s utterance at 30fps
  is ~31 KB. Negligible.
- No new network surface, no WebSocket server, no port to manage.

## Components

### 1. A2F bridge (server, Python)
- gRPC client using `nvidia_ace` wheel.
- Input: WAV path. Output: JSON/msgpack of `[{timecode, weights[52]}, ...]`.
- Runs on the H100 box alongside the NIM container.
- **Verify at integration:** (a) batch mode works — feed the full WAV and
  drain the full track (A2F is a streaming model; confirm it accepts a full
  buffer); (b) input sample rate — Kokoro outputs 24 kHz, A2F may expect
  16 kHz; resample in the bridge if they differ.

### 2. Client renderer (Tauri webview)
- three.js + `@pixiv/three-vrm` (v3), loaded via CDN importmap (no build step
  for the demo).
- Loads `VRoid_V110_Female_v1.1.3.vrm` (hinzka/52blendshapes-for-VRoid-face).
- All 52 ARKit blendshapes present.
- **Verify at integration:** blendshape name mapping is 1:1 (A2F output names
  vs VRM clip names). Expected to hold (both use ARKit names), but confirm on
  first asset load and build a remap table if any differ.

### 3. Animation driver
- **Lip sync:** map the 52 blendshape weights onto the VRM's blendshapes each
  frame, indexed by the Rust-reported audio position.
- **Blink:** procedural, gated to idle/silence (see above).
- **Idle motion:** subtle head sway + eye saccades, low amplitude, so it
  reads alive between utterances.
- **Head motion during speech:** A2F outputs blendshapes only (no bone
  transforms), so the head is static during speech. Layer a subtle procedural
  head sway on top of A2F's blendshapes during speech too, to avoid stiffness.

## Emotion (deferred, but note the default)

A2F detects emotion from audio **by default** and folds it into the
blendshapes. We're deferring *explicit* emotional reflection, but we must
decide whether to suppress A2F's auto-emotion (clean lip sync only) or let it
through. **Verify at integration:** set `emotion_params` to suppress if we
want pure lip sync for Phase 1.

## Risks / open questions

1. **NIM pull is gated** — NGC API key + NVIDIA Software License. Must confirm
   we can pull `nvcr.io/nim/nvidia/audio2face-3d:1.3`.
2. **VRoid commercial terms** for the hinzka model — fine for internal demo,
   verify before public showing.
3. **Blendshape name mapping** — verify 1:1 at integration (see above).
4. **Latency budget** — Option A adds ~1–2s. Acceptable for demo, but measure
   the actual A2F inference time on the H100.
5. **"Alive" quality is the judgment criterion and the vaguest part.** Lip
   sync is deterministic (A2F does it); blink + idle motion is hand-tuned
   procedural animation, which is exactly what can look robotic. This is the
   highest *quality* risk, separate from the technical risks. Budget real
   tuning time here.

## Phasing

- **Phase 1 (this work):** lip sync + blink + idle motion. Batch mode.
- **Phase 2 (production):** streaming (Option B) + Audio2Emotion emotional
  reflection.
