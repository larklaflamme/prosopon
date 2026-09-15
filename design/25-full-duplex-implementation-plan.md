# 25 — Full-Duplex Implementation & Refactoring Plan

**Status:** Plan (accepted 2026-09-15)
**Predecessors:** `24-full-duplex-fsm.md` (design, accepted 2026-09-14)
**Scope:** Client-side only. No server changes.
**Source:** read live from `/home/ubuntu/prosopon` this turn (mic.rs, wake_word.rs, stt.rs, state_machine.rs, lib.rs, config.rs, playback.rs).

---

## 0. What the code actually does today (verified, not assumed)

The architecture is **sequential mic handoff**, not a shared bus:

- `client-core/src/mic.rs` — `Mic::start()` opens the default input device, resamples to 16 kHz mono f32, forwards chunks over a single `mpsc::Receiver`. `next_chunk()` blocks; `try_next_chunk()` doesn't.
- `client-core/src/wake_word.rs` — `WakeWordDetector::start()` spawns the openWakeWord sidecar, then **creates its own `Mic::start()` inside its writer thread** and streams PCM to the sidecar's stdin. Reads `WAKE` lines from stdout.
- `client-core/src/stt.rs` — `SttDetector::start()` is a mirror image: **its own `Mic::start()`**, streams to Moonshine's stdin, reads `FINAL <text>`.
- `src-tauri/src/lib.rs` — `run_conversation_loop()` is the orchestrator. A single sequential loop:
  1. start wake-word detector → block on `wake_rx.recv()`
  2. `detector.stop()` (frees the mic)
  3. start STT detector → block on `stt_rx.recv_timeout(silence)`
  4. `stt.stop()` (frees the mic)
  5. send text, `recv_audio()`, `play_audio()` (blocks to end)
  6. loop back to step 1
- `src-tauri/src/state_machine.rs` — `ResponseComplete => Speaking → Idle`. This is the single line that breaks multi-turn: after every response we land in `Idle`, which requires the wake word to re-enter `Listening`.
- `client-core/src/playback.rs` — `play_wav()` blocks via `sink.sleep_until_end()`. No way to interrupt mid-playback.

**Two structural facts that block full-duplex:**
1. The mic is opened **twice** (once per detector), and only one detector owns it at a time. There is no way to listen while speaking.
2. Playback is **blocking and uninterruptible**. There is no barge-in hook.

---

## 1. The refactor in one sentence

Replace "sequential handoff of an exclusively-owned mic" with "one shared mic bus that fans every chunk to N consumers, plus an interruptible playback sink, plus a warm/cold conversation flag in the FSM."

---

## 2. Phase 0 — Shared Mic Bus (prerequisite, no behavior change)

**New module:** `client-core/src/mic_bus.rs`

**What it does:** opens the mic ONCE, fans each 16 kHz chunk to every registered subscriber.

**Why a new module, not a tweak:** `cpal::Stream` is `!Send` on CoreAudio (macOS), so the `Mic` must live on exactly one thread. The current code already respects this by creating `Mic` inside each detector's writer thread. A bus centralizes it: one capture thread owns the `Mic`, and fans chunks out over `mpsc::Sender<Vec<f32>>` (which are `Send`).

**API sketch:**
```rust
pub struct MicBus {
    subscribers: Arc<Mutex<Vec<mpsc::Sender<Vec<f32>>>>>,
    capture: Option<thread::JoinHandle<()>>,
}

impl MicBus {
    pub fn start() -> Result<Self, MicError>;   // spawns capture thread
    pub fn subscribe(&self) -> mpsc::Receiver<Vec<f32>>;  // new consumer
}
```

**Capture thread body:**
```rust
let mic = Mic::start()?;              // stays on THIS thread
while let Ok(chunk) = mic.next_chunk() {
    let subs = subscribers.lock().unwrap();
    subs.retain(|tx| tx.send(chunk.clone()).is_ok());  // drop dead consumers
}
```

**Refactor of the two detectors:** `WakeWordDetector::start()` and `SttDetector::start()` stop calling `Mic::start()`. Instead they accept a `Receiver<Vec<f32>>` (a bus subscription) and stream from it. The `Mic` import disappears from both files.

**Hook points (exact):**
- `wake_word.rs` writer thread (~line 110): replace `let mic = Mic::start()?` + `mic.next_chunk()` loop with `while let Ok(chunk) = rx.recv()`.
- `stt.rs` writer thread (~line 105): same.
- `lib.rs` `run_conversation_loop()`: create ONE `MicBus` at loop start; pass subscriptions to each detector instead of letting them open the mic.

**Test:** `client-core/tests/loopback.rs` already exists. Add a test that subscribes two receivers to one bus and asserts both see the same chunks.

---

## 3. Phase 1 — Pre-roll buffer (fixes clipped first word)

**Problem:** the wake-word sidecar has detection latency. By the time `WAKE` arrives, the user is already saying "…what's the weather". The audio between the wake word and the `WAKE` event is lost because the wake-word detector consumed it.

