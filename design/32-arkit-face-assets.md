# Prosopon — ARKit-compatible three.js face assets (research)

Date: 2026-09-15
Goal: find a ready-made ARKit-compatible head/avatar for the A2F demo.
Constraint: female avatar (matches af_bella TTS) or male (am_xxx).

## The key insight: VRM is the format

VRM (VRoid) avatars natively use ARKit blendshapes. VRM 1.0 defines the
52 ARKit blendshapes as a standard. VRoid Studio (free) exports VRM.
`@pixiv/three-vrm` renders VRM in three.js — works in a Tauri webview.

## Verified assets (fetched live this turn)

### 1. @pixiv/three-vrm — the renderer
- GitHub: https://github.com/pixiv/three-vrm (2.2k stars, MIT license)
- npm: `@pixiv/three-vrm` (v3)
- Loads VRM via GLTFLoader + VRMLoaderPlugin.
- CDN importmap works (jsDelivr) — no build step needed for a quick demo.
- Supports WebGPURenderer (v3+) and MToon materials.

### 2. hinzka/52blendshapes-for-VRoid-face — THE ready-made heads
- GitHub: https://github.com/hinzka/52blendshapes-for-VRoid-face
- Contains BOTH:
  - `VRoid_V110_Female_v1.1.3.vrm` (23.3 MB)  ← matches af_bella
  - `VRoid_V110_Male_v1.1.3.vrm`   (22.7 MB)  ← matches am_xxx
- Each has all 52 ARKit blendshapes + auxiliary shapes, "Perfect Sync" spec.
- Author explicitly permits free use as source data.
- License note: VRoid Studio output — check VRoid terms for commercial use
  (VRoid allows commercial use of avatars created in Studio, but verify).

### 3. suchipi/arkit-face-blendshapes — reference
- https://arkit-face-blendshapes.com/ — visual reference of each blendshape.
- Useful for understanding what each of the 52 does.

### 4. mind-ar-js three.js face blendshapes example
- https://hiukim.github.io/mind-ar-js-doc/more-examples/threejs-face-blendshapes/

## The integration mapping (the real work)

A2F outputs 52 ARKit-named blendshapes (jawOpen, mouthSmileLeft, etc.).
VRM avatars expose blendshape clips. Two options:
  A. Use hinzka's 52-blendshape VRM → clips are ARKit-named → direct 1:1 map.
  B. Use a standard VRM 1.0 → map A2F's 52 → VRM expression presets
     (happy/angry/sad/relaxed/surprised + visemes aa/ih/ou/ee/oh + blink).

Option A is the demo path: direct name match, no mapping table needed.

## Demo architecture (pragmatic path)

Kokoro TTS (af_bella) → audio → Audio2Face-3D NIM (H100)
  → 52 ARKit blendshape floats → WebSocket → Tauri webview
  → @pixiv/three-vrm drives VRoid_V110_Female.vrm

## Open items
- Confirm VRoid Studio commercial-use terms for the hinzka model.
- Confirm self-hosted A2F NIM exposes ProcessAudioStream on localhost.
- AEC still the prerequisite (Lane 1 step 2).
