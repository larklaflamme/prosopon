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

## Decision needed (Lark)

How should the server deliver audio to the client?

- **Option A — demux in server:** server demuxes Ogg → raw Opus packets, feeds
  them into the WebRTC audio track (RTP). Standard, but adds an Ogg demuxer to
  the server and keeps the WebRTC audio-track architecture.
- **Option B — ship Ogg over the data channel:** server forwards the Ogg Opus
  bytes as-is over the WebRTC data channel; client plays them (browsers/Tauri
  can play Ogg Opus natively). Simpler server, but changes the transport from
  "audio track" to "data channel + client-side playback."
- **Option C — WAV/PCM + client-side Opus encode:** server returns WAV, client
  encodes to Opus. Worst latency; not recommended.

**My lean:** Option A — it keeps the plan's audio-track architecture and the
demux is a solved, cheap problem. But it's your call, since it touches the
transport design (Slice 5).