**Fix:** a rolling ring buffer of the last ~1.5s of chunks, maintained by the conversation loop (not the bus — the bus shouldn't know about wake words). The loop subscribes to the bus, and while waiting for the wake event, it also pushes chunks into a `VecDeque<Vec<f32>>` capped at ~1.5s (24,000 samples). On wake, it feeds the buffered chunks to STT first, then switches to live streaming.

**Config:** add `pre_roll_secs: f32` (default 1.5) to `ConversationConfig`.

**Hook point:** `run_conversation_loop()` step 1→2 transition.

---

## 4. Phase 2 — AEC (echo cancellation) — the hard part

**Problem:** while the agent speaks, the mic hears the agent's own TTS (speaker → room → mic). A naive VAD barge-in detector would false-trigger on this echo.

**Two-tier plan (be honest about risk):**

**M0 — no AEC, ducking + VAD gating.** While speaking, run a simple energy VAD on the mic. To avoid self-trigger, either (a) duck playback volume, or (b) accept false-positives and rely on a "barge-in requires sustained speech > 300ms" heuristic. Cheap; gets multi-turn working; barge-in will be flaky.

**M1 — real AEC.** Integrate `webrtc-audio-processing` (the `webrtc-audio-processing` crate). Feed it the reference signal (the TTS samples being played) + the mic signal; it outputs echo-cancelled mic audio. Run VAD on the cleaned signal. This is the Alexa iAEC approach from the research note.

**Risk flag:** `webrtc-audio-processing` is a C++ build (bindgen + cmake), and wiring it into a rodio-playback + cpal-capture loop is the single riskiest integration in this whole plan. Recommend M0 first, M1 as a follow-up. Verify the crate builds on arm64 macOS before committing to it.

**Hook point:** `playback.rs` needs to expose the reference signal (currently `play_wav` discards it), and the conversation loop needs a VAD consumer on the bus.

---

## 5. Phase 3 — Extended FSM

**Changes to `state_machine.rs`:**

1. Add `conversation_active: bool` to `ClientState` (orthogonal flag, like `muted`).
2. Add two transitions:
   - `BargeIn` — legal from `Speaking`; → `Listening` (sets `conversation_active = true`).
   - `InactivityTimeout` — legal from `Listening` when warm; → `Idle` (clears `conversation_active`).
3. Make `ResponseComplete` conditional: `Speaking → Listening` if `conversation_active`, else `Speaking → Idle`.
4. `WakeWord` sets `conversation_active = true` on entry.

**Hook points (exact):**
- `state_machine.rs` `Transition` enum (~line 60): add `BargeIn`, `InactivityTimeout`.
- `state_machine.rs` `apply()` `ResponseComplete` arm (~line 110): the conditional.
- `ClientState` struct (~line 45): add `conversation_active`.

**Tests:** extend the existing `#[cfg(test)]` module — warm multi-turn path, barge-in path, inactivity-timeout path.

---

## 6. Phase 4 — Orchestration rewrite (multi-turn + barge-in)

**Rewrite `run_conversation_loop()`** to:

1. Create ONE `MicBus` at loop start (lives for the whole session).
2. **Cold state:** run wake-word detector on a bus subscription. Maintain pre-roll buffer.
3. On wake: `WakeWord` transition (sets warm), drain pre-roll into STT.
4. **Warm state:** run STT on a bus subscription. On `FINAL`, `UtteranceComplete` → `Thinking`.
5. Send + receive audio. `ResponseStarted` → `Speaking`.
6. **While speaking:** run VAD (M0) or AEC+VAD (M1) on a bus subscription. On barge-in: interrupt playback, `BargeIn` → `Listening`, loop to step 4.
7. On `ResponseComplete`: if warm → `Listening` (loop to step 4); if cold → `Idle` (loop to step 2).
8. **Inactivity:** a 5s timer in the warm state; on expiry, `InactivityTimeout` → `Idle` (cold).

**The playback change (required for barge-in):** `play_wav` must become interruptible. Replace `sink.sleep_until_end()` with a loop that checks a shared `AtomicBool` "stop" flag every ~50ms, and calls `sink.stop()` when set. The barge-in detector sets the flag.

**Hook points (exact):**
- `playback.rs` `play_wav()` — add an interruptible variant `play_wav_interruptible(bytes, stop_flag)`.
- `lib.rs` `run_conversation_loop()` — full rewrite of the loop body.
- `config.rs` `ConversationConfig` — add `inactivity_timeout_secs` (default 5), `pre_roll_secs` (default 1.5).

---

## 7. Order of work & dependencies

```
Phase 0 (mic bus)        ← prerequisite, no behavior change
   ↓
Phase 3 (FSM)            ← pure logic, unit-testable, no audio
   ↓
Phase 1 (pre-roll)       ← small, fixes clipped word
   ↓
Phase 4 (orchestration)  ← multi-turn works (M0 barge-in, flaky)
   ↓
Phase 2 M1 (AEC)         ← real barge-in, riskiest
```

Phases 0 and 3 are independent and can be done in parallel. Phase 4 depends on 0, 1, 3. Phase 2-M1 is last and optional for the first demo.

---

## 8. Risks & unknowns (honest)

1. **AEC integration** (Phase 2-M1) — C++ build on arm64 macOS, rodio/cpal wiring. Highest risk. Verify crate builds before committing.
2. **`cpal::Stream` `!Send`** — the bus must keep the `Mic` on one thread. Handled by the capture-thread design, but it's the constraint that shapes everything.
3. **STT sidecar latency** — Moonshine streaming may add latency that makes barge-in feel sluggish. Measure before tuning.
4. **Two sidecars alive simultaneously** — in the warm state, do we keep the wake-word sidecar running (to catch "Hey Jarvis, stop") or rely on VAD barge-in? The design says VAD barge-in (no wake word needed while warm), so the wake-word sidecar can be stopped while warm. Consequence: "Hey Jarvis" won't work mid-conversation — acceptable per the design.

---

## 9. Resolved decisions (2026-09-15)

1. **M0/M1 split** — M0 first (flaky-but-cheap barge-in: ducking + VAD gating). M1 (AEC) as a follow-up.
2. **Inactivity timeout** — separate `inactivity_timeout_secs = 5`, distinct from `silence_timeout_secs = 15` (utterance timeout).
3. **Mac tunnel** — up on port 2222; commits go to the prosopon repo.
