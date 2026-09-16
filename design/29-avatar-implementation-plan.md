# Avatar — Audio2Face-3D implementation & test plan

**Status:** Plan — for review (v2: failure modes + config added)
**Date:** 2026-09-16
**Depends on:** `prosopon-avatar-design.md` (reviewed, rulings folded in)

## Goal

Ship Phase 1: a female VRM avatar whose mouth, eyes, and idle motion are
driven by the assistant's own TTS audio via A2F-3D on the H100. Judged on
**how well the avatar behaves** — lip sync, natural blink, reads as alive.

**Non-negotiable principle:** the avatar is an *enhancement* on top of a
working voice assistant. A failure in the avatar layer must never break the
voice assistant. Every failure mode below degrades to audio-only + procedural
idle motion, never to silence.

## Ground truth (verified against the actual repo this turn)

Real hook points, not assumptions:

| Piece | File | What it does today |
|-------|------|--------------------|
| Server pipeline | `server/src/pipeline.rs` | `PipelineOutput { reply, audio }` — WAV bytes from Kokoro |
| Server WebRTC | `server/src/webrtc.rs` | `send_audio()` — `audio:<len>` header + binary chunks over the data channel |
| Client WebRTC | `client-core/src/webrtc_client.rs` | creates the data channel, reassembles audio |
| Client playback | `client-core/src/playback.rs` | rodio WAV playback, `start_with_reference()` for AEC |
| Client UI | `ui/main.js` + `index.html` + `styles.css` | robot/orb avatar via Tauri events |

## Phases (ordered by risk, not by file)

### Phase 0 — NIM container + A2F bridge (server, H100) — RISKIEST

This is the unknown everything else hangs off. Do it first, in isolation.

1. **Pull the NIM** — `nvcr.io/nim/nvidia/audio2face-3d:1.3`. GATED: NGC API
   key + NVIDIA Software License. Confirm the pull works before any code.
2. **Stand up the container** — `quick-start/docker-compose.yml`, two-stage
   init (`a2f-3d-init` → TRT models, then `a2f-3d-service`). `network_mode:
   host`, so gRPC is on localhost.
3. **Install the client wheel** — `nvidia_ace` v1.2.0 from
   `proto/sample_wheel/`.
4. **Write the bridge** — a small Python gRPC client:
   - Input: WAV path (or bytes).
   - Calls `ProcessAudioStream` (bidirectional streaming).
   - Output: `[{timecode_ms, weights[52]}, ...]` as JSON/msgpack.
   - **Structured logging** — log request size, duration, frame count, and
     any error, to a single grep-able line per call.
5. **Smoke test** — feed a Kokoro WAV, confirm a plausible track comes back.

**Verify at integration (Phase 0 gates):**
- (a) **Batch mode works** — A2F is a streaming model; confirm it accepts a
  full buffer and drains a full track.
- (b) **Sample rate** — Kokoro = 24 kHz; A2F may expect 16 kHz. Resample in
  the bridge if they differ.
- (c) **Blendshape name mapping** — A2F output names vs VRM clip names, 1:1.
- (d) **Emotion suppression** — A2F auto-detects emotion by default; decide
  whether to suppress via `emotion_params` for clean lip sync.

### Phase 1 — Server: generate + ship the blendshape track

1. **Extend `PipelineOutput`** — add `blendshapes: Option<Vec<Frame>>` where
   `Frame = { timecode_ms: f32, weights: [f32; 52] }`.
2. **Call the bridge** from `pipeline.rs` after TTS — pass the WAV, get the
   track back. (Bridge is a localhost gRPC/HTTP call on the H100 box.)
   **With a timeout** (see Failure modes FM-1).
3. **Second data channel** in `webrtc.rs` — create a dedicated
   `RTCDataChannel` (label `"blendshapes"`), send the track as one
   ordered+reliable payload (or a few chunks). Mirror of `send_audio()`.

### Phase 2 — Client: VRM renderer + animation driver + clock sync

