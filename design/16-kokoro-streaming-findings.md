# 16 — Kokoro Streaming Findings (corrects plan §1.2)

**Date:** 2026-09-02
**Context:** probing Kokoro's streaming response before writing `tts.rs` (Slice 2).

## What I tested (live, this session)

`POST http://localhost:21802/v1/audio/speech` with `response_format: "opus"`,
`stream: true`, `voice: "af_heart"`.

## Findings

1. **`response_format: "opus"` returns Ogg Opus, not raw Opus.**
   - Magic bytes: `OggS` … `OpusHead` … `OpusTags` … audio pages.
   - `content-type: audio/opus`, `Transfer-Encoding: chunked`.
   - This is an **Ogg container**, not bare Opus frames.

2. **It is a SINGLE Ogg stream, not sentence-by-sentence streams.**
   - Tested with 3 sentences: exactly **1 `OpusHead`** (each Ogg stream has one).
   - So `stream: true` means *HTTP chunked delivery of one continuous Ogg
     stream*, NOT multiple concatenated streams per sentence.
   - This **corrects plan §1.1** ("streams sentence-by-sentence"): the
     sentence-level granularity is not visible in the container structure.

3. **`stream: true` does give chunked transfer encoding** — the server can
   consume the Ogg stream incrementally as Kokoro generates it (relevant to
   time-to-first-audio), but the unit of streaming is Ogg pages, not sentences.

## Consequence for the design

Plan §1.2 said "Kokoro returns Opus directly … pipe straight into WebRTC with
no transcoding." That is **half right**:

- ✅ No *transcoding* (no decode → re-encode).
- ❌ There IS a **container demux** step: Ogg Opus → raw Opus packets, because
  WebRTC audio tracks carry raw Opus packets as RTP payload, not Ogg pages.

Demuxing is cheap (container parsing, no codec work) but it is a real step the
plan did not account for, and it needs a Rust crate (`ogg` + `opus`, or
`symphonia`).

## DECISION (Lark, 2026-09-02): Option B — ship Ogg over the data channel

Lark's reasoning: this is **not** a real-time conversation over a changing
network quality, so the WebRTC audio track (with its RTP/Opus-packet framing,
jitter buffers, and adaptive bitrate) is overkill. Shipping the Ogg Opus bytes
as-is over the **data channel** keeps the server simpler and matches the use
case.

**Consequences:**

- **Server:** `tts.rs` fetches Ogg Opus bytes and returns them as-is. **No
  demuxer, no transcoding, no audio track.** The `ogg`/`symphonia` dependency
  is dropped entirely.
- **Transport (Slice 5):** WebRTC data channel carries text (client → server)
  and Ogg Opus bytes (server → client). No audio track, no RTP.
- **Client:** plays the Ogg Opus natively (browsers/Tauri WebView can decode
  Ogg Opus via `<audio>` or Web Audio API).

**What this means for the plan:**

- Plan §1.2's "pipe Opus straight into the WebRTC audio track" is **superseded**.
- Plan §2's crate structure drops the demuxer; `webrtc.rs` becomes "data
  channel in, Ogg Opus bytes out" instead of "audio track out".
- Plan §5's transport stage is unchanged in cost (data channel ≈ audio track
  for a single local peer).

## Verified implementation (Slice 2)

`tts.rs` is written and tested against the live Kokoro:

- `TtsClient::synthesize(text) -> Result<Vec<u8>, TtsError>` returns the full
  Ogg Opus stream.
- Unit test (no infra): request serializes to the expected JSON shape.
- Live test (`--features live-tests`): asserts non-empty, `OggS` magic bytes,
  and `OpusHead` present. **Passes against the running Kokoro.**

[END FILE]
