# 17 — Client Implementation (client-core + transport findings)

Date: 2026-09-02
Author: Skye Laflamme
Status: client-core built + loopback-verified; Tauri shell wiring pending
Depends on: `14-server-implementation-plan.md`, `16-kokoro-streaming-findings.md`

## What was built

A new **`client-core/`** crate — pure Rust, no Tauri dependency — holding the
client's transport, signaling, and config. It is deliberately GUI-free so it
can be compiled and tested on a headless Linux box (where the Tauri shell
cannot build). The Tauri app (`src-tauri/`) will depend on it.

```
client-core/
├── Cargo.toml
├── src/
│   ├── lib.rs            # ClientError + module decls
│   ├── config.rs         # ClientConfig { signaling.url } (default localhost:29435/offer)
│   ├── signaling.rs      # SignalingClient + SignalingMessage (SDP + candidates)
│   └── webrtc_client.rs  # WebRtcClient (offer, data channel, send text, recv audio)
└── tests/
    └── loopback.rs       # full-loop test vs the real server crate (gated live-tests)
```

## Two critical findings (both fixed)

### 1. Audio exceeds the SCTP data-channel message limit

Kokoro's Ogg Opus output for a 14-word sentence is **76,700 bytes** (measured
2026-09-02). The SCTP data channel's default max message size is **64 KiB**
(RFC 8841). The server's original `send()` of the whole buffer would therefore
fail for typical replies.

**Fix — chunking.** The server now sends audio as a text header
`audio:<total_bytes>` followed by `ceil(total_bytes / 16 KiB)` binary chunks.
The client reassembles by concatenation (the channel is ordered + reliable).
16 KiB is safely under both the 64 KiB default and the stale 16 KiB doc
comment in the webrtc crate.

### 2. Trickle ICE requires candidate exchange

The `webrtc` 0.20.x crate uses **trickle ICE**: candidates are gathered
asynchronously and delivered via `on_ice_candidate`, *not* embedded in the
SDP. The original signaling (SDP only) therefore never exchanged candidates,
and the data channel never opened.

**Fix — non-trickle-style exchange over HTTP.** Both sides collect candidates
via `on_ice_candidate`, wait for `on_ice_gathering_state_change(Complete)`,
and exchange SDP + candidates in a single round-trip. The wire message is:

```json
{
  "type": "offer",
  "sdp": "v=0\r\n...",
  "candidates": [
    { "candidate": "candidate:...", "sdpMid": "", "sdpMLineIndex": 0 }
  ]
}
```

`RTCSessionDescription` is flattened (`sdp_type` is `#[serde(rename = "type")]`)
and `RTCIceCandidateInit` carries the `candidate:` string (via
`RTCIceCandidate::to_json()`).

## Verification (this box)

- `server`: `cargo test` → **13/13 pass** (incl. new chunking + signaling
  round-trip tests).
- `client-core`: `cargo test` → **3/3 pass** (config + signaling round-trip).
- `client-core`: `cargo test --features live-tests --test loopback` → **1/1
  pass** — the full loop: client connects over real WebRTC, sends text, the
  server runs the *real* Kokoro + Ollama pipeline, chunks the audio, the
  client reassembles it, and the result starts with `OggS` magic.

This is the highest-value verification possible before Lark runs the real
Mac ↔ server test: it proves the client and server WebRTC code interoperate,
the candidate exchange works, and the chunked audio round-trips losslessly.

## What is NOT yet built (Mac-side, unverifiable here)

1. **Tauri shell wiring** — `src-tauri/` must depend on `client-core` and add
   commands that drive the transport from the state machine. Cannot compile
   here (no `webkit2gtk`).
2. **Audio playback** — decode Ogg Opus → PCM → speaker (rodio/symphonia).
   Mac audio device required.
3. **STT (Moonshine)** — Python sidecar, JSON-over-stdio.
4. **Wake word (openWakeWord)** — Python sidecar, "Hey Skye".
5. **Mic capture** — cpal, macOS.

## Open architectural question (flagged, not blocking)

The design docs (04, 13) describe an **SSH tunnel** (`ssh -L 29434:...`) for
the WebRTC port. But SSH tunnels forward **TCP**, while WebRTC ICE uses
**UDP**. The signaling (HTTP/TCP) can be tunneled, but the data channel
(SCTP over DTLS over UDP) cannot. This needs a decision before the real
deployment: direct UDP exposure (with a firewall rule), a UDP-capable tunnel
(e.g. WireGuard), or a TURN relay. Not blocking the loopback test, but it
blocks the Mac ↔ server test.