1. **Replace the orb with a VRM face** in `ui/`:
   - three.js + `@pixiv/three-vrm` (v3) via CDN importmap (no build step).
   - Load `VRoid_V110_Female_v1.1.3.vrm` (hinzka repo).
   - Keep the robot/orb code in the repo (rollback), disconnect from UI.
   - **Runtime rollback toggle** — a flag (config or UI) to switch robot ↔
     VRM without a rebuild (see Config §C-6).
2. **Second data channel receive** in `webrtc_client.rs` — mirror the audio
   reassembly for the blendshape track.
3. **Clock sync (the load-bearing piece)** — Rust is master clock:
   - `playback.rs`: record `t0` at playback start; expose current position
     (sample position → ms).
   - Push `{ utterance_id, position_ms }` at ~30 Hz over a **dedicated
     control channel** (label `"control"`), NOT the blendshape channel. This
     avoids position updates queueing behind the large ordered track payload.
   - `main.js`: on each `requestAnimationFrame`, index the track by the
     latest `position_ms`, interpolate the 52 weights. JS never uses its own
     elapsed time.
   - End of utterance: `done: true`, ease back to idle.
   - **Barge-in cancel** — on barge-in, Rust sends
     `{ utterance_id, done: true, cancelled: true }`; JS invalidates the
     track and eases to idle (see Failure modes FM-3).

### Phase 3 — Blink gating + idle motion + head sway

1. **Blink gating** — procedural blink only during idle/silence; A2F's
   `EyeBlinkLeft/Right` win during speech.
2. **Idle motion** — subtle head sway + eye saccades between utterances.
3. **Head sway during speech** — A2F outputs blendshapes only (no bone
   transforms), so layer a subtle procedural head sway on top during speech
   to avoid stiffness.

## Failure modes & recovery

The invariant: **avatar failure degrades to audio-only + procedural idle,
never to a broken assistant.** Each mode has a detection signal and a
recovery action.

| ID | Failure | Detection | Recovery |
|----|---------|-----------|----------|
| FM-1 | Bridge call hangs / NIM down | timeout on bridge call (default 3s) | ship audio with `blendshapes = None`; client falls back to procedural idle (no lip sync) |
| FM-2 | Bridge returns empty / malformed track | frame count < 2, or parse error | treat as `None`; same fallback as FM-1 |
| FM-3 | Barge-in mid-utterance | existing barge-in signal | Rust sends `{ utterance_id, done: true, cancelled: true }`; JS invalidates track, eases to idle |
| FM-4 | Blendshape data channel fails to open | channel `onerror` / open timeout | client runs procedural idle only; audio unaffected (separate channel) |
| FM-5 | Clock drift detected at runtime | per-utterance max \|position_ms − expected\| logged | log + flag for tuning; if drift > threshold, snap to `done` and ease to idle |
| FM-6 | VRM model fails to load | three-vrm load error | fall back to robot/orb avatar (code kept); audio unaffected |
| FM-7 | A2F returns fewer than 52 weights | weight array length mismatch | pad missing weights with 0.0, log a warning |

**Graceful-degradation path (the one that matters):** FM-1 and FM-2 both
converge on "audio plays, avatar does procedural idle motion, no lip sync."
This is the behavior that keeps the demo alive when the GPU model misbehaves.

## Configuration parameters

Tunable behavior, all with defaults, so the demo can be tuned without code
changes. Grouped by where they live.

### Server (bridge + pipeline)

| Key | Default | Meaning |
|-----|---------|---------|
| C-1 `bridge.timeout_ms` | 3000 | max wait for the A2F bridge call before FM-1 fallback |
| C-2 `bridge.endpoint` | `localhost:50051` | gRPC endpoint of the A2F NIM |
| C-3 `bridge.sample_rate` | 24000 | Kokoro output rate; bridge resamples to A2F's expected rate if different |
| C-4 `avatar.enabled` | true | master switch; false = ship audio only, no bridge call |

### Client (renderer + animation)

