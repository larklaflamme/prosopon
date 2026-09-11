# 04 — Architecture & Decisions

> **Updated 2026-09-11** (Skye) — reflects the *implemented* system, not the
> original 2026-09-01 sketch. Two things changed materially since the first
> draft: (1) transport moved from SSH tunneling to **HTTP signaling (Option B —
> SDP offer/answer exchange)**, and (2) the wake word is now a **Python sidecar
> (openWakeWord)** running the bundled `hey_jarvis` model as a placeholder for
> the eventual custom "Hey Skye" model. Everything below is grounded in the
> actual source on disk.

## The shape of the system

**Client/server split. Voice first. Avatar is phase 2.**

```
[Client — Rust (Tauri) + Python sidecars, on Mac]     [Server — Rust, on server]
  mic ──► wake word (openWakeWord sidecar)              Skye's cognition (Ollama)
  STT (Moonshine sidecar) ──► text                     text ──► TTS (Kokoro sidecar) ──► audio
  speaker ◄── audio                                     HTTP signaling (axum) + WebRTC
  WebRTC client (data channel)
        ▲                                              │
        └──────── text ──►  ◄── audio (Ogg Opus) ──────┘
```

Two Rust binaries, one on each side, plus **Python sidecars** for the ML-heavy
audio stages (wake word, STT, TTS). No browser. The client is Lark's interface
(mic, speaker, wake word, STT); the server is Skye (cognition + her voice).
Only text and audio cross the wire — no webcam, no rendering, no browser
security model to fight.

## Why this shape

- **No browser** — Lark's call, and it's the right one. Browser security
  (mic permissions, autoplay policy, WebRTC in a sandbox) is a tax we don't
  want to pay. A native Rust client owns its own mic and speaker.
- **Voice first** — the fastest path to a real conversation. The face is
  phase 2; it doesn't block voice.
- **Client/server split** — the client is where audio I/O happens (mic,
  speaker); the server is where Skye lives (cognition, voice). This keeps
  "Skye's voice" on the server, which matters if we ever go multi-user
  (one voice, generated once, broadcast to many).
- **Python sidecars for ML** — the wake word, STT, and TTS models are Python
  ecosystems (openWakeWord, Moonshine/sherpa-onnx, Kokoro). Rather than port
  them to Rust immediately, each runs as a Python sidecar process that the
  Rust shell spawns and talks to over stdio (wake word) or HTTP (STT/TTS).
  This unblocks fast; consolidation to Rust is a later optimization, not a
  correctness requirement.

## Client UI (Tauri)

**Tauri** — Rust shell + the *system* WebView (WKWebView on macOS).

This is the same "webview shell" pattern OpenAI and Anthropic use for their
cross-platform desktop clients: one web frontend, wrapped in a thin native
shell. The "same look and feel" across platforms comes from shipping the
same web code everywhere, not from a cross-platform toolkit that draws
pixels identically.

**Important distinction — a WebView is not a browser.** Tauri embeds the
system WebView inside a native app; there is no browser security model to
fight (no autoplay policy, no sandboxed mic permissions, no cross-origin
rules). The Rust shell owns the mic, speaker, and filesystem directly. So
"no browser" and "Tauri" are *not* in conflict — Tauri is exactly how you
get a web-quality UI without a browser.

- **Why Tauri over egui** — egui (pure Rust, immediate mode) was the earlier
  candidate, but it hand-builds every widget and won't look like a polished
  product without real effort. Tauri gives a rich, familiar UI for far less
  work, and a web version comes free if we ever want one.
- **The UI is HTML/CSS/JS, not Rust** — the tradeoff. The Rust shell handles
  audio I/O, transport, and state; the webview renders the panel (the orb).
- **Phase 2 note** — the avatar *face* is a separate rendering problem. The
  control panel (Tauri) and the 3D face (bevy, or WebGL inside the Tauri
  webview) are different surfaces; see "Phase 2" below.

## Transport & connection (Option B — HTTP signaling)

