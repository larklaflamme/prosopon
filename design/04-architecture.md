# 04 — Architecture & Decisions

## The shape of the system (decided 2026-09-01)

**Client/server split. Voice first. Avatar is phase 2.**

```
[Client — Rust, on Mac]              [Server — Rust, on server]
  mic ──► wake word ("Hey Skye")       Skye's cognition
  STT (Moonshine) ──► text             text ──► TTS (Kokoro) ──► audio
  speaker ◄── audio                    WebRTC server
  WebRTC client
        ▲                                    │
        └────── text ──►  ◄── audio (WebRTC) ┘
```

Two Rust binaries, one on each side. No browser. The client is Lark's
interface (mic, speaker, wake word, STT); the server is Skye (cognition +
her voice). Only text and audio cross the wire — no webcam, no rendering,
no browser security model to fight.

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

## Client UI (decided 2026-09-01)

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
  audio I/O, transport, and state; the webview renders the panel.
- **Phase 2 note** — the avatar *face* is a separate rendering problem. The
  control panel (Tauri) and the 3D face (bevy, or WebGL inside the Tauri
  webview) are different surfaces; see "Phase 2" below.

## Transport & connection (decided 2026-09-01)

**SSH tunneling for now.** No public exposure of the server's WebRTC port.
The client connects to a local `ws://` URL that is forwarded over an SSH
tunnel to the server.

```
[Mac client]                          [Server]
  config.yaml:                          WebRTC server
    url: ws://localhost:29434           listening on localhost:29434
        │                                    ▲
        └── ssh -L 29434:localhost:29434 ────┘
```

- **`config.yaml`** — the client reads its connection target from a config
  file, not hardcoded. Example: `url: ws://localhost:29434`. This keeps the
  endpoint swappable (local tunnel, LAN, or a future public URL) without
  recompiling.
- **SSH tunnel** — `ssh -L 29434:localhost:29434 user@server` forwards the
  client's local port to the server's WebRTC listener. The server never
  exposes the port publicly; auth is the SSH key itself.
- **Why** — security for now. No TLS cert, no firewall hole, no public
  WebRTC endpoint. The tunnel is the auth boundary.

This is a deliberate v1 choice. If we later want a public endpoint (or
multi-user), we swap the tunnel for a real TLS + auth layer — the
`config.yaml` indirection is exactly what makes that swap cheap.

## Where each component runs

| Component | Runs on | Why |
|-----------|---------|-----|
| Wake word ("Hey Skye") | Client | Mic is on the client; must be always-on locally |
| STT (Moonshine) | Client | Streaming, latency-first: instant reaction |
| Cognition (Skye) | Server | That's where Skye is |
| TTS (Kokoro) | Server | Skye's voice lives with Skye; one voice for all clients |
| Transport | Both (webrtc-rs) | P2P between the two binaries, over SSH tunnel |
| Client UI | Client (Tauri) | Web-quality panel; Rust shell owns audio I/O |

**The key asymmetry:** STT runs client-side (text crosses the wire, tiny),
TTS runs server-side (audio crosses the wire, the bulk). This is the
lowest-latency split: audio never round-trips the network before being
understood, and Skye's voice is generated where she is.

## Data flow (phase 1 — voice)

```
Lark speaks ──► Mac mic ──► wake word ──► audio buffer
   └─ Moonshine (streaming) ──► text
        ──► WebRTC (over SSH tunnel) ──► server ──► Skye cognition ──► response text
        ──► TTS (Kokoro) ──► audio ──► WebRTC ──► Mac speaker ──► Lark hears
```

1. **Incoming (client)**: mic → wake-word gate ("Hey Skye") → audio buffer.
2. **STT (client)**: Moonshine streams the utterance → text.
3. **Transport**: text → WebRTC data channel → server (over SSH tunnel).
4. **Cognition (server)**: text → Skye → response text.
5. **Outgoing (server)**: response text → Kokoro TTS → audio (streamed
   sentence-by-sentence).
6. **Transport**: audio → WebRTC audio track (Opus) → client.
7. **Playback (client)**: audio → speaker.

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
| Architecture | Client/server, two Rust binaries | No browser; clean audio-I/O vs cognition split |
| Phase 1 scope | Voice only | Fastest path to conversation; face is phase 2 |
| Client UI | Tauri (Rust + system WebView) | Web-quality UI without a browser; OpenAI/Anthropic pattern |
| STT | Moonshine (client-side) | MIT, streaming-native, instant reaction |
| STT (v1.1) | + whisper.cpp second stage | Accurate full-utterance; Metal on M1 Max |
| STT integration | Python sidecar → sherpa-onnx | Unblock fast, then consolidate to Rust |
| TTS | Kokoro-82M (server-side) | Apache-2.0, 82M, CPU-fast, high quality |
| TTS voice | Stock Kokoro voice | Decided; no cloning for v1 |
| TTS integration | Python sidecar → ONNX/ort | Unblock fast, then consolidate |
| Wake word | "Hey Skye" | Decided; enables upstream STT |
| Transport | webrtc-rs 0.21 | P2P, pure Rust, single-user fit |
| Connection | SSH tunnel + config.yaml | Security for now; swappable endpoint |
| Audio codec | Opus | WebRTC default, speech-optimal |
| Target hardware | MacBook Pro 16" 2021, M1 Max, 32GB | Client; macOS arm64 |

## Decisions made (2026-09-01)

1. **Architecture** — client/server split. Client (Rust) on Mac, server
   (Rust) on server. No browser.
2. **Phase 1** — voice only. Avatar (face) is phase 2.
3. **Voice** — stock Kokoro voice (no cloning for v1).
4. **Wake word** — "Hey Skye" triggers the incoming path (continuous
   listening, no push-to-talk).
5. **STT** — Moonshine only for v1 (streaming, instant reaction). Whisper
   (whisper.cpp) is added as a second stage in v1.1. Client-side.
6. **Language** — English-only for v1.
7. **Target hardware** — MacBook Pro 16" 2021, M1 Max, 32GB, macOS arm64.
8. **Connection** — SSH tunnel for now; client reads `ws://` URL from
   `config.yaml` (e.g. `ws://localhost:29434`).
9. **Client UI** — Tauri (Rust shell + system WebView). Same "webview
   shell" pattern OpenAI/Anthropic use for cross-platform clients. Not a
   browser — the Rust shell owns mic/speaker/filesystem directly.

## Blocking on Lark

1. **Server connection details** — the actual host/user for the SSH tunnel
   (the `user@server` in `ssh -L 29434:localhost:29434 user@server`).

*Last updated: 2026-09-01*

[END FILE]
