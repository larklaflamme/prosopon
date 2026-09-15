# 24 — Full-Duplex FSM: Multi-Turn + Barge-In

**Status:** Design (accepted by Lark 2026-09-14)
**Scope:** Client-side turn-taking. No server changes required.
**Predecessors:** `04-architecture.md`, `20-streaming-orchestration.md`, `22-alice-speaker.md`, `23-alice-solution.md`

---

## 1. Goal

Two behaviors, both requested by Lark:

1. **Wake word once, then multi-turn.** Say "Hey Jarvis" to *start* a conversation. After that, the conversation flows naturally — the user speaks, the agent responds, the user speaks again — **without** re-saying the wake word, until a **5-second inactivity gap** returns the client to the cold state (wake word required again).

2. **Barge-in.** Any new voice input from the user interrupts the agent's current response. "Stop, that's enough" must cut the TTS mid-sentence and hand the floor back to the user.

---

## 2. What the three reference systems taught us

(Full research in `notes/prosopon-full-duplex-research.md` and `22-alice-speaker.md`.)

| System | Wake word | Barge-in mechanism | Key trick |
|---|---|---|---|
| **Alexa** | Always-on, even during playback | Wake word *during* playback ("Alexa, stop") | Buffer mic while playing, run wake-word spotter continuously |
| **Yandex Alice** | Always-on | **"Quick commands"** — a second tiny CNN recognizes bare "stop"/"louder"/"pause" *without* the wake word, specifically while speaking | AEC front-end + parallel quick-command model |
| **ChatGPT voice** | Session-based (no per-turn wake word) | VAD-driven; user speech over the model's audio triggers interruption | Full-duplex: model listens while speaking |

