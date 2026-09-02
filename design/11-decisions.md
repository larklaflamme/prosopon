# 11 — Design Decisions (locked)

Date: 2026-09-01
Author: Skye Laflamme
Status: Decided (Lark, 2026-09-01)
Depends on: `09-client-ui-design.md`, `10-client-ui-wireframes.md`

## The four blocking questions — resolved

| # | Question | Decision | Notes |
|---|----------|----------|-------|
| 1 | Window chrome | **Frameless** | Custom titlebar; presence-widget feel. Drag region + window controls drawn in-app. |
| 2 | Transcript persistence | **Ephemeral** | Cleared on quit. No SQLite for v1. |
| 3 | Tray behavior | **Minimize to tray** | Wake word stays armed while hidden. Quit is explicit (tray menu). |
| 4 | Settings scope (v1) | **Connection-only** | Server URL + pairing code. Voice/appearance deferred. |

## Consequences for the buildable slice

- **Frameless** → Tauri `decorations: false`; need a custom drag region (`data-tauri-drag-region`) and min/close buttons wired to Tauri commands. On macOS, `titleBarStyle: Overlay` is the cleaner path; on Linux/Windows, full custom.
- **Ephemeral transcript** → no persistence layer in v1. Transcript lives in frontend state only. Simplifies the slice: no DB, no schema.
- **Minimize to tray** → `tauri-plugin-tray-icon` (or `tray-icon` crate) + a `close` handler that hides the window instead of exiting. Wake-word loop must run in the Rust side, not the webview, so it survives window hide.
- **Connection-only settings** → settings panel is a single field (server URL) + Connect button + inline error. Matches Wireframe 8 (onboarding) almost exactly — onboarding and settings collapse into one surface for v1.

## What this means for the first slice

The first buildable slice is now well-bounded:

1. **Rust shell** — frameless window, tray icon, minimize-to-tray, wake-word armed in background.
2. **State machine** — `disconnected → idle → listening → thinking → speaking → idle`, with `muted` as an orthogonal flag.
3. **Orb** — the CSS orb from `wireframes.html`, driven by the state machine via Tauri events.
4. **Connection-only settings** — server URL field + Connect, inline error on failure.

No transcript persistence, no voice/appearance settings, no face. Those are phase 2+.
