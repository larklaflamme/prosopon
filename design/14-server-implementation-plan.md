# 14 — Server Implementation Plan (Rust WebRTC server)

**Date:** 2026-09-01
**Scope:** the server binary only — receives text over WebRTC, calls Ollama
(cognition) and Kokoro (TTS), returns audio. Client (Moonshine STT, Tauri
shell) is out of scope here.

---

## 0. Verified infra (checked live this session)

Every claim below was confirmed with `curl` against the running services,
not from training.

| Service | Endpoint | Verified |
|---------|----------|----------|
| Kokoro TTS | `POST http://localhost:21802/v1/audio/speech` | HTTP 200, 133 KB WAV, 24 kHz mono 16-bit PCM, 0.16 s |
| Ollama chat | `POST http://localhost:11434/api/chat` | non-streaming works; returns `message.content` |
| Ollama generate | `POST http://localhost:11434/api/generate` | works (fallback) |
| Ollama tags | `GET http://localhost:11434/api/tags` | 26 models present |

### Kokoro request schema (from live OpenAPI)

```json
{
  "model": "kokoro",            // tts-1 | tts-1-hd | kokoro
  "input": "text to speak",
  "voice": "af_heart",          // default af_heart
  "response_format": "wav",     // mp3 | opus | aac | flac | wav | pcm
  "speed": 1.0,                 // 0.25 .. 4.0
  "stream": true,               // default TRUE — streams sentence-by-sentence
  "return_download_link": false,
  "lang_code": null,
  "volume_multiplier": 1.0,
  "normalization_options": null
}
```

### Ollama chat request/response (from live call)

```json
// request
{ "model": "qwen2.5:3b", "think": false, "messages": [{"role":"user","content":"..."}], "stream": false }
// response
{ "message": {"role":"assistant","content":"..."}, "done": true, "done_reason": "stop",
  "total_duration": ..., "eval_count": ... }
```

---

## 1. Key findings that shape the design

Three things I learned from the live checks that change how we build this:

1. **Kokoro streams sentence-by-sentence by default** (`stream: true`).
   This is *exactly* the M0 requirement ("streamed sentence-by-sentence").
   The server should consume the streaming response and forward each
   sentence's audio to the WebRTC track as it arrives — that's what hits the
   sub-second first-audio target.

2. **Kokoro returns Opus directly** (`response_format: "opus"`). Opus is
   WebRTC-native. So the server can request Opus from Kokoro and pipe it
   straight into the WebRTC audio track with **no transcoding**. This is a
   real simplification — no WAV→Opus step in the server.

3. **Ollama `/api/chat` is the right endpoint**, not `/api/generate`. Chat
   carries the conversation history (`messages` array), which is what a
   voice loop needs. `generate` is single-prompt only.

---

## 2. Server crate structure

```
server/
  Cargo.toml
  config.yaml              # tts.voice, cognition.model, webrtc.listen_port
  src/
    main.rs                # entry: load config, start WebRTC server
    config.rs              # config.yaml loader (serde_yaml), defaults
    tts.rs                 # Kokoro HTTP client (streaming)
    cognition.rs           # Ollama HTTP client (chat)
    pipeline.rs            # text → cognition → TTS → audio chunks
    webrtc.rs              # WebRTC server: data channel in, audio track out
  tests/
    live_tts.rs            # integration test vs real Kokoro (gated)
    live_cognition.rs      # integration test vs real Ollama (gated)
    pipeline.rs            # integration test vs both (gated)
```

Dependencies: `tokio`, `serde`, `serde_yaml`, `reqwest` (HTTP), `webrtc`
(webrtc-rs 0.21), `tracing` (logs).

---

## 3. Implementation order (slices)

Each slice is independently testable and lands a working artifact.

### Slice 1 — `config.rs` (no infra needed)
- Struct `Config { tts: TtsConfig, cognition: CognitionConfig, webrtc: WebrtcConfig }`.
- `TtsConfig { base_url, model, voice }` — voice defaults to `af_heart`.
- `CognitionConfig { base_url, model }` — model defaults to `qwen2.5:3b` (fast-but-dumb; swappable).
- `WebrtcConfig { listen_port }` — defaults to `29434`.
- **Test:** unit test parses a sample `config.yaml`, asserts defaults apply
  when keys are absent, asserts overrides apply when present.

