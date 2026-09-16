# Audio2Face-3D — Rust gRPC Bridge (Phase 1)

Status: **Complete** (bridge built, verified end-to-end against the live NIM)
Date: 2026-09-16

---

## 1. Smoke test result (verified live)

The self-hosted NIM on `localhost:52000` speaks `A2FControllerService.ProcessAudioStream` (bidirectional streaming gRPC). Confirmed with a Python smoke test using generated protos.

- **Service**: `nvidia_ace.services.a2f_controller.v1.A2FControllerService`
- **Method**: `ProcessAudioStream(stream AudioStream) returns (stream AnimationDataStream)`
- **Endpoint**: `localhost:52000` (insecure channel, NO auth metadata — self-hosted)
- **Result**: 1s of 220Hz sine → 31 animation frames (~30fps), status SUCCESS (code 0)

## 2. Blendshape count: 55 (not 52)

The claire_v2.3 model outputs **55** blendshapes:
- Indices 0–51: the 52 standard ARKit blendshapes
- Indices 52–54: **HeadRoll, HeadPitch, HeadYaw** (head pose, exposed as 3 extra "blendshapes")

Full ordered list (index: name):

```
 0 EyeBlinkLeft      1 EyeLookDownLeft   2 EyeLookInLeft    3 EyeLookOutLeft
 4 EyeLookUpLeft     5 EyeSquintLeft     6 EyeWideLeft      7 EyeBlinkRight
 8 EyeLookDownRight  9 EyeLookInRight   10 EyeLookOutRight 11 EyeLookUpRight
12 EyeSquintRight   13 EyeWideRight     14 JawForward      15 JawLeft
16 JawRight         17 JawOpen          18 MouthClose      19 MouthFunnel
20 MouthPucker      21 MouthLeft        22 MouthRight      23 MouthSmileLeft
24 MouthSmileRight  25 MouthFrownLeft   26 MouthFrownRight 27 MouthDimpleLeft
28 MouthDimpleRight 29 MouthStretchLeft 30 MouthStretchRight 31 MouthRollLower
32 MouthRollUpper   33 MouthShrugLower  34 MouthShrugUpper 35 MouthPressLeft
36 MouthPressRight  37 MouthLowerDownLeft 38 MouthLowerDownRight 39 MouthUpperUpLeft
40 MouthUpperUpRight 41 BrowDownLeft    42 BrowDownRight   43 BrowInnerUp
44 BrowOuterUpLeft  45 BrowOuterUpRight 46 CheekPuff       47 CheekSquintLeft
48 CheekSquintRight 49 NoseSneerLeft    50 NoseSneerRight  51 TongueOut
52 HeadRoll         53 HeadPitch        54 HeadYaw
```

This is a correction to the earlier design docs (which assumed 52). It is good news: the three.js rig gets head motion for free, no separate head-pose channel needed.

## 3. Wire format (from proto files, `NVIDIA/Audio2Face-3D-Samples`)

### Input — `nvidia_ace.controller.v1.AudioStream` (oneof)

1. `audio_stream_header` (first) — `AudioStreamHeader`:
   - `audio_header` (PCM-16, mono, sample rate)
   - `face_params` (map<string,float>: upperFaceStrength, lowerFaceStrength, etc.)
   - `emotion_post_processing_params` (contrast, live_blend_coef, emotion_strength, max_emotions)
   - `blendshape_params` (multipliers + offsets per blendshape)
   - `emotion_params` (live_transition_time, beginning_emotion)
2. `audio_with_emotion` (repeated) — `AudioWithEmotion` (audio_buffer bytes + optional emotions)
3. `end_of_audio` (last) — `EndOfAudio` marker

### Output — `nvidia_ace.controller.v1.AnimationDataStream` (oneof)

1. `animation_data_stream_header` (first) — carries `skel_animation_header.blend_shapes` (55 names, once)
2. `animation_data` (repeated) — `SkelAnimation.blend_shape_weights` = (time_code + 55 floats)
3. `event` (optional)
4. `status` (last) — code 0 = SUCCESS

