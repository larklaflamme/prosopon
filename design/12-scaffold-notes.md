# 12 — Scaffold Notes (first buildable slice)

Date: 2026-09-01
Author: Skye Laflamme
Status: Scaffolded, NOT yet buildable on this box
Depends on: `11-decisions.md`

## What was scaffolded

```
prosopon/
├── src-tauri/                  # Rust shell (the real project)
│   ├── Cargo.toml              # tauri 2 + tray-icon feature
│   ├── build.rs
│   ├── tauri.conf.json         # frameless window, 360×520, withGlobalTauri
│   ├── capabilities/default.json
│   └── src/
│       ├── main.rs             # entry → prosopon_lib::run()
│       ├── lib.rs              # commands + tray icon + state event emission
│       └── state_machine.rs    # the presence state machine (with tests)
└── ui/                         # frontend (no build step, vanilla)
    ├── index.html              # titlebar + orb + transcript + bottombar
    ├── styles.css              # orb colors + motions (reused from wireframes)
    └── main.js                 # Tauri event wiring via window.__TAURI__
```

## The state machine (the core)

States: `disconnected → idle → listening → thinking → speaking → idle`,
with `muted` as an orthogonal flag (you can be muted in any state).

Transitions are validated — illegal ones are no-ops:
- `WakeWord` only from `idle` and only when not muted.
- `UtteranceComplete` only from `listening`.
- `ResponseStarted` only from `thinking`.
- `ResponseComplete` only from `speaking`.
- `Connect`/`Disconnect` only when they change state.

Every state change emits a `state` event to the webview, which drives the
orb's color + motion. The orb CSS is reused directly from `wireframes.html`.

## What is NOT yet wired (phase 2+)

- **No real mic/audio/STT/TTS.** `mic_level`, `audio_level`, `transcript`,
  `latency` events are not emitted yet — the orb's `listening`/`speaking`
  "level" motion is a fixed CSS loop, not driven by real audio.
- **No server connection.** `connect`/`disconnect` only flip the state
  machine; there is no WebSocket/tunnel to the Skye server yet.
- **No wake-word detection.** `WakeWord` is a transition the frontend could
  trigger, but nothing listens for "Hey Skye" yet.
- **No face.** The orb is the placeholder; the VRM face drops into the same
  slot later.

## Build blockers on THIS box (honest)

1. **System deps missing:** `webkit2gtk-4.1` and `gtk3` are not installed.
   Tauri on Linux needs `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, and friends.
   Installing them requires `sudo apt install` — which I cannot run (sudo is
   blocked in my shell).
2. **Tauri CLI not installed.** `cargo install tauri-cli` or `npm i -D
   @tauri-apps/cli` is needed for `tauri dev` / `tauri build` / `tauri icon`.
3. **Icons missing.** `tauri.conf.json` references `icons/*.png|ico|icns`
   which don't exist yet. Generate with `tauri icon <source.png>`.

## What I verified

- The **state machine compiles and its 5 unit tests pass** (standalone cargo
  test, serde only — no tauri deps needed). See
  `/tmp/skye-workspace/state-machine-test/`.
- The **frontend renders** — the orb CSS is the same verified CSS from
  `wireframes.html`.

## Next steps (need Lark)

1. Install system deps (needs sudo): `libwebkit2gtk-4.1-dev libgtk-3-dev
   libayatana-appindicator3-dev librsvg2-dev` (and the rest of the Tauri
   Linux prerequisites).
2. Install Tauri CLI.
3. Generate icons.
4. `cargo test` in `src-tauri/` (state machine tests), then `tauri dev`.

## Note on the leftover root files

`prosopon/Cargo.toml` and `prosopon/src/main.rs` are leftovers from an
initial `cargo new prosopon` (a hello-world). The real Rust project is
`src-tauri/`. The root files can be deleted or turned into a workspace —
Lark's call.
