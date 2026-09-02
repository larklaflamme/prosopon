# 15 — Cognition Model Selection (fast, non-thinking)

**Date:** 2026-09-01
**Requirement (Lark):** no thinking model — near-zero time-to-first-token for the
voice loop. **Decision (2026-09-01): fast-but-dumb model first.** The pipeline
is the deliverable; the model is a `config.yaml` swap. We know how to get a
smart response — the key business value right now is near-zero latency.

## Hardware (verified live)

- **GPU:** NVIDIA H100 PCIe, 80 GB (81,559 MiB total)
- **CPU:** AMD EPYC 9124, 32 threads
- **RAM:** 188 GiB total

## Key finding: which models are "thinking" models

Tested live via `curl`/Python against `http://localhost:11434/api/chat`
(streaming), counting `message.thinking` tokens vs `message.content` tokens.

| Model | Thinking? | Notes |
|-------|-----------|-------|
| `muse-glimmer` (27.9B) | **Yes** | `think:false` → empty output (does NOT cleanly disable) |
| `qwen3:30b` | **Yes (default)** | emits `thinking` tokens by default |
| `gemma4:26b` | **Yes** | emits `thinking` tokens |
| `qwen2.5:3b` | No | small, fast, weak conversation quality |
| `llama3.2:latest` (3B) | No | small, fast, very terse |
| `tinyllama:latest` (1.1B) | No | smallest, fastest, dumbest |

## The critical implementation detail

Thinking is disabled by passing `think: false` at the **TOP LEVEL** of the
Ollama request body — **NOT** inside `options`:

```json
{ "model": "qwen3:30b", "think": false, "messages": [...], "stream": true }
```

- `options: {"think": false}` → **does not work** (thinking tokens still emitted)
- top-level `"think": false` → **works** (0 thinking tokens, straight to content)

## Verified TTFT (warm, H100)

### Fast-but-dumb candidates (non-thinking, 3 clean warm runs each)

| Model | Size | TTFT | Thinking | Content tokens |
|-------|------|------|----------|----------------|
| `tinyllama:latest` | 1.1B | **~5 ms** | 0 | 3–8 |
| `llama3.2:latest` | 3B | ~7–8 ms | 0 | 2 (very terse) |
| `qwen2.5:3b` | 3B | ~7–10 ms | 0 | 9–10 (fuller) |

All three are effectively instant. The differentiator is response quality, not
speed — they're all within a few ms of each other.

### Smart candidate (for later, config swap)

`qwen3:30b` with top-level `think: false`, 3 consecutive warm runs:

| Run | TTFT | Tokens |
|-----|------|--------|
| 1 | 10 ms | 30 |
| 2 | 14 ms | 30 |
| 3 | 15 ms | 30 |

**~10–15 ms.** Also effectively instant when warm — but it's a 30B model, so
keeping it warm costs ~18 GB of the 80 GB H100, and it's the "smart" tier we
defer until the pipeline is proven.

## Decision (2026-09-01)

**Default model: `qwen2.5:3b`** — the best quality among the fast-but-dumb
models (fuller responses than `llama3.2`, same ~7 ms TTFT), non-thinking,
tiny enough to keep warm for free.

**Architecture:** the model is a `config.yaml` swap. The pipeline is
model-agnostic. When we want smart, we change one line:

```yaml
cognition:
  model: "qwen2.5:3b"     # fast-but-dumb default (M0)
  # model: "qwen3:30b"    # smart tier, think:false, swap later
```

`muse-glimmer` (Lark's earlier choice) is a thinking model whose thinking
cannot be cleanly disabled — it returns empty output with `think:false`.
It does not meet the speed requirement and is dropped.

## Server implementation consequence

1. The Rust server's Ollama client MUST send `think: false` at the top level
   of the request body (not in `options`) — harmless for non-thinking models,
   required for the smart tier later.
2. The model must stay **warm** in GPU memory. A 3B model is ~2–3 GB — nearly
   free to keep resident on the 80 GB H100. Cold load is ~7 s, which would
   blow the sub-second target on the first utterance.
3. The pipeline must be model-agnostic: cognition is "text in → text out,"
   and the model name comes from `config.yaml`, never hardcoded.

## Honesty note

- **Verified live this session:** hardware (nvidia-smi), thinking-token
  behavior of muse-glimmer / qwen3:30b / gemma4:26b, the top-level
  `think:false` mechanism, and clean warm TTFT of tinyllama / llama3.2 /
  qwen2.5:3b / qwen3:30b — all via direct requests to the running Ollama.
- **Not verified:** whether `qwen3:32b` or `qwen3.6:35b` are thinking models
  (same qwen3 family, likely yes — not tested). Irrelevant for M0 since we're
  on the 3B tier.
- **Judgment call, not measurement:** `qwen2.5:3b` over `llama3.2` as the
  default is a quality preference (fuller responses), not a speed difference.
  Both are ~7 ms. The real quality/speed tradeoff should be re-evaluated in
  the live pipeline test (Slice 4).