| Key | Default | Meaning |
|-----|---------|---------|
| C-5 `avatar.mode` | `vrm` | `vrm` \| `robot` — runtime rollback toggle (FM-6) |
| C-6 `clock.push_hz` | 30 | position-update rate over the control channel |
| C-7 `clock.drift_threshold_ms` | 150 | max tolerated drift before FM-5 snap-to-done |
| C-8 `blink.interval_ms` | 4000 | mean procedural blink interval during idle |
| C-9 `blink.duration_ms` | 120 | procedural blink duration |
| C-10 `idle.sway_amplitude_deg` | 2.0 | head sway amplitude during idle |
| C-11 `speech.sway_amplitude_deg` | 1.5 | head sway amplitude layered during speech |
| C-12 `emotion.suppress` | true | suppress A2F auto-emotion for clean lip sync (Phase 0 gate (d)) |

## Test plan

### Unit (Rust, `cargo test`)
- `playback.rs`: position tracking returns correct ms at known sample offsets.
- `webrtc_client.rs`: blendshape track reassembly (header + chunks → frames).
- `pipeline.rs`: bridge call returns a track with the right frame count.
- **Failure-path tests:** bridge timeout → `blendshapes = None`; empty track
  → `None`; short weight array → padded with 0.0.

### Integration (server, H100)
- Bridge smoke test: Kokoro WAV → plausible 52-float track.
- Sample-rate check: confirm/resample 24 kHz → A2F's expected rate.
- Name mapping: A2F output names == VRM clip names (dump both, diff).
- **Timeout test:** kill the NIM, confirm the bridge call returns within
  `bridge.timeout_ms` and the pipeline still ships audio.

### Live (end-to-end, on the Mac)
1. **Lip sync** — speak a known sentence; mouth opens/closes on the words.
2. **Blink** — natural blink during idle; no double-blink during speech.
3. **Idle** — head/eye motion between utterances; reads alive, not frozen.
4. **Clock drift** — long utterance (10s+); mouth stays on the words to the end.
5. **Barge-in** — interrupt mid-speech; face eases back to idle cleanly.
6. **Rollback** — toggle `avatar.mode` robot ↔ VRM at runtime; both work.
7. **Degradation** — kill the NIM mid-session; audio continues, avatar falls
   back to procedural idle, no crash.

## Open decisions (need Lark's ruling before/at implementation)

1. **Bridge language** — Python (fastest, `nvidia_ace` wheel is Python) vs
   Rust (matches the server, but no official Rust client). Recommend Python
   sidecar for speed; revisit if we want it in-process later.
2. **Emotion** — suppress A2F auto-emotion for clean lip sync, or let it
   through? (Phase 0 gate (d); config C-12.)
3. **Track format** — JSON vs msgpack vs raw float array. Recommend msgpack
   (compact, fast) or raw floats (simplest); JSON is fine for the demo.

## Risks (ranked)

1. **NIM pull/license** — gated, not a code problem. Could stall everything.
2. **Batch mode** — if A2F only behaves correctly streamed, Option A needs a
   "feed full buffer, drain full track" shim.
3. **Clock sync** — the hardest engineering; if it drifts, the demo fails its
   core criterion. (Mitigated by FM-5 drift detection + snap.)
4. **"Alive" quality** — procedural idle/blink is hand-tuned and is exactly
   what looks robotic if done poorly. Highest *quality* risk, separate from
   technical risk. (Mitigated by config C-8..C-11 tunables.)


## Future work

**Real-time configuration sliders (client settings panel).** Once the simple
version is working and we've confirmed the avatar behaves, add a settings
section on the client that exposes the client-side tunables (C-5..C-12) as
live sliders — blink interval/duration, idle and speech sway amplitude, clock
drift threshold, and the robot ↔ VRM mode toggle. This lets us visually tune
the avatar to optimal "alive" settings in real time instead of editing config
and rebuilding. Deferred until after the baseline works; the config keys
above are the seam this will bind to.
