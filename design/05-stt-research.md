# 05 — STT Engine Research (incoming user speech)

## Problem

The avatar is a two-way loop. TTS (01) is the *outgoing* path — Skye → user.
STT is the *incoming* path — user → Skye:

```
user speaks ──► STT ──► text ──► Skye's cognition ──► text ──► TTS ──► avatar speaks
```

We need a speech-to-text engine that is:

1. **Local** — no cloud API, runs on our own hardware (same constraint as TTS).
2. **Real-time / streaming** — start transcribing while the user is still
   talking, not after they finish. This is the single biggest latency lever in
   a conversational avatar.
3. **Accurate** — good enough that mis-transcription doesn't degrade the
   conversation.
4. **Licensed for our use** — permissive (MIT/Apache) preferred.
5. **Small footprint** — laptop-class CPU or GPU.

## Landscape (surveyed 2026-09-01)

| Engine | License | Streaming | Accuracy | Footprint | Notes |
|--------|---------|-----------|----------|-----------|-------|
| **Moonshine** | MIT (models MIT too) | **Native** | High (claims > Whisper Large V3 on open ASR leaderboard) | Tiny→large (1MB→) | **v1 engine** |
| **whisper.cpp** | MIT | Chunk-based | High | 74M–1.5B | C/C++, Metal on Apple Silicon, Rust bindings (whisper-rs) |
| **faster-whisper** | MIT | Chunk-based | High | 74M–1.5B | CTranslate2, Python, fast on CPU |
| **sherpa-onnx** | Apache-2.0 | Native | High | Small (ONNX) | Rust bindings; does ASR *and* TTS *and* VAD |
| Vosk | Apache-2.0 | Native | Medium | ~50MB | Lightweight, 20+ langs, Rust bindings |
| NVIDIA Parakeet/NeMo | Apache-2.0 | Native | Very high | GPU-heavy | Overkill for laptop CPU |

## The key distinction: streaming vs chunk-based

Whisper-family models (faster-whisper, whisper.cpp) are **chunk-based**: they
transcribe a fixed window of audio after it's been captured. They are accurate
but add latency — you wait for a chunk boundary, then wait for inference.

**Streaming** engines (Moonshine, sherpa-onnx, Vosk) emit partial hypotheses
*while the user is still speaking*. For a conversational avatar, this is the
difference between a natural back-and-forth and a stilted turn-taking delay.

## Phasing: v1 = Moonshine only, v1.1 = Moonshine + Whisper (decided 2026-09-01)

**v1 ships with a single engine: Moonshine.** It is streaming-native, MIT
licensed, and fast enough on the M1 Max to give instant reaction — which is
the explicit priority. One engine, one audio buffer, one transcription path.
Simplest thing that works.

**v1.1 adds Whisper as a second stage.** The two-stage design is a real
pattern (two-tier inference / speculative execution), but it is *not* needed
for the first working loop. It adds coordination complexity (two engines
sharing one buffer, VAD-gated handoff) that we don't want to pay before the
basic loop works.

### The v1.1 two-stage design (deferred, spec'd for later)

The core insight: **the acknowledgment and the answer have different accuracy
requirements.** The ack just needs to know the user finished speaking and
roughly what they said; the answer needs the authoritative transcription.

```
mic → wake word → audio buffer (ring buffer)
                    ├─ Moonshine (streaming) → partial hypotheses
                    │        → [VAD: end-of-speech] → canned ack (instant)
                    └─ Whisper (full utterance, on end-of-speech)
                             → authoritative text → cognition → detailed answer
```

- **Stage 1 — Moonshine (fast).** Streams partial hypotheses while the user
  speaks. On end-of-speech, its latest partial triggers an **instant
  acknowledgment** — a canned template with slot-fill, e.g. *"Hm, interesting
  take on <topic>, <name>! Let me think about that."* The ack is a template
  precisely so it does **not** block on cognition; that's what makes it instant.
- **Stage 2 — Whisper (accurate).** A single clean pass over the complete
  buffered utterance, producing the authoritative transcription that drives
  the actual detailed answer.

**Why two engines instead of one:** Moonshine's streaming partials are
inherently less reliable mid-stream (they self-correct as more audio arrives).
Whisper's chunk-based pass over the full utterance is a single, stable,
high-accuracy result. The two-stage design gets both: instant reaction *and*
an accurate answer, without waiting for the accurate path before saying
anything.

**The ack is canned, not generated.** This is the load-bearing decision. A
generated ack would round-trip through cognition and TTS and lose the latency
win. A template with slot-fill (topic + name from the Moonshine partial) is
fast and still feels responsive.

## Which Whisper (for v1.1): whisper.cpp (recommended)

- **whisper.cpp** — C/C++, **Metal support on Apple Silicon** (a real win on
  the M1 Max), Rust bindings via `whisper-rs`. Fits the Rust client.
- **faster-whisper** — Python (CTranslate2). The fallback if we keep a Python
  sidecar for Moonshine anyway and want one sidecar for both.

**Recommendation:** whisper.cpp for the Metal advantage and the Rust-native
fit. faster-whisper is the sidecar-consolidation alternative.

## Where STT runs: client-side (decided)

With the client/server split (04), STT runs **on the client (Mac)**:

- **Latency-first** — no audio over the network before understanding; the
  text is what crosses the wire (tiny).
- **M1 Max is plenty** — Moonshine runs comfortably in real-time on the
  client's own silicon (and whisper.cpp gets Metal in v1.1).
- **The mic is on the client anyway** — wake word + STT both need it locally.

## Integration decision (mirrors the TTS question)

The same fork as TTS (01): **Python sidecar vs in-process Rust.**

- **Moonshine** is Python-first → fits the sidecar pattern.
- **whisper.cpp** has Rust bindings (`whisper-rs`) → fits in-process Rust.
- **sherpa-onnx** has Rust bindings → could replace *all* of STT + TTS + VAD
  in one ONNX runtime.

**Recommendation:** start with Moonshine (Python sidecar) for v1, and keep
sherpa-onnx as the long-term consolidation target for a single self-contained
Rust binary. whisper.cpp (whisper-rs) slots in for v1.1's second stage.

## Decisions made (2026-09-01)

- **v1 STT** — Moonshine only. Single engine, streaming, instant reaction.
- **v1.1 STT** — add Whisper (whisper.cpp) as a second stage: Moonshine for
  the instant ack, Whisper for the authoritative answer. Deferred until the
  basic loop works.
- **Ack is canned** — template with slot-fill (topic + name), not generated.
  This is what makes the ack instant. (v1.1)
- **Whisper flavor** — whisper.cpp (Metal on M1 Max, Rust bindings). (v1.1)
- **Wake word** — "Hey Skye" is the trigger. Continuous listening with a wake
  word; the wake word enables the upstream (STT) path. No push-to-talk.
- **Latency over accuracy** — streaming-grade accuracy is acceptable; instant
  reaction is the priority.
- **Language** — English-only for v1.
- **Location** — client-side (on the Mac).

*Last updated: 2026-09-01*

[END FILE]
