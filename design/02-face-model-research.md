# 02 — Face Model + ARKit Blendshapes Research

## Problem

Give Skye a face that moves. The face must be:

1. **Driven by ARKit-standard blendshapes** — the de-facto industry standard
   for facial expression (52 named shapes: jawOpen, eyeBlinkLeft, browInnerUp,
   mouthSmileLeft, etc.). This is the interchange format every tool speaks.
2. **Tracked from a webcam** — no iPhone TrueDepth dependency for v1; a plain
   webcam + landmark detection must be enough.
3. **Renderable in real time** — 30+ fps, low latency.

## The two standards that matter

### ARKit blendshapes (52 shapes)

Apple's ARKit defines 52 facial blendshapes (weights 0–1). They are the lingua
franca: face-capture tools output them, avatar runtimes consume them. We do not
need to invent a format — we adopt ARKit's 52-shape set as our internal
expression vector.

### VRM 1.0 (avatar format)

VRM is the open standard for 3D humanoid avatars (glTF extension). VRM 1.0
defines a **preset expression system** that maps cleanly onto ARKit:

- **Emotions**: happy, angry, sad, relaxed, surprised.
- **Lip-sync procedurals**: aa, ih, ou, ee, oh (viseme-like mouth shapes).
- **Blink**: blink, blinkLeft, blinkRight.
- **Gaze**: lookUp, lookDown, lookLeft, lookRight.

Each expression is a weighted group of morph targets (blendshapes) + material
color + texture transform. The runtime drives expression weights 0–1, and the
model deforms accordingly.

**The mapping** ARKit-blendshape → VRM-expression is well-trodden (three-vrm and
other runtimes ship it). We adopt VRM 1.0 as the avatar format and translate
ARKit weights into VRM expression weights.

## Face tracking (webcam → blendshapes)

The pipeline: webcam frame → face landmark detection → blendshape estimation.

**MediaPipe Face Landmarker** (Google) is the standard choice:
- 478 3D landmarks + optional **blendshape output** (it can emit ARKit-style
  blendshapes directly).
- Runs in-browser (JS/WASM) or on-device (Python/C++).
- Real-time on CPU.

Alternatives: MediaPipe Face Mesh (older, 468 landmarks), OpenCV + dlib
(weaker), or a dedicated landmark→blendshape regressor.

**Recommendation:** MediaPipe Face Landmarker, using its native blendshape
output where possible, with a small landmark→blendshape mapping layer as
fallback.

## Rendering (the fork — see 04)

Two ways to render the VRM face:

- **Browser**: three.js + `@pixiv/three-vrm` (mature, battle-tested, trivial to
  deploy). The browser does tracking + rendering locally; Rust streams audio +
  lip-sync timing.
- **Rust**: `bevy` + `bevy_vrm` (younger, less complete). Single self-contained
  binary; heavier to build and iterate.

This is the central architecture decision and is treated in 04-architecture.md.

## Lip sync

Two sources of mouth motion:

1. **Viseme timing from TTS** — Kokoro (or the G2P layer) can emit phoneme
   timings; map phonemes → VRM lip-sync procedurals (aa/ih/ou/ee/oh). This is
   the *correct* approach (mouth matches the actual speech).
2. **Audio-driven** — analyze the audio stream for amplitude/spectral shape and
   drive mouth open/close. Simpler, but less accurate.

**Recommendation:** phoneme-timed visemes from the TTS layer (path 1), with
audio-driven as a fallback. This requires the TTS service to expose phoneme
timing, not just audio — a design constraint to carry into 01.

## Blink + gaze (procedural)

VRM 1.0 supports procedural blink and gaze. These should be driven by a small
idle-behavior layer (random blinks, occasional gaze shifts) so the face feels
alive even when not speaking. This is a "presence" concern, not a tracking
concern — worth a dedicated module later.

## Open questions for Lark

- Do we have a **specific VRM model** in mind (a VRoid avatar, a custom model,
  a purchased asset), or do we start with a free/placeholder VRM?
- Is **webcam tracking** the v1 target, or do we want iPhone TrueDepth (ARKit
  native) support from the start? Webcam is the pragmatic v1; TrueDepth is
  higher fidelity but requires an iOS capture app.

*Sources: vrm-c/vrm-specification (VRMC_vrm-1.0 expressions, fetched
2026-09-01). ARKit 52-blendshape set and MediaPipe Face Landmarker details are
from training, not re-verified this session.*
