# 01 — TTS Engine Research (Piper replacement)

## Problem

Piper (rhasspy/piper) was the planned TTS engine. It is **archived** as of
2026-10-06 (read-only). We need a replacement that is:

1. **Local** — no cloud API, runs on our own hardware.
2. **Real-time** — low latency, ideally streaming (synthesize as we speak).
3. **High quality** — natural, expressive, not robotic.
4. **Licensed for our use** — permissive (Apache/MIT) preferred; non-commercial
   licenses (e.g. Coqui CPML) are a constraint to flag.
5. **Small footprint** — runs on a laptop-class GPU or CPU.

## Landscape (surveyed 2026-09-01)

| Engine | Params | License | Quality | Latency | Notes |
|--------|--------|---------|---------|---------|-------|
| **Kokoro-82M** | 82M | Apache-2.0 (weights) | High | Very low (CPU) | **Leading candidate** |
| XTTS-v2 (Coqui) | ~500M | CPML (non-commercial) | High | Medium | Coqui shut down; voice cloning |
| StyleTTS2 | ~100M | MIT | High | Medium | Heavier pipeline |
| Chatterbox (Resemble) | ~300M | MIT | High | Medium | Newer, good |
| F5-TTS | ~300M | MIT | High | Medium | Flow-matching |
| Orpheus | ~3B | Apache-2.0 | Very high | High (GPU) | Too heavy for laptop |
| MeloTTS | ~100M | MIT | Medium | Low | Decent fallback |
| espeak-ng | — | GPL | Low (robotic) | Instant | Emergency fallback only |

## Recommendation: Kokoro-82M

**Why it wins:**

- **82M parameters** — tiny by modern standards, runs comfortably on CPU, and
  trivially on any GPU. Real-time factor well under 1.0 (faster than realtime).
- **Apache-2.0 weights** — no commercial restriction, no copyleft. Clean.
- **Quality** — comparable to much larger models; natural prosody.
- **Multi-language** — en-US, en-GB, es, fr, hi, it, ja, pt-br, zh (via `misaki` G2P).
- **Voice cloning** — load a voice tensor to clone a target voice (useful for
  giving Skye a *specific* voice rather than a stock one).
- **24 kHz output** — fine for speech; upsample if needed.

**Caveats:**

- It is a **PyTorch** model (Python `kokoro` package). Integration into a Rust
  binary is the real engineering question (see below).
- G2P depends on `espeak-ng` for English out-of-vocabulary fallback and some
  languages — a system dependency to install.

## Where TTS runs: server-side

With the client/server split (04), TTS runs **on the server**:

- **Skye's voice lives with Skye** — the server is where she is, and her voice
  is generated there.
- **One voice, many clients** — if we ever go multi-user, generating the voice
  once on the server and broadcasting is the right shape.
- **The server has the compute** — Kokoro is CPU-fast, but the server is the
  natural home for the model.

The client receives the synthesized audio over WebRTC and plays it. The added
network latency (tens of ms) is negligible next to synthesis time.

## Integration into Rust (the actual decision)

Kokoro is PyTorch-native. Three paths to get it into a Rust project:

1. **ONNX export + `ort` (Rust ONNX Runtime).**
   Kokoro has community ONNX exports. `ort` is a mature Rust binding to ONNX
   Runtime. Pros: single Rust binary, no Python. Cons: ONNX export may lag the
   PyTorch release; need to manage the export ourselves.

2. **`candle` (HuggingFace's Rust ML framework).**
   Kokoro has been ported to candle. Pros: pure Rust, no runtime dependency,
   compiles to a single binary. Cons: candle is younger than ONNX Runtime;
   port quality varies.

3. **Python sidecar service.**
   Run `kokoro` in a small Python process (FastAPI/gRPC), Rust calls it over
   localhost. Pros: always tracks upstream, simplest to get working. Cons: adds
   a Python runtime to the deployment; IPC latency (negligible on localhost).

**Recommendation:** start with **path 3 (sidecar)** to unblock voice immediately,
then migrate to **path 1 (ONNX/ort)** once the avatar is working end-to-end.
Path 2 (candle) is attractive for a pure-Rust single binary but is the riskiest
of the three; revisit after v1.

## Streaming / latency

For a real-time avatar, we want to synthesize sentence-by-sentence (or
chunk-by-chunk) and stream audio as it's produced, rather than waiting for a
full paragraph. Kokoro's `KPipeline` is a generator — it yields audio per
grapheme group, so it already supports incremental synthesis. The TTS service
should expose a streaming interface (chunked audio) that the WebRTC audio track
can consume.

## Decisions made (2026-09-01)

- **Voice** — stock Kokoro voice for v1. No voice cloning (would need a clean
  reference sample; defer to later).
- **Location** — server-side (Skye's voice lives with Skye).

*Sources: github.com/hexgrad/kokoro (README, fetched 2026-09-01);
github.com/rhasspy/piper (archived banner, fetched 2026-09-01).*
