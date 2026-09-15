# 27 — Pre-roll → STT Handoff Design

**Status:** Design (for Lark's review — gate before Phase 1)
**Predecessors:** `25-full-duplex-implementation-plan.md` §3 (Phase 1), `26-full-duplex-review.md` Gap 2
**Scope:** Client-side only. The concrete mechanism for "drain pre-roll buffer, then switch to live" into the STT sidecar.

---

## 0. The problem, stated precisely

The wake-word sidecar has detection latency. By the time `WAKE` arrives on its stdout, the user is already saying "…what's the weather". The audio between the wake word and the `WAKE` event is *not* lost — it's sitting in the pre-roll ring buffer — but the STT sidecar isn't running yet, so it never hears it. Result: the first word of the query is clipped.

The fix is to hand the buffered audio to STT *before* it starts consuming live audio. The mechanism for that handoff is what this doc specifies.

## 1. The mechanism — single writer thread, snapshot-drain

**One thread owns the STT stdin, in this order:**

1. **Snapshot** the pre-roll buffer at the moment the wake word fires (immutable `Vec<Vec<f32>>`).
2. **Drain** the snapshot into the sidecar's stdin, in order.
3. **Switch to live** — block on the bus `Receiver<Vec<f32>>` and stream each chunk.

```rust
// stt.rs writer thread, after Phase 0 refactor
fn stt_writer(mut child: Child, snapshot: Vec<Vec<f32>>, rx: Receiver<Vec<f32>>) {
    let stdin = child.stdin.take().unwrap();
    for chunk in snapshot {          // 1. drain pre-roll
        stdin.write_all(&to_pcm(&chunk)).ok();
    }
    while let Ok(chunk) = rx.recv() { // 2. then live
        stdin.write_all(&to_pcm(&chunk)).ok();
    }
}
```

**Why this shape:** one thread, one output stream, deterministic ordering. No merge channel, no tee, no synchronization on the snapshot (it's frozen at capture time). This is the simplest correct mechanism, and it's the one the review flagged as missing.

## 2. What goes in the snapshot — Option A (full drain) for M0

The pre-roll buffer holds the last ~1.5 s, which at wake time contains: silence → the wake word ("Hey Jarvis") → the start of the query.

**Decision for M0: drain the *entire* buffer.** STT transcribes "Hey Jarvis what's the weather". The wake-word prefix is then stripped/ignored downstream.

**Why not offset-drain (Option B):** offset-drain needs the *end-of-wake-word* timestamp, which the openWakeWord sidecar does not currently emit (it prints bare `WAKE` lines). Getting it would mean extending the sidecar's stdout protocol or estimating the offset from detection latency — real complexity, no M0 payoff. Option B is a documented refinement, not a blocker.

**Wake-word prefix handling:** the client already knows the wake word fired (it's the thing that triggered this path). Strip the known wake-word phrase from the transcript prefix if present, case-insensitively, before sending to the server. If STT mis-transcribes the wake word, the server's cognition layer tolerates a stray "hey jarvis" prefix — it's a known, bounded failure, not a correctness bug.

## 3. Startup latency and pipe buffering (the real risk)

The STT sidecar (Moonshine) has a startup delay before it reads stdin. During that delay the writer thread is draining the snapshot into the stdin pipe.

- OS pipe buffer is ~64 KB. Pre-roll at 16 kHz mono f32 = 64 KB/s, so 1.5 s ≈ 96 KB — **larger than the pipe buffer.**
- Consequence: the writer thread **blocks** on `write_all` until the sidecar starts reading. That's *correct* (no data loss — the snapshot is in memory), but it means the live `Receiver` accumulates chunks during the block.

**Mitigation:** the live receiver is a bus subscription; the bus fans to it regardless. Accumulation is bounded by the sidecar's startup time (typically < 1 s), so the backlog is small and self-drains once the sidecar reads. No action needed for M0, but this is *why* the review's "bounded channel" note matters — an unbounded receiver here is fine, an unbounded *bus* is not.

## 4. Exact hook points

| Change | File | Location |
|---|---|---|
| Pre-roll ring buffer (`VecDeque<Vec<f32>>`, cap ~1.5 s) | `src-tauri/src/lib.rs` | `run_conversation_loop()`, cold-state wait loop |
| Snapshot on wake | `src-tauri/src/lib.rs` | wake-event handler, before starting STT |
| `SttDetector::start()` accepts `(snapshot, rx)` | `client-core/src/stt.rs` | `start()` signature + writer thread |
| `pre_roll_secs: f32` (default 1.5) | `client-core/src/config.rs` | `ConversationConfig` |

## 5. Test plan

1. **Unit (stt.rs):** feed a `SttDetector` a snapshot of known PCM + a live receiver; assert the sidecar's stdin receives snapshot bytes *before* live bytes (order assertion).
2. **Integration (loopback):** existing `client-core/tests/loopback.rs` — extend to assert no clipped first word when a wake word is immediately followed by speech.
3. **Manual:** speak "Hey Jarvis, what's the weather" in one breath; confirm the transcript contains "what's the weather" (not "…the weather").

## 6. Open questions for review

1. **Full-drain vs. offset-drain** — confirm Option A (full drain) is acceptable for M0, or do you want the sidecar protocol extended for precise offset (Option B) now?
2. **Wake-word prefix strip** — client-side strip (my recommendation) vs. server-side tolerance vs. both?
3. **`pre_roll_secs = 1.5`** — is 1.5 s enough headroom for typical "Hey Jarvis … query" pacing, or should it be 2.0 s?
