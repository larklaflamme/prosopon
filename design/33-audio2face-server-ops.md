# Audio2Face-3D NIM — Server-Side Ops (Phase 0)

Status: **Complete** (container pulled, stood up, verified)
Server: `g3-h100-small-dal-1` (H100 PCIe, device `2330:10de`, 81.5GB VRAM)
Date: 2026-09-16

---

## 1. Prerequisites (verified)

- **GPU**: H100 PCIe present, healthy (38°C, ~70GB free VRAM).
- **Docker**: 29.4.0, working.
- **NGC API key**: `NVIDIA_API_KEY` in `/home/ubuntu/prosopon/.env` (70 chars, `nvap...`).
- **Container image**: `nvcr.io/nim/nvidia/audio2face-3d:1.3` (58.5GB, digest `sha256:b57b3c2c...`).

## 2. Docker login to NGC

```bash
KEY=$(grep -E '^NVIDIA_API_KEY=' /home/ubuntu/prosopon/.env | cut -d= -f2-)
echo "$KEY" | docker login nvcr.io --username '$oauthtoken' --password-stdin
```

- Username is the literal string `$oauthtoken` (single-quoted so the shell doesn't expand it).
- Credentials land in `~/.docker/config.json` (unencrypted — noted, acceptable for now).

## 3. Pull the image

```bash
docker pull nvcr.io/nim/nvidia/audio2face-3d:1.3
```

- 58.5GB. Requires the NGC login from step 2 AND acceptance of the NVIDIA Software License (tied to the NGC account — already accepted, since the pull succeeded).

## 4. Container anatomy (learned by inspection)

- **Entrypoint**: `/bin/bash -c $SERVER_START_SCRIPT_PATH`, where `SERVER_START_SCRIPT_PATH=/opt/nim/start_server.sh`.
- **start_server.sh** calls the NIM built-in `start_server` (from `nimlib`, not a readable shell function).
- **Model manifest**: `/opt/nim/etc/default/model_manifest.yaml`. Profiles for A10G, A100, A30, GB20x, **H100**, L4, L40S, RTX4090, RTX6000, h100-nvl, h100-pcie.
  - Our GPU matches **`gpu: H100`, `gpu_device: 2330:10de`** (H100 PCIe).
  - H100 profile models: `a2e_v1.0_h100_fp32_bs8_v4`, and `claire/james/mark_v2.3_h100_fp16_bs8_v3`.
- **Models download at runtime** from NGC (`ngc://` URIs in the manifest) — so the container needs `NGC_API_KEY` passed in as an env var.
- **Streaming endpoint**: bidirectional, port **52000** (from `/apps/configs/deployment_config.yaml`, `use_bidirectional: true`).
- **NIM HTTP API**: standard NIM health/API port (default 8000).
- **Config files** (mounted/overridable):
  - `/apps/configs/deployment_config.yaml` — streams, logging, endpoints.
  - `/apps/configs/a2e_config.yaml` — audio→emotion params (device_id, emotion strength, samplerate 16000).
  - `/apps/configs/stylization_config.yaml`, `/apps/configs/advanced_config.yaml`.
- **Key env vars** (from `inference.py`): `NIM_DISABLE_MODEL_DOWNLOAD`, `NIM_MANIFEST_PROFILE`, `NGC_API_KEY`.

## 5. Stand up the container

```bash
KEY=$(grep -E '^NVIDIA_API_KEY=' /home/ubuntu/prosopon/.env | cut -d= -f2-)
docker run -d --name audio2face \
  --gpus all \
  -e NGC_API_KEY="$KEY" \
  -p 8002:8000 \
  -p 52000:52000 \
  nvcr.io/nim/nvidia/audio2face-3d:1.3
```

- `--gpus all` exposes the H100.
- `NGC_API_KEY` lets the runtime download the H100 TensorRT models on first boot.
- **Port correction**: host 8000 was already taken by `skye-chroma`, so the NIM HTTP API is mapped **host 8002 → container 8000**. Streaming is on **52000** (free).

## 6. Verify

```bash
docker logs -f audio2face                       # watch model download + server start
curl -s http://localhost:8002/v1/health/ready   # {"status":"ready"}
curl -s http://localhost:8002/v1/health/live    # {"status":"live"}
```

- First boot downloads the H100 `.trt` models — expect several minutes before ready.

## 7. Metadata / manifest verification

The NIM HTTP API exposes two useful endpoints for confirming exactly what we're running:

### `/v1/metadata`

```bash
curl -s http://localhost:8002/v1/metadata
```

Returns (verified live):

- **version**: `1.3.16-rc4`
- **licenseInfo**: NVIDIA Software License Agreement + AI Foundation Models Community License.
- **modelInfo** (4 models):
  - `audio2emotion_model:a2e_v1.0_h100_fp32_bs8_v4`
  - `audio2face_3d_model:claire_v2.3_h100_fp16_bs8_v3`
  - `audio2face_3d_model:james_v2.3_h100_fp16_bs8_v3`
  - `audio2face_3d_model:mark_v2.3_h100_fp16_bs8_v3`

### `/v1/manifest`

```bash
curl -s http://localhost:8002/v1/manifest
```

Returns the full `model_manifest.yaml` (schema 2.0): `profile_selection_criteria: auto`, and a list of profiles keyed by GPU tag (`A10G`, `A100`, `H100`, `L4`, `L40S`, `RTX4090`, …). Each profile lists its TensorRT engine files (`a2e.trt`, `claire_v2.3.trt`, `james_v2.3.trt`, `mark_v2.3.trt`) with `ngc://` URIs and blake3 checksums.

## 8. Stand-up result (2026-09-16) — SUCCESS

- Container `audio2face` running, healthy.
- **Profile selected**: `h100-pcie` / `gpu_device: 2331` (auto-selected). Our GPU is device 2330 (H100 PCIe) — a minor tag mismatch, but both profiles use identical `h100_fp16` TensorRT engines (same SM90 arch), so it runs correctly. Not a blocker.
- **Models downloaded at runtime** (first boot): `a2e.trt`, `claire_v2.3.trt`, `mark_v2.3.trt`, `james_v2.3.trt` — all H100 fp16.
- **Health**: `/v1/health/ready` → `{"status":"ready"}`, `/v1/health/live` → `{"status":"live"}`.
- **gRPC server**: `Serving gRPC Server on 0.0.0.0:52000` (bidirectional streaming).
- **A2E + A2F processors**: both initialized (emotion params loaded, blendshape solver loaded).

## 9. Notes / gotchas

- The model download is the long pole on first boot; subsequent boots are fast (models cached in the container's writable layer — consider a named volume for `/tmp/a2x` if we want persistence across `docker rm`).
- `NIM_MANIFEST_PROFILE` can force a profile if auto-selection misbehaves; auto should pick H100 correctly given the device match.
- The gRPC/streaming surface is **52000** (bidirectional), not 50051 — the bridge must target 52000.
- The NIM HTTP API (8002) is for health/metadata only — **not** the animation stream.
