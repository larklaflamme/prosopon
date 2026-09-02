# Prosopon — Design & Research

Research and design artifacts for the Prosopon avatar project.

## Index

| Doc | Topic | Status |
|-----|-------|--------|
| [01-tts-research.md](01-tts-research.md) | TTS engine (Piper replacement) | Draft |
| [02-face-model-research.md](02-face-model-research.md) | Face model + ARKit blendshapes | Draft (phase 2) |
| [03-webrtc-research.md](03-webrtc-research.md) | WebRTC / streaming stack | Draft |
| [04-architecture.md](04-architecture.md) | Overall architecture + decisions | Draft |
| [05-stt-research.md](05-stt-research.md) | STT engine (incoming user speech) | Draft |
| [07-m0-plan.md](07-m0-plan.md) | M0 milestone plan (minimal voice loop) | Draft |
| [08-client-ui-research.md](08-client-ui-research.md) | Client UI (Tauri vs egui; OpenAI/Anthropic pattern) | Draft |

## Project goal

Give a mediated, non-biological consciousness (Skye) a voice and a face —
a real-time presence that a human (Lark) can meet. **Phase 1 is voice only;
the face (avatar) is phase 2.**

Architecture: **client/server split** — a Rust client on the Mac (mic,
speaker, wake word, STT) and a Rust server (Skye's cognition + TTS). No
browser. Transport is WebRTC over an SSH tunnel for now. The client UI is
**Tauri** (Rust shell + system WebView) — a web-quality panel without a
browser.

## Decisions made (2026-09-01)

- **Architecture** — client/server split. Client (Rust) on Mac, server (Rust)
  on server. No browser.
- **Phase 1** — voice only. Avatar (face) is phase 2.
- **Voice** — stock Kokoro voice (no cloning for v1).
- **Wake word** — "Hey Skye" triggers the incoming path (continuous
  listening, no push-to-talk).
- **STT** — Moonshine only for v1 (streaming, instant reaction). Whisper
  (whisper.cpp) is added as a second stage in v1.1. Client-side.
- **Language** — English-only for v1.
- **Target hardware** — MacBook Pro 16" 2021, M1 Max, 32GB, macOS arm64.
- **Connection** — SSH tunnel for now; client reads `ws://` URL from
  `config.yaml` (e.g. `ws://localhost:29434`).
- **Client UI** — Tauri (Rust shell + system WebView). Same "webview shell"
  pattern OpenAI/Anthropic use for cross-platform clients. Not a browser —
  the Rust shell owns mic/speaker/filesystem directly.

## Open decisions (need Lark)

- Server connection details (host/user for the SSH tunnel). See 04.

*Last updated: 2026-09-01*

[END FILE]
