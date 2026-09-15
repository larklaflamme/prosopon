# 26 — Full-Duplex: Critical Review of Plan (25) Against Design (24)

**Status:** Review (2026-09-15)
**Predecessors:** `24-full-duplex-fsm.md` (design), `25-full-duplex-implementation-plan.md` (plan)
**Scope:** Client-side turn-taking. No server changes.

---

## 0. Verdict

The plan is **mostly faithful** to the design, and in two places it is *more accurate* than the design. But there are **three real gaps** that will bite during implementation, and one semantic muddle neither document resolves. Proceed on **Phases 0 + 3 + 4 as one atomic unit**; hold Phase 1 (pre-roll) until gap #2 is designed; get a ruling on gap #1 before writing the loop.

---

## 1. Where the plan corrects the design (good)

1. **The `stt.rs` mic question.** Design §5 flags "not yet verified" whether STT opens its own mic. Plan §0 resolves it: `SttDetector::start()` is a mirror image of `WakeWordDetector::start()` — its own `Mic::start()`, streams to Moonshine's stdin. Verified, closed.
2. **AEC is not "free."** Design §5 says "enable WebRTC AEC on `webrtc_client.rs`" — implying it already exists. Plan §4 correctly identifies it as a *separate* C++ crate (`webrtc-audio-processing`) and the riskiest integration. The plan is more honest here.

---

## 2. The three gaps that matter

### Gap 1 — Two timers, one state, no reconciliation (HIGH)

The design has `silence_timeout_secs = 15` (utterance timeout, fires `Cancel`) and adds a **5s** `inactivity_timeout_secs` (warm gap, fires `InactivityTimeout`). Both live in the **same `Listening` state**. Neither document says how they coexist.

**The problem:** in warm `Listening`, the 5s timer fires *first* (5 < 15), cooling the session before the 15s utterance timeout ever matters. So the 15s timer becomes **dead code in the warm state** — and it's not clear whether `Cancel` (utterance silence) and `InactivityTimeout` (warm gap) are the same event or different events.

**Status:** Phase 3 implementation already added `Cancel` clearing `conversation_active`, which the design never specified. This needs an explicit decision: **is the 15s utterance timeout still meaningful once warm, or does the 5s timer fully subsume it?**

### Gap 2 — Pre-roll → live handoff into STT is underspecified (HIGH)

Plan Phase 1 says "feeds the buffered chunks to STT first, then switches to live streaming." But STT reads from **stdin**, and after Phase 0 it reads from a bus `Receiver`. The mechanism for "drain buffer, then switch to live" is never specified — single writer thread that drains `VecDeque` then the receiver? A tee? This is exactly the kind of detail that turns a "small" phase into a debugging session. It needs a concrete design before Phase 1 starts.

### Gap 3 — AEC demotion changes the barge-in contract (MEDIUM)

Design §6 puts AEC at **step 2** (a prerequisite for correct barge-in). Plan §7 pushes it to **last, optional**. That's justified by the M0-first preference — but the consequence should be named plainly: **M0 barge-in will false-trigger on the agent's own TTS.** And the plan's M0 mitigation "(a) duck playback volume" is a poor fix — it degrades the listening experience and treats the symptom. The better M0 is to **gate the VAD during `Speaking`** or raise the threshold, not duck the output.

---

## 3. Two things the design itself is internally conflicted about

- **Always-on wake word vs. VAD barge-in.** Design §2 cites Alexa's "always-on wake word, even during playback" as a key trick — but §4.5 implements barge-in via VAD *without* the wake word. Plan §8 resolves it by stopping the wake-word sidecar while warm. Consequence: **"Hey Jarvis, stop" won't work mid-conversation** — the user must just talk over the agent. Defensible, but it's a UX consequence Lark should sign off on explicitly, because it contradicts the Alexa reference the design leaned on.
- **Commit atomicity.** Plan §7 orders Phase 3 (FSM) before Phase 4 (orchestration), but doesn't flag that Phase 3 alone leaves the loop broken (`ResponseComplete → Listening` while the loop still expects `Idle`). Phases 0+3+4 must land as one coherent unit, or `master` is half-wired.

---

## 4. Minor notes

- **Pre-roll location:** design says "capture thread," plan says "conversation loop." The plan's separation is *better* (bus shouldn't know about wake words) — flag as intentional divergence.
- **Bus channels:** plan uses unbounded `mpsc::Sender` — won't block the capture thread, but can grow unboundedly if a subscriber stalls. Consider bounded + `try_send` drop-on-full.
- **Design §8 Q3** (AEC always-on vs. only-during-Speaking) is never answered by the plan.

---

## 5. Open rulings needed from Lark

1. **Gap 1** — timer reconciliation: does the 5s warm-gap timer subsume the 15s utterance timeout, or do both remain meaningful?
2. **Gap 2** — pre-roll→STT handoff mechanism (needs a concrete design before Phase 1).
3. **Gap 3** — M0 barge-in mitigation: VAD gating/threshold vs. ducking.
4. **§3** — "Hey Jarvis, stop" mid-conversation: accept the loss, or keep the wake-word sidecar warm?