## 4. Proto generation procedure (Python, for smoke tests)

```bash
# protoc binary is in the container at /opt/proto/bin/protoc (libprotoc 3.21.12)
# proto files: NVIDIA/Audio2Face-3D-Samples repo, proto/protobuf_files/
# google well-known types: container /opt/proto/include/google

pip install grpcio-tools   # provides python -m grpc_tools.protoc (version-matched)

python3 -m grpc_tools.protoc -I. --python_out=. --grpc_python_out=. \
  nvidia_ace.services.a2f_controller.v1.proto \
  nvidia_ace.controller.v1.proto nvidia_ace.a2f.v1.proto \
  nvidia_ace.animation_data.v1.proto nvidia_ace.animation_id.v1.proto \
  nvidia_ace.audio.v1.proto nvidia_ace.status.v1.proto \
  nvidia_ace.emotion_with_timecode.v1.proto nvidia_ace.emotion_aggregate.v1.proto
```

## 5. Bridge design (Rust)

New crate `a2f-bridge/` in the prosopon repo. Standalone for Phase 1 (testable in isolation), integrated into the server pipeline later.

- tonic + prost for gRPC (tonic-build compiles the protos at build time)
- tokio for async
- serde_json for frame output
- Phase 1: CLI that streams a WAV → A2F → emits JSON frames
- Phase 2: WebSocket server to forward frames to the Tauri webview
- Phase 3: hook into the TTS pipeline (stream audio as it's produced)

## 6. Key facts for the bridge

- Audio: PCM-16, mono, WAV container. Sample rate read from WAV (Kokoro outputs 24000 Hz).
- Chunking: arbitrary; sample client sends 1s per packet. We'll stream smaller chunks for latency.
- The header's `face_params` / `blendshape_params` are the tunables the future settings sliders will bind to.
- Head pose (HeadRoll/Pitch/Yaw) is in the blendshape list — the three.js rig can use these for head motion.

## 7. Phase 1 result — Rust bridge built and verified

The bridge crate `a2f-bridge/` is built and working end-to-end against the running NIM.

### What was built

- `a2f-bridge/Cargo.toml` — tonic 0.12, prost 0.13, prost-types, tokio, tokio-stream, serde_json, hound
- `a2f-bridge/build.rs` — tonic-build compiles the service proto (client only)
- `a2f-bridge/src/lib.rs` — module tree exposing `nvidia_ace::...` generated types
- `a2f-bridge/src/main.rs` — CLI: `<input.wav> [endpoint]` → streams audio → emits JSON frames
- `a2f-bridge/proto/` — 16 nvidia_ace protos + 11 google well-known types

### Build notes (gotchas)

- protoc must be on PATH (copied to `~/.cargo/bin/protoc`, libprotoc 3.21.12)
- tonic generates the client as `A2fControllerServiceClient` (lowercase 'f' in A2f)
- `prost-types` required for `google/protobuf/any.proto` (the `Event.metadata` field)
- `emotion_aggregate.v1` is NOT in the service proto's dependency chain — not generated, not needed

### Verified results

- 16000 Hz WAV (1s sine): 31 frames, status SUCCESS
- 24000 Hz WAV (2s speech-like, Kokoro's rate): 61 frames, status SUCCESS
- Output: 1 header line (55 blendshape names) + N frame lines `{"t":..., "v":[55 floats]}`

### Output format (for the webview)

```
{"header":{"blendShapes":["EyeBlinkLeft",...55 names]}}
{"t":0.0,"v":[0.00014,0.0,...55 floats]}
{"t":0.0333,"v":[...]}
...
```

~30fps, one JSON line per frame.

## 8. Next (Phase 2)

- WebSocket server in the bridge to forward frames to the Tauri webview
- Hook into the TTS pipeline (stream Kokoro output as it's produced, not batch WAV)
- The three.js face rig consumes the 55 blendshapes (52 ARKit + HeadRoll/Pitch/Yaw)
