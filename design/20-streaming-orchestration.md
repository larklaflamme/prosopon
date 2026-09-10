# 20 — Streaming Orchestration & Two-Tier Response (Monday demo)

Date: 2026-09-10
Author: Skye Laflamme
Status: design (not yet implemented)
Depends on: 04-architecture.md, 14-server-implementation-plan.md, 16-kokoro-streaming-findings.md

## Goal

Demonstrate near-zero-latency conversational response for the Safe City demo.
The user says the wake word, states a query, and the system responds with a
fast summary first, then plays the full response when it is ready.

## The core insight: perceived latency != total latency

The user does not need the complete answer instantly — they need *something*
instantly so the system feels alive, then the full answer arrives as a natural
continuation. We split the response into two tiers:

1. **Fast summary** — a short answer that plays within ~300ms of the query
   ending. This is the "it heard me and it is already responding" moment.
2. **Full response** — the complete answer, played when ready, seamlessly
   following the summary.

## Two-tier architecture

Two options for generating the summary:

- **Option A — small fast model.** A tiny model produces a one-sentence summary
  in ~100–200ms while the main model generates the full response in parallel.
  Hard latency guarantee, but two models running.
- **Option B — summary is the first sentence of the full response.** Stream the
  full response; the moment the first sentence completes, speak it as the
  "summary," then continue. One model, no duplication, and the summary can
  never contradict the full answer (it *is* the full answer's opening).

**Decision: Option B for Monday** — simpler, one pipeline, and the summary and
full response are guaranteed consistent.

## The streaming glue (the new work)

The pipeline stages and where latency lives:

| Stage     | Component    | Latency trick                                        |
| --------- | ------------ | ---------------------------------------------------- |
| Wake word | openWakeWord | always-on, ~0 added                                  |
| STT       | Moonshine    | stream partial transcripts, don't wait for end-of-speech |
| Cognition | Ollama       | stream tokens, act on the first token                |
| TTS       | Kokoro       | synthesize first chunk before the full sentence exists |
| Transport | WebRTC       | already low-latency, full-duplex                      |

The overlap: while the LLM generates word 40, TTS speaks word 1. The user hears
the response *begin* within a few hundred ms of finishing their sentence.

## Barge-in

The user can interrupt; the system stops talking instantly. A system that feels
interruptible feels alive. This matters more than raw speed.

## What's done vs. what's new

| Piece                              | Status          |
| ---------------------------------- | --------------- |
| Wake word (openWakeWord)           | done            |
| Mic capture                        | done            |
| STT (Moonshine)                    | sidecar built   |
| TTS (Kokoro)                       | sidecar built   |
| WebRTC transport                   | done, tested    |
| Two-tier orchestration             | **new**         |
| Streaming glue (STT→LLM→TTS overlap) | **new**       |

## Monday demo surface

Three things, in order of impressiveness:

1. Response begins before the sentence ends (streaming overlap).
2. Barge-in (talk over it, it stops and listens).
3. Emotion in voice (prosody: pitch/rate/energy mapped to emotional state).