### Slice 2 — `tts.rs` (live Kokoro)
- `TtsClient::synthesize(text) -> impl Stream<Item = AudioChunk>`.
- POSTs to `/v1/audio/speech` with `stream: true`, `response_format: "opus"`.
- Parses the streaming response into sentence-sized audio chunks.
- **Test (live, gated):** synthesize a fixed sentence, assert non-empty
  audio, assert Opus magic bytes, measure time-to-first-chunk.

### Slice 3 — `cognition.rs` (live Ollama)
- `CognitionClient::chat(history: &[Message]) -> String`.
- POSTs to `/api/chat` with `stream: false` (M0: simplest correct path).
- Sends `think: false` at the TOP LEVEL of the request body (not in `options`) —
  harmless for the 3B model, required for the smart tier later.
- **Test (live, gated):** send a fixed prompt, assert non-empty response,
  assert `done_reason == "stop"`.

### Slice 4 — `pipeline.rs` (both, no WebRTC yet)
- `Pipeline::respond(text) -> impl Stream<Item = AudioChunk>`.
- Chains: text → cognition → response text → TTS → audio chunks.
- **Test (live, gated):** "Say hello" → assert audio comes back, measure
  end-to-end latency (cognition + TTS, no transport).

### Slice 5 — `webrtc.rs` (transport)
- WebRTC server: accepts one peer, exposes a data channel (text in) and an
  audio track (Opus out).
- Host-only / no-ICE mode (per 07 risk note — over SSH tunnel, no STUN).
- **Test:** loopback — client sends text over data channel, server echoes
  audio back. (Requires the tunnel up; Lark's step.)

---

## 4. Testing strategy (the emphasis)

Layered, so we can test *now* against the live infra without waiting for the
WebRTC transport to exist.

### Layer 1 — Unit tests (no infra, always run)
- `config.rs` parsing + defaults.
- Any pure logic (message history trimming, chunk framing).

### Layer 2 — Live integration tests (gated behind a feature flag)
- `cargo test --features live-tests` hits the real Kokoro and Ollama.
- Gated so `cargo test` (default) doesn't require the services to be up.
- These are the tests Lark asked for: they prove the server's two HTTP
  clients actually work against the running infra.

### Layer 3 — Contract tests (assert the shapes)
- Pin the exact request/response JSON shapes we verified this session, so a
  Kokoro/Ollama API change breaks the build loudly instead of silently.

### Layer 4 — End-to-end (pipeline, then full loop)
- `pipeline.rs` test: text → audio, no transport.
- Full loop (Slice 5): text over WebRTC → audio back, requires tunnel.

### Manual smoke script
- `scripts/smoke.sh` — curls Kokoro and Ollama directly, prints status +
  latency. The human-readable equivalent of the live tests, for when you
  want to check infra health without running the Rust test suite.

---

## 5. Latency budget (target: sub-second first audio)

| Stage | Expected | Notes |
|-------|----------|-------|
| Cognition (qwen2.5:3b) | ~10 ms | 3B non-thinking; verified warm |
| TTS first chunk (Kokoro) | ~0.15 s | verified 0.16 s for full WAV |
| Transport (WebRTC, local tunnel) | ~10 ms | negligible |

**Honest flag:** with the fast-but-dumb default (`qwen2.5:3b`, ~10 ms warm),
cognition is no longer the latency risk — TTS (~0.15 s) is now the dominant
cost, and the sub-second target is comfortably met. The risk moves to the
*smart* tier: when we swap in `qwen3:30b` (think:false) later, its TTFT is
~10–15 ms warm but it costs ~18 GB to keep resident. Both are `config.yaml`
changes, no rebuild.

---

## 6. Honesty note

- **Verified live this session:** Kokoro request schema + smoke test, Ollama
  `/api/chat` + `/api/generate` shapes, 26-model list, all via `curl`.
- **From training, not verified:** the exact `webrtc-rs` 0.21 API surface
  (data channel + audio track + host-only ICE mode), and `reqwest` streaming
  response handling. These get verified when Slice 2 and Slice 5 are written
  and compiled.

---

## 7. Open questions (none blocking)

- **Streaming TTS vs whole-response:** Slice 2 assumes streaming (sentence
  chunks). If streaming proves fiddly, fall back to `stream: false` + whole
  Opus blob — still correct, just loses the sentence-level pipelining.
- **Cognition streaming:** M0 uses `stream: false` (simplest). Streaming
  cognition (token-by-token) is a later optimization, not M0.