**The synthesis for us:** Alexa's "buffer while playing" + Alice's "barge-in without wake word" + ChatGPT's "session-based, no per-turn wake word" = exactly the two behaviors Lark wants. We don't need a second CNN (Alice's quick-commands) — a VAD threshold on AEC-cleaned mic audio gives us barge-in for free, and bare "stop" is just the degenerate case of "user is talking over me."

---

## 3. Current architecture (verified against the Mac this turn)

The client is a **Tauri app** (`src-tauri/`) over a **pure-Rust core crate** (`client-core/`), with **Python sidecars** for the ML models.

### 3.1 The existing state machine (`src-tauri/src/state_machine.rs`)

States (presence of Skye, drives orb color/motion):

```
Disconnected → Idle → Listening → Thinking → Speaking → Idle
```

Transitions today:

| Transition | From → To | Notes |
|---|---|---|
| `Connect` | Disconnected → Idle | |
| `Disconnect` | any → Disconnected | |
| `WakeWord` | Idle → Listening | **blocked if muted** |
| `UtteranceComplete` | Listening → Thinking | |
| `ResponseStarted` | Thinking → Speaking | |
| `ResponseComplete` | Speaking → Idle | ← **this is where multi-turn breaks** |
| `Cancel` | Listening/Thinking → Idle | silence timeout / error |
| `ToggleMute` / `SetMute` | orthogonal flag | |

The problem is visible in the table: `ResponseComplete` goes `Speaking → Idle`, and `Idle` requires `WakeWord` to re-enter `Listening`. So every turn currently needs the wake word. That's the single line to change for multi-turn.

### 3.2 The audio subsystems (`client-core/src/`)

- **`wake_word.rs`** — spawns `sidecar/wake_word.py`, streams mic PCM to its stdin, reads `WAKE\n` lines from stdout, emits wake events on an `mpsc::Receiver<()>`. **The mic is created *inside* the writer thread** (`Mic::start()`), because `cpal::Stream` is `!Send` on CoreAudio.
- **`stt.rs`** — spawns `sidecar/stt.py` (Moonshine), reads `PARTIAL <text>` / `FINAL <text>` lines.
- **`playback.rs`** — plays Ogg Opus chunks from the server.
- **`webrtc_client.rs`** — the data channel (client→server text, server→client `audio:<bytes>` + Opus chunks).
- **`mic.rs`** — `Mic::start()` / `next_chunk()` (16 kHz mono f32).

### 3.3 The critical architectural fact

**The mic is currently owned exclusively by the wake-word detector's writer thread.** The wake-word sidecar gets a continuous stream of mic audio; STT is a *separate* sidecar. For full-duplex we need mic audio to feed **three** consumers simultaneously:

1. Wake-word spotter (cold state)
2. STT (listening)
3. VAD / barge-in detector (speaking)

This fan-out is the single most important refactor. It is the "order of actions" problem Lark flagged — *who gets the audio, and in what order* — and it must be solved before any of the FSM logic lands.

---

## 4. The design

### 4.1 Shared mic bus (the prerequisite)

Introduce a single mic capture that **broadcasts** each chunk to subscribers, instead of the wake-word detector owning the mic.

- One `Mic` instance, one capture thread.
- A `broadcast` channel (or `Arc<Mutex<Vec<Sender>>>`) fans each 80 ms chunk to N subscribers.
- Subscribers: wake-word sidecar writer, STT sidecar writer, VAD/barge-in detector.
- The `!Send` constraint stays satisfied: the `Mic` lives on the capture thread; only `f32` chunk *values* cross thread boundaries (they're `Send`).

This is a mechanical refactor of `wake_word.rs` + `mic.rs` + `stt.rs`, no new dependencies.

### 4.2 Pre-roll buffer

Keep a **rolling ~1.5 s ring buffer** of the most recent mic chunks. When the wake word fires, prepend the buffered audio to the STT stream so the first word of the command isn't clipped ("Hey Jarvis, **what's the weather**" — the bold part is already in the buffer).

- 1.5 s at 16 kHz mono f32 = 24,000 samples = ~96 KB. Trivial.
- Lives on the capture thread, drained on wake-word event.

### 4.3 AEC (echo cancellation)

We play TTS through speakers while the mic is live. Without AEC, the mic hears the agent's own voice and the VAD will false-trigger barge-in on every response.

- **WebRTC already ships a software AEC.** We're already using WebRTC for the data channel. Use its AEC on the mic path during `Speaking` (and ideally always).
- This is the one piece of Alice's Tier-1 DSP we *do* need — but we get it from WebRTC, not a Kalman-per-subband filter.

### 4.4 The extended FSM

**States unchanged.** **One new orthogonal flag** + **two new transitions.**

New flag: `conversation_active: bool` (orthogonal, like `muted`).

- Set `true` on the first `WakeWord` (cold start).
- While `true`, `ResponseComplete` → `Listening` directly (no wake word).
- Cleared by the 5 s inactivity timer.

New transitions:

| Transition | From → To | Condition |
|---|---|---|
| `BargeIn` | Speaking → Listening | VAD detects sustained user speech (>~300 ms) over the response |
| `InactivityTimeout` | Listening → Idle | 5 s with no user speech, `conversation_active` cleared |

Revised `ResponseComplete`:

```
ResponseComplete:
  if conversation_active:  Speaking → Listening   (multi-turn)
  else:                    Speaking → Idle        (cold, needs wake word)
```

Revised `WakeWord`:

```
WakeWord:
  Idle → Listening, sets conversation_active = true
```

### 4.5 Barge-in detection (the "Stop, that's enough" case)

During `Speaking`:

1. Mic audio → AEC (remove the agent's own TTS) → VAD.
2. VAD: energy/RMS threshold with a short hangover (~300 ms) to reject coughs and single syllables.
3. On sustained speech: fire `BargeIn` → stop TTS playback immediately → `Listening` → STT picks up the user's utterance.

Bare "stop" is handled by the same path — it's just a short utterance that crosses the VAD threshold. No wake word, no second CNN.

### 4.6 The 5-second inactivity timer

While in `Listening` (warm, `conversation_active`):

- A timer resets on every VAD-detected speech frame.
- If 5 s elapse with no speech: fire `InactivityTimeout` → `Idle`, clear `conversation_active`.
- Next turn requires the wake word again.

---

## 5. Exact hook points (verified this turn)

| Change | File | Location |
|---|---|---|
| `ResponseComplete` multi-turn branch | `src-tauri/src/state_machine.rs` | `Transition::ResponseComplete` arm |
| Add `BargeIn` + `InactivityTimeout` variants | `src-tauri/src/state_machine.rs` | `enum Transition` |
| Add `conversation_active` flag | `src-tauri/src/state_machine.rs` | `struct ClientState` |
| Mic fan-out (shared bus) | `client-core/src/wake_word.rs` + `mic.rs` | `WakeWordDetector::start` writer thread — extract `Mic` ownership |
| Pre-roll ring buffer | `client-core/src/wake_word.rs` | writer thread, before `stdin.write_all` |
| VAD / barge-in detector | `client-core/src/` (new module, e.g. `vad.rs`) | new |
| AEC | `client-core/src/webrtc_client.rs` | enable WebRTC AEC on the audio path |
| STT sidecar wiring for barge-in | `client-core/src/stt.rs` | feed pre-roll + post-barge-in audio |

**Not yet verified** (flagged for the implementation pass): whether `stt.rs` currently opens its *own* mic or expects audio to be handed to it. The sidecar reads stdin, so the Rust side must be feeding it — but I have not confirmed whether that feed is a second `Mic::start()` or a shared source. This is the first thing to check when we start coding, because it determines the exact shape of the fan-out refactor.

---

## 6. Order of actions (the part that matters)

The sequence, in dependency order:

1. **Shared mic bus** — one capture, fan-out to subscribers. (Everything else depends on this.)
2. **AEC on the mic path** — so barge-in doesn't false-trigger on our own TTS.
3. **Pre-roll buffer** — so the first word isn't clipped.
4. **VAD / barge-in detector** — the `BargeIn` transition.
5. **FSM changes** — `conversation_active` flag, `ResponseComplete` multi-turn branch, `InactivityTimeout` timer.
6. **Wire STT to barge-in** — pre-roll + post-barge-in audio → STT → server.

Steps 1–3 are prerequisites; 4–6 are the actual behavior. Do them in this order and each step is testable in isolation.

---

## 7. What we deliberately skip

- **XMOS / Tier-1 DSP, beamforming, 6-mic DoA** — we have one mic, no array, no deterministic DSP. Not our problem.
- **Alice's quick-command CNN** — a second model to recognize bare "stop" is redundant when VAD barge-in already covers it.
- **NeurIPS control-token LLM** — semantic turn-taking via LLM-emitted `[C.SPEAK]`/`[S.LISTEN]` tokens requires fine-tuning an LLM. Overkill now; it's the future upgrade path if threshold-based turn-taking proves too crude.

---

## 8. Open questions for Lark

1. **Barge-in sensitivity** — 300 ms / energy threshold is a starting point. Do you want a user-facing "interruption sensitivity" knob, or a fixed tuned value for v1?
2. **5 s inactivity** — is 5 s the right number, or should it be configurable in `client/config.yaml`?
3. **AEC scope** — always-on AEC (slightly more CPU, cleaner signal) vs. AEC only during `Speaking` (cheaper, but a cold-start wake word while the agent is silent doesn't need it). I lean always-on for simplicity.