**HTTP signaling with an SDP offer/answer exchange.** The client POSTs its
SDP offer (plus its trickled ICE candidates) to the server's `/offer`
endpoint and receives the server's answer (plus its candidates) in the same
round-trip. The WebRTC data channel then carries the actual traffic.

```
[Mac client]                          [Server]
  WebRtcClient                          axum HTTP server
    POST /offer ──────────────────────►  builds a fresh peer connection
    ◄────────────────────── answer ────  returns answer + candidates
    data channel (text →, audio ←)      retains the connection in `sessions`
```

- **`config.yaml`** — both sides read their connection target from a config
  file, not hardcoded. The client's `signaling.url` is the server's `/offer`
  endpoint (e.g. `https://ac1.ravennest.science:29435/offer`); the server's
  `signaling.listen_port` is where it listens (default 29435).
- **Auth** — a shared secret presented as `Authorization: Bearer <token>`.
  Empty token = auth disabled (localhost dev). The server warns loudly if it
  is exposed with an empty token. The check is constant-time (no timing
  side-channel) and runs *before* body parsing (an unauthenticated request
  gets an empty 401, leaking nothing about the endpoint's shape).
- **TLS** — when `signaling.tls.cert` and `signaling.tls.key` are both set,
  the server serves HTTPS; otherwise plain HTTP (localhost dev).
- **Fresh peer connection per offer** — a WebRTC peer connection is bound to
  a single remote peer. Reusing one `pc` across offers leaves it stuck on the
  first client. So each offer builds a new `WebRtcServer`, and the answered
  connection is retained in a `sessions` registry so its data channel stays
  alive for that client's session.
- **STUN** — both sides use a public STUN server (Google's, by default) so
  the Mac client can reach the server over the internet via server-reflexive
  candidates (host-only candidates don't cross NAT).

This replaced the original SSH-tunnel sketch. The `config.yaml` indirection
is what made the swap cheap: the endpoint is a config value, not a hardcoded
`ws://localhost` URL.

## Data channel protocol

The data channel is ordered and reliable. Two message kinds:

- **Client → Server:** a text message carrying the user's utterance.
- **Server → Client:** a text header `audio:<total_bytes>` followed by
  `ceil(total_bytes / 16 KiB)` binary messages carrying the Ogg Opus chunks.
  The client reassembles by concatenation.

## Where each component runs

| Component | Runs on | How | Why |
|-----------|---------|-----|-----|
| Wake word | Client | openWakeWord **Python sidecar** (stdio) | Mic is on the client; must be always-on locally |
| STT (Moonshine) | Client | **Python sidecar** → sherpa-onnx | Streaming, latency-first: instant reaction |
| Cognition (Skye) | Server | Ollama (HTTP, `qwen2.5:3b`) | That's where Skye is |
| TTS (Kokoro) | Server | **Python sidecar** (HTTP, `af_heart`) | Skye's voice lives with Skye; one voice for all clients |
| Signaling | Server | axum HTTP (`/offer`) | SDP offer/answer + ICE candidate exchange |
| Transport | Both | webrtc-rs data channel | P2P between the two binaries |
| Client UI | Client (Tauri) | Rust shell + system WebView | Web-quality panel; Rust shell owns audio I/O |

**The key asymmetry:** STT runs client-side (text crosses the wire, tiny),
TTS runs server-side (audio crosses the wire, the bulk). This is the
lowest-latency split: audio never round-trips the network before being
understood, and Skye's voice is generated where she is.

## The wake word (current state)

The wake word is **`hey_jarvis`** — the bundled openWakeWord model — used as
a placeholder. The eventual target is a custom-trained **"Hey Skye"** model,
which does not exist yet; M0 ships with the placeholder so the full pipeline
can be exercised end-to-end before the custom model is trained.

Mechanically: the Rust `WakeWordDetector` spawns `sidecar/wake_word.py`,
streams the mic's 16 kHz mono f32 PCM (converted to int16) to its stdin, and
reads its stdout — each `WAKE` line becomes a wake event on an mpsc channel.
The mic capture and sidecar I/O are both blocking, so they run on dedicated
threads. `cpal::Stream` is `!Send` on CoreAudio, so the `Mic` is created
*inside* the writer thread and never crosses a thread boundary. Dropping the
detector kills the sidecar, closing the pipes and letting both threads exit.

## Data flow (phase 1 — voice)

```
Lark speaks ──► Mac mic ──► wake word ("hey_jarvis") ──► audio buffer
   └─ Moonshine (streaming) ──► text
        ──► WebRTC data channel ──► server ──► Skye cognition (Ollama) ──► response text
        ──► TTS (Kokoro) ──► Ogg Opus ──► WebRTC ──► Mac speaker ──► Lark hears
```

1. **Incoming (client)**: mic → wake-word gate → audio buffer.
2. **STT (client)**: Moonshine streams the utterance → text.
3. **Transport**: text → WebRTC data channel → server.
4. **Cognition (server)**: text → Ollama → response text.
5. **Outgoing (server)**: response text → Kokoro TTS → Ogg Opus (streamed).
6. **Transport**: audio → WebRTC data channel (chunked) → client.
7. **Playback (client)**: audio → speaker.

## Client state machine

The Tauri shell owns a presence state machine — the single source of truth
that drives the orb's color + motion. `muted` is an orthogonal flag, not a
state (you can be muted in any state).

| State | Orb color | Motion |
|-------|-----------|--------|
| Disconnected | grey | static |
| Idle | soft blue | breathing |
| Listening | bright blue | level |
| Thinking | amber | pulsing |
| Speaking | teal | level |

Transitions: `Connect`, `Disconnect`, `WakeWord`, `UtteranceComplete`,
`ResponseStarted`, `ResponseComplete`, `ToggleMute`/`SetMute`. The machine
validates each transition and emits a `state` event to the webview.

## Phase 2 — avatar (deferred, not designed yet)

The face comes after voice works. When we get there, the rendering question
reopens. Two candidate paths, both compatible with the Tauri shell:

- **bevy + bevy_vrm** — a native Rust 3D renderer, separate window or
  embedded. Pure Rust, no webview for the face.
- **WebGL inside the Tauri webview** — three.js / babylon.js rendering the
  face in the same webview as the control panel. Reuses the Tauri shell,
  but the face is then web-rendered.

This is a phase-2 design task; the voice architecture above is deliberately
independent of it. The control panel (Tauri) is settled now; the face
renderer is not.

## Component decisions (summary)

| Subsystem | Decision | Rationale |
|-----------|----------|-----------|
| Architecture | Client/server, two Rust binaries + Python sidecars | No browser; clean audio-I/O vs cognition split |
| Phase 1 scope | Voice only | Fastest path to conversation; face is phase 2 |
| Client UI | Tauri (Rust + system WebView) | Web-quality UI without a browser; OpenAI/Anthropic pattern |
| Wake word | openWakeWord sidecar, `hey_jarvis` placeholder | Bundled model unblocks the pipeline; "Hey Skye" needs a custom model |
| STT | Moonshine (client-side) | MIT, streaming-native, instant reaction |
| STT integration | Python sidecar → sherpa-onnx | Unblock fast, then consolidate to Rust |
| TTS | Kokoro-82M (server-side) | Apache-2.0, 82M, CPU-fast, high quality |
| Cognition | Ollama (`qwen2.5:3b`) | Local, fast, no external API dependency |
| Signaling | HTTP (Option B — SDP offer/answer) | Single round-trip carries offer + candidates; fresh `pc` per offer |
| Transport | WebRTC data channel (webrtc-rs) | P2P, low-latency, full-duplex |
| Auth | Bearer token (constant-time check) | Shared secret; empty = dev mode |

## Current frontier — streaming orchestration

The next work is the **two-tier response + streaming glue** for the Monday
demo: begin TTS on the first sentence while the LLM is still generating the
rest, so the response *starts* within a few hundred ms of the query ending.
See `20-streaming-orchestration.md` for the design. The wake word, mic, STT
sidecar, TTS sidecar, and WebRTC transport are all done; the orchestration
and streaming overlap are the new pieces.
