# 07 — M0 Milestone Plan (minimal voice loop)

## Goal

Prove the full conversational loop end-to-end with the simplest possible
system: Lark speaks on the Mac, Skye hears it, Skye answers, Lark hears her.

```
[Mac client]                          [Server]
  mic → wake word → Moonshine → text    cognition (standalone LLM for M0)
  speaker ← audio                       text → Kokoro → audio
  WebRTC client                         WebRTC server
        ▲                                    │
        └────── text →  ← audio (WebRTC) ────┘
```

M0 is **voice only**. No face, no avatar, no multi-user, no Whisper. One
engine per direction, one buffer, one transcription path.

## Acceptance criteria (what "M0 done" means)

1. Lark says "Hey Skye" on the Mac; the client wakes and starts listening.
2. Lark speaks a sentence; Moonshine transcribes it to text.
3. The text crosses the tunnel to the server via WebRTC data channel.
4. The cognition component receives the text and produces a reply.
5. The reply text is synthesized by Kokoro into audio on the server.
6. The audio crosses back to the Mac via WebRTC audio track (Opus).
7. Lark hears Skye's voice through the Mac speaker.
8. The loop repeats for a second utterance without restarting either binary.

**Latency target:** sub-second from end-of-speech to first audio out. This is
the "instant reaction" requirement from the design decisions.

## Components

| Component | Runs on | Language | Role |
|-----------|---------|----------|------|
| Wake word ("Hey Skye") | Client | Rust | Always-on listener; gates the incoming path |
| Moonshine (STT) | Client | sidecar | Streaming transcription of the utterance |
| WebRTC client | Client | Rust (webrtc-rs) | Sends text, receives audio |
| WebRTC server | Server | Rust (webrtc-rs) | Receives text, sends audio |
| Cognition | Server | Rust + LLM | Text in → text out |
| Kokoro (TTS) | Server | sidecar | Text → audio |

## Sidecar decision (M0)

Both Kokoro and Moonshine are **Python-first** (Kokoro is PyTorch; Moonshine
ships a Python package). For M0 we run each as a **local sidecar process**
that the Rust binary talks to over a local socket (stdin/stdout or a small
JSON-over-stdio protocol). This is the fastest path to a working loop.

**Deferred (not M0):** ONNX/`ort` or `candle` consolidation into a single
self-contained Rust binary. That is a later milestone; M0 accepts the
sidecar cost to unblock voice.

## Cognition interface (decided 2026-09-01)

**M0 uses a standalone LLM, not Skye.** The Rust server interfaces directly
with an LLM, completely separate from Skye's internals. This is deliberate:
we validate the voice pipeline end-to-end against a generic model first,
without touching Skye's engine.

**The eventual interface (post-M0) is queue-based and per-user:**

- The Rust server injects the transcribed request into Skye's **voice input
  queue**, tagged per user.
- Skye's response arrives via the **output queue**, and the router delivers
  it back to the originating user.

This queue design is the integration contract we build toward. It is *not*
implemented in M0 — M0's cognition is a placeholder LLM. Skye integration
happens once the pipeline is proven and we're satisfied with it.

## Build order

### Step 0 — Prerequisites
- [ ] Rust toolchain on the server (confirmed: cargo 1.98.0, rustc 1.98.0).
- [ ] Rust toolchain on the Mac (Lark installs; same version line).
- [ ] Kokoro-82M model + weights downloaded on the server.
- [ ] Moonshine model downloaded on the Mac.
- [ ] SSH tunnel up (Lark's responsibility; `ws://localhost:29434` reachable).

### Step 1 — TTS sidecar (server, standalone)
- [ ] Kokoro sidecar: text in → WAV/Opus out, over stdio.
- [ ] Smoke test: synthesize a fixed sentence, play it back, confirm audio.

### Step 2 — STT sidecar (client, standalone)
- [ ] Moonshine sidecar: audio in → text out, streaming.
- [ ] Smoke test: feed a recorded utterance, confirm transcription.

### Step 3 — WebRTC transport (both, no cognition yet)
- [ ] webrtc-rs server: accepts one peer, exposes a data channel + audio track.
- [ ] webrtc-rs client: connects to `ws://localhost:29434` from `config.yaml`.
- [ ] Loopback test: client sends text over data channel, server echoes it back.

### Step 4 — Wire the loop (integration)
- [ ] Client: wake word → Moonshine → text → data channel.
- [ ] Server: data channel → cognition (standalone LLM) → text → Kokoro → audio track.
- [ ] Client: audio track → speaker.

### Step 5 — End-to-end acceptance
- [ ] Run all 8 acceptance criteria above.
- [ ] Measure end-of-speech → first-audio latency; record it.

## Dependencies

- Step 1 and Step 2 are independent (parallelizable).
- Step 3 depends on the tunnel being up (Lark).
- Step 4 depends on 1, 2, 3.
- Step 5 depends on 4.

## Risks & mitigations

| Risk | Mitigation |
|------|-----------|
| **WebRTC over a local tunnel** — ICE/STUN normally wants public candidates; over a pure local forward we likely run host-only / no-ICE mode. | Spec host-only mode explicitly in Step 3; don't assume ICE discovery works. |
| **Kokoro is PyTorch** — heavy dependency, slow cold start. | Accept for M0; warm the model at sidecar startup so first synthesis isn't slow. |
| **Moonshine streaming vs chunk** — need to confirm it emits partials fast enough for "instant reaction." | Smoke-test latency in Step 2 before wiring the loop. |
| **Wake word false positives/negatives** — "Hey Skye" detection quality. | openWakeWord (MIT, fully local); tune threshold in Step 4. |
| **Audio format mismatch** — mic (PCM) → Moonshine (16kHz mono) → Opus → Kokoro (24kHz). | Fix sample rates explicitly at each boundary; resample where needed. |

## Out of scope (explicitly)

- Face / avatar (phase 2).
- Whisper second-stage STT (v1.1).
- Voice cloning (stock Kokoro voice only).
- Multi-user (single Lark ↔ single Skye).
- TLS / public exposure (SSH tunnel only).
- ONNX/candle consolidation (later milestone).
- Skye integration (M0 cognition is a standalone LLM; see Cognition interface).

## Decisions made (2026-09-01)

1. **Wake-word model** — openWakeWord (MIT, fully local).
2. **Audio codec** — Opus (WebRTC-native).
3. **Cognition interface** — queue-based, per-user (voice input queue in,
   output queue + router out). M0 uses a standalone LLM; Skye integration
   is deferred until the pipeline is proven.

*Last updated: 2026-09-01*
