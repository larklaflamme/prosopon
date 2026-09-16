# Prosopon — Audio2Face Integration Research

Date: 2026-09-15
Status: RESEARCH PASS COMPLETE (initial)

## The headline finding

The old "Audio2Face app" (Omniverse Kit GUI) is **dead**. The NIM endpoint is
deprecated, and the `NVIDIA-Omniverse/audio2face` repo is 404. The current path
is **Audio2Face-3D**, part of NVIDIA ACE, with two deployment options.

## Hardware (verified live this turn)

- Server has an **NVIDIA H100 PCIe** — 80GB VRAM, CUDA 13.2, driver 595.58.03.
- `nvidia-smi` confirms: 81.5GB total, ~8GB in use (ollama llama-server + a python process).
- Docker 29.4.0 present.
- This GPU is *massively* over-provisioned for Audio2Face (needs 4GB+ VRAM).

## The two current paths

### Path A — Audio2Face-3D NIM (Microservice) — RECOMMENDED
- Docker container from NGC catalog (`nim/nvidia/audio2face-3d`).
- Converts speech → **ARKit blendshapes** (52 standard blendshapes) + emotion.
- gRPC interface (proto/ folder in the Samples repo).
- Repo: `NVIDIA/Audio2Face-3D-Samples` (Apache 2 license for the repo).
- NIM itself: NVIDIA Software License Agreement (gated, needs NGC API key).
- Helm chart + docker-compose quick-start provided.
- **Sidesteps the CUDA version issue** — container bundles its own CUDA.

### Path B — Audio2Face-3D SDK (Audio2X SDK) — embed from source
- Repo: `NVIDIA/Audio2Face-3D-SDK` (MIT license).
- C++/CUDA/TensorRT. Faster than 60 FPS, multi-track, batch + interactive.
- Build: CMake + CUDA + TensorRT. Linux (Ubuntu 20.04+) supported.
- Requirements: CUDA 12.8–13.0 (server has 13.2 — slightly above, may need care),
  TensorRT 10.13–11.0, 4GB+ GPU memory.
- Output: `libaudio2x.so` shared library (combines A2E + A2F + common).

## Models

- **Audio2Face-3D**: regression (2.3) and diffusion (3.0). ONNX-TRT format.
  NVIDIA Open Model License.
- **Audio2Emotion-3D**: 2.2 (production) / 3.0 (experimental). Gated on Hugging Face
  (license click-through + HF token). Custom license.
- **Audio2Face-3D Training Framework**: Apache license, train your own.

## The integration surface (proposed architecture)

1. Prosopon server already does TTS → audio (Kokoro).
2. Feed that audio → Audio2Face-3D (NIM container on the H100) → ARKit blendshapes.
3. Blendshapes are **tiny**: 52 floats/frame @ 60fps ≈ ~12KB/s. Trivial bandwidth.
4. Stream blendshapes to the Tauri client (WebSocket or WebRTC data channel).
5. Client renders a face driven by the blendshape weights (three.js/WebGL in the
   Tauri webview, or a 2D avatar rig).

## The real build work (not A2F itself)

A2F is the easy part (H100 makes compute trivial). The actual work is:
1. **A face to render** — a 3D rig (three.js + a blendshape-compatible mesh) or a
   2D avatar. This is the bulk of the "demo" effort.
2. **The bridge** — audio → A2F → blendshapes → client. A thin gRPC/WebSocket layer.
3. **AEC** (Lane 1 step 2) — still the prerequisite so the mic doesn't hear itself.

## Caveats / open questions

- CUDA 13.2 vs SDK's 12.8–13.0 requirement → prefer the NIM (Docker) path.
- NIM is gated (NGC API key + NVIDIA Software License). Audio2Emotion gated on HF.
- UE5 plugin exists (Audio2Face-3D plugin) but that's a heavier path than we need.
- "Build by end of week" is realistic IF we scope the face to a simple rig and
  use the NIM path. The face itself is the variable.

## Sources (fetched this turn)

- nvidia-smi (live) — H100 confirmed.
- https://www.nvidia.com/en-us/omniverse/apps/audio2face/ — deprecated NIM.
- https://github.com/NVIDIA-Omniverse — new Omniverse libraries (ovrtx, ovphysx, ovstream...).
- https://docs.nvidia.com/ace/... — NVIDIA ACE for Games (Audio2Face-3D SDK/models).
- https://github.com/NVIDIA/Audio2Face-3D-SDK — Audio2X SDK README (build/requirements).
- https://github.com/NVIDIA/Audio2Face-3D-Samples — NIM microservice README (gRPC, ARKit blendshapes).
