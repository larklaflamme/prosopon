# Prosopon × Audio2Face-3D — Integration Surface (pragmatic path)

Date: 2026-09-15
Status: research note (pre-implementation)

## Goal
Audio2Face-3D NIM on the H100 → ARKit blendshapes → lightweight three.js face in the Tauri webview.

## Verified facts (fetched live this turn)

### The service
- gRPC service: `A2FControllerServiceStub.ProcessAudioStream()` — a **bidirectional streaming** RPC.
- Proto package: `nvidia_ace.a2f.v1` (main), `nvidia_ace.animation_data.v1` (output), plus `audio`, `emotion_with_timecode`, `animation_id`, `status`, `controller`.
- Source: `NVIDIA/Audio2Face-3D-Samples` repo, `proto/protobuf_files/`.

### Input (what we send)
`AudioStream` = oneof:
1. `AudioStreamHeader` (first message) — carries:
   - `animation_ids` (stream IDs)
   - `audio_header` (PCM-16, single channel, sample rate)
   - `face_params` (smoothing, strengths, blink, lip offsets, tongue)
   - `emotion_post_processing_params` (contrast, live_blend_coef, preferred_emotion_strength, emotion_strength, max_emotions)
   - `blendshape_params` (per-blendshape multipliers/offsets, clamping)
   - `emotion_params` (live_transition_time, beginning_emotion)
2. `AudioWithEmotion` (repeated) — `audio_buffer` (bytes) + optional `emotions` (timecoded).

Audio format: **PCM-16, single channel, WAV container** (per the sample client).

### Output (what we receive)
`AnimationDataStream` = oneof:
1. `AnimationDataStreamHeader` — carries `SkelAnimationHeader` with `blend_shapes` (the 52 names, sent ONCE).
2. `AnimationData` — carries `SkelAnimation` with `blend_shape_weights` = `FloatArrayWithTimeCode` (time_code + 52 floats).

So the wire format is: **header (names once) → stream of (timecode, 52 floats)**. Tiny — ~52 floats/frame.

### The 52 ARKit blendshapes (from the proto comment)
EyeBlinkLeft/Right, EyeLookDown/In/Out/Up Left/Right, EyeSquintLeft/Right, EyeWideLeft/Right,
JawForward, JawLeft, JawRight, JawOpen, MouthClose, MouthFunnel, MouthPucker, MouthLeft, MouthRight,
MouthSmileLeft/Right, MouthFrownLeft/Right, MouthDimpleLeft/Right, MouthStretchLeft/Right,
MouthRollLower/Upper, MouthShrugLower/Upper, MouthPressLeft/Right, MouthLowerDownLeft/Right,
MouthUpperUpLeft/Right, BrowDownLeft/Right, BrowInnerUp, BrowOuterUpLeft/Right, CheekPuff,
CheekSquintLeft/Right, NoseSneerLeft/Right, TongueOut.

### Two deployment modes
1. **Cloud NVCF** (what the sample client uses): gRPC channel to `grpc.nvcf.nvidia.com:443`, metadata = `function-id` + `authorization: Bearer <NGC API key>`. No GPU needed locally.
2. **Self-hosted NGC container** (what we'd do on the H100): pull `audio2face-3d` NIM from NGC Catalog, run via docker compose (`quick-start/` folder has compose files). Bundles its own CUDA — sidesteps the 13.2 vs 12.8 mismatch.

## Integration architecture (proposed)

```
Prosopon TTS (Kokoro) → WAV (PCM-16 mono)
        ↓
A2F-3D NIM (H100, self-hosted) → gRPC ProcessAudioStream
        ↓
AnimationDataStream → (timecode, 52 blendshape floats)
        ↓
thin bridge (Rust or Python) → WebSocket/JSON to Tauri webview
        ↓
three.js face rig → morph targets driven by blendshape weights
```

## The real work (ranked by effort)
1. **The face** — a three.js mesh with 52 morph targets (or a subset). This is the bulk of the demo effort. Options: (a) a ready-made ARKit-compatible head (e.g. a CC0/glTF head with ARKit morph targets), (b) a stylized 2D-ish face driven by a subset of the 52.
2. **The bridge** — A2F gRPC output → WebSocket → webview. Thin. Could be a small Rust service or a Python sidecar.
3. **AEC** — still the prerequisite (echo cancellation so the mic doesn't hear the face's own voice).

## Open questions / next research
- Does the self-hosted NIM expose the same `ProcessAudioStream` gRPC surface, or a different local endpoint? (The sample client targets NVCF cloud; the self-hosted container likely exposes the same proto on localhost.)
- Licensing: NGC API key + NVIDIA Software License + Audio2Emotion model license (Hugging Face click-through).
- A ready-made ARKit-compatible three.js head (glTF with 52 morph targets) — need to find a CC0/usable asset.
- Latency: streaming (not batch) mode — the proto supports streaming, so we can drive it live as TTS produces audio.

## Sources (fetched this turn)
- `NVIDIA/Audio2Face-3D-Samples` README (raw + browser)
- `proto/protobuf_files/nvidia_ace.a2f.v1.proto`
- `proto/protobuf_files/nvidia_ace.animation_data.v1.proto`
- `scripts/audio2face_3d_api_client/nim_a2f_3d_client.py`
- GitHub API directory listings (proto/, scripts/)
