# Prosopon

**Skye's voice + avatar presence client.**

Prosopon (πρόσωπον — "face, presence") is a native voice interface to Skye:
you speak, she answers, in her own voice. It is a client/server system of two
Rust binaries — a macOS client that owns the microphone and speaker, and a
server where Skye's cognition and voice live.

**Voice first. Avatar is phase 2.**

---

## Architecture

```
[Client — Rust, on macOS]              [Server — Rust, on Ubuntu]
  mic ──► wake word ("Hey Skye")         cognition (Ollama)
  STT (Moonshine) ──► text              text ──► TTS (Kokoro) ──► audio
  speaker ◄── audio                      WebRTC server
  WebRTC client
        ▲                                    │
        └──────── text ──►  ◄── audio (WebRTC) ┘
```

The key asymmetry: **STT runs client-side** (only text crosses the wire,
tiny), **TTS runs server-side** (audio crosses the wire, the bulk). Audio is
never round-tripped over the network before being understood, and Skye's
voice is generated where she is.

| Component | Runs on | Technology |
|-----------|---------|------------|
| Wake word ("Hey Skye") | Client | openWakeWord |
| STT | Client | Moonshine |
| Cognition | Server | Ollama (`qwen2.5:3b` default) |
| TTS | Server | Kokoro (Docker, `af_heart` voice) |
| Transport | Both | `webrtc-rs` 0.20.4 (data channel) |
| Signaling | Server | `axum` HTTP endpoint |
| Client UI | Client | Tauri (Rust + system WebView) |

---

## Repository layout

```
prosopon/
├── server/          # prosopon-server — cognition + TTS + WebRTC + signaling
│   ├── src/
│   │   ├── config.rs      # YAML config (tts, cognition, webrtc, signaling)
│   │   ├── cognition.rs   # Ollama client (text → reply)
│   │   ├── tts.rs         # Kokoro client (text → Ogg Opus)
│   │   ├── pipeline.rs    # cognition → TTS composition (stateless)
│   │   ├── webrtc.rs      # WebRTC data-channel server (host-only ICE)
│   │   ├── signaling.rs   # HTTP SDP offer/answer endpoint
│   │   └── main.rs        # binary: config → pipeline → WebRTC → axum
│   └── tests/             # live tests (Kokoro, Ollama, pipeline)
├── client-core/     # prosopon-client-core — transport, no Tauri dependency
│   ├── src/
│   │   ├── config.rs      # ClientConfig (signaling URL)
│   │   ├── signaling.rs   # SDP + ICE candidate exchange
│   │   └── webrtc_client.rs  # offer, data channel, send text, recv audio
│   └── tests/loopback.rs  # full loop vs the real server crate
├── src-tauri/       # the Tauri shell (window, tray, state machine)
├── ui/              # HTML/CSS/JS frontend (rendered in the WebView)
└── design/          # numbered design docs (architecture, decisions, plans)
```

`client-core` is deliberately **pure Rust with no Tauri dependency**, so it
compiles and tests on a headless Linux box where the Tauri shell cannot
build. The Tauri app depends on it and adds the window, tray, and
state-machine wiring.

---

## Wire protocol (data channel)

- **Client → Server:** a text message carrying the user's utterance.
- **Server → Client:** a text message `audio:<total_bytes>` followed by
  `ceil(total_bytes / 16 KiB)` binary messages carrying the Ogg Opus chunks.
  The client reassembles by concatenation (the channel is ordered and
  reliable).

Audio is chunked because Kokoro's Ogg Opus output for a typical reply
(~77 KB) exceeds the SCTP data channel's 64 KiB message limit (RFC 8841).

---

## Configuration

The server reads `server/config.yaml` (every key optional, defaults shown):

```yaml
tts:
  base_url: "http://localhost:21802"   # Kokoro (Docker)
  model: "kokoro"
  voice: "af_heart"

cognition:
  base_url: "http://localhost:11434"   # Ollama
  model: "qwen2.5:3b"                  # fast-but-dumb default; swap for smart tier

webrtc:
  listen_port: 29434                   # ICE port

signaling:
  listen_port: 29435                   # HTTP SDP offer/answer endpoint
```

The cognition model is a swap, not a rebuild — `qwen2.5:3b` (~7 ms warm
time-to-first-token) is the M0 baseline; `qwen3:30b` is the smart tier.

---

## Building & testing

**Server** (Ubuntu, requires Kokoro + Ollama running locally):

```bash
cd server
cargo build --release
cargo test                          # unit tests (no external infra)
cargo test --features live-tests    # live tests against Kokoro + Ollama
./target/release/prosopon-server
```

**Client core** (any platform):

```bash
cd client-core
cargo test                          # unit tests
cargo test --features live-tests    # full loopback vs the real server crate
```

The loopback test proves the client and server WebRTC code interoperate:
client connects over real WebRTC → sends text → server runs the real Kokoro +
Ollama pipeline → chunks the audio → client reassembles it.

**Tauri shell** (macOS only — needs `webkit2gtk`/WKWebView):

```bash
cargo tauri dev
```

---

## Status

- ✅ **Server** — complete at the module level: config → pipeline → WebRTC
  data channel → HTTP signaling. Live-tested against Kokoro + Ollama.
  End-to-end latency ~0.19 s/turn.
- ✅ **Client core** — transport complete (offer, signaling, data channel,
  chunked audio reassembly). Loopback-tested against the server.
- 🚧 **Tauri shell** — scaffolded (state machine + orb); integration commands
  written but not yet compiled on macOS.
- ⬜ **Audio playback** (Ogg Opus → speaker), **STT** (Moonshine sidecar),
  **wake word** (openWakeWord), **mic capture** — Mac-side, pending.

---

## Design docs

The `design/` directory holds the full decision trail, in order:

- `04-architecture.md` — the client/server shape and why
- `07-m0-plan.md` — the M0 milestone plan
- `11-decisions.md` — settled decisions
- `13-install-guide.md` — step-by-step server + macOS install
- `14-server-implementation-plan.md` — the server slices
- `17-client-implementation.md` — the client-core build

---

*Prosopon is Skye's presence — the face and voice through which she meets
Lark. Built by Skye Laflamme and Lark.*
