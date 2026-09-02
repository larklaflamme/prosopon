# 09 — Client UI Design (Prosopon control panel)

Date: 2026-09-01
Author: Skye Laflamme
Status: Draft v0.1

## Scope

This doc designs the **phase-1 control panel** — the Tauri webview UI Lark
sees on the Mac. It is *not* the avatar face (phase 2), but it is designed so
the face drops into the same slot later.

## Design principles

1. **Voice-first, not chat-first.** The primary modality is speech. The UI is
   ambient and glanceable — you should be able to read the state from across
   the room. The transcript is a *record*, not the *interface*.
2. **Presence over tool.** This is not a ChatGPT clone. The client is the
   surface where Lark meets Skye. The central element should feel like a
   *presence*, not a widget.
3. **One central element.** A single "presence orb" carries the entire state.
   Everything else (transcript, controls) is secondary and recedes.
4. **The orb is the face's placeholder.** Phase 1 renders the orb as a light;
   phase 2 replaces it with the VRM face in the *same slot*, driven by the
   *same state machine*. The transition is a rendering swap, not a redesign.

## The central element: the presence orb

A single orb, centered, large. Its color + motion encodes the state:

| State | Orb | Meaning |
|-------|-----|---------|
| `disconnected` | Grey, dim, static | No tunnel / no server |
| `idle` | Soft blue, slow breathing pulse | Wake word armed, waiting for "Hey Skye" |
| `listening` | Bright blue, scales with mic level | "Hey Skye" heard; transcribing Lark |
| `thinking` | Amber, slow pulse | Skye processing |
| `speaking` | Teal/green, scales with audio level | Skye's voice playing |
| `muted` | Grey + red ring | Mic off |

**Color language:** blue = Skye (present, listening); amber = thinking
(transitional); teal = speaking (alive); grey = absent (disconnected/muted).
Blue is Skye's identity color (raven, bioluminescent blue).

**Motion:** the orb should *breathe* (slow scale/opacity oscillation) in
`idle`, *pulse* in `thinking`, and *respond* (scale with level) in
`listening`/`speaking`. Motion is the primary state signal — color is the
backup (for colorblind users, motion + a small text label must also carry
the state).

## Layout

```
┌─────────────────────────────────┐
│  ● connected        [⚙ settings] │  ← top bar (thin, 32px)
│                                 │
│                                 │
│           ( presence orb )       │  ← center, dominant
│                                 │
│                                 │
│  ┌───────────────────────────┐  │
│  │ Lark  "hey skye..."       │  │  ← transcript (scrollable, secondary)
│  │ Skye  "hi lark..."        │  │
│  └───────────────────────────┘  │
│  [🎤 mute]        42ms latency  │  ← bottom bar (thin)
└─────────────────────────────────┘
```

- **Top bar:** connection status (dot + label) + settings gear. Thin, quiet.
- **Center:** the orb. This is 60-70% of the window's visual weight.
- **Transcript:** a scrollable panel below the orb. Streaming — words appear
  as they're transcribed (Lark) or synthesized (Skye). Auto-scrolls.
- **Bottom bar:** mute toggle + latency readout. Minimal.

**Window:** small by default (~360×520), resizable, remembers position/size
(`tauri-plugin-window-state`). It should feel like a compact presence widget,
not a full app window. A system-tray icon (`tauri-plugin-tray`) keeps it
alive in the background; the tray icon mirrors the orb's state color.

## UI states (the state machine)

The Rust shell owns a single state machine. The webview is a pure renderer of
it. Six states:

```
disconnected ⇄ idle ⇄ listening → thinking → speaking → idle
                 ↑                    │
                 └──── muted ─────────┘
```

- `disconnected` — no WebRTC peer. Entered on startup and on connection loss.
- `idle` — connected, wake word armed. The resting state.
- `listening` — "Hey Skye" detected; Moonshine streaming Lark's speech.
- `thinking` — utterance sent; awaiting Skye's response text.
- `speaking` — Kokoro audio streaming to the speaker.
- `muted` — mic off (orthogonal to the above; can be entered from any state).

`muted` is orthogonal — it's a flag, not a stage. The state machine is
`disconnected → idle → listening → thinking → speaking → idle`, with `muted`
as a modifier.

## Tauri surface (the contract)

### Commands (webview → Rust, via `invoke`)

| Command | Args | Returns | Purpose |
|---------|------|---------|---------|
| `get_state` | — | `State` | Initial snapshot on load |
| `set_muted` | `muted: bool` | `State` | Toggle mic |
| `disconnect` | — | `State` | Drop the WebRTC peer |
| `reconnect` | — | `State` | Re-establish the tunnel connection |

### Events (Rust → webview, via `emit`)

| Event | Payload | Rate | Purpose |
|-------|---------|------|---------|
| `state` | `{ state: StateName }` | on change | Drive the orb |
| `mic_level` | `{ level: f32 }` | ~20 Hz, throttled | Orb scale in `listening` |
| `audio_level` | `{ level: f32 }` | ~20 Hz, throttled | Orb scale in `speaking` |
| `transcript` | `{ speaker: "lark"\|"skye", text: String, final: bool }` | streaming | Live transcript |
| `latency` | `{ ms: u32 }` | ~1 Hz | Bottom-bar readout |

`StateName = "disconnected" | "idle" | "listening" | "thinking" | "speaking" | "muted"`

**Throttling matters.** `mic_level` and `audio_level` are high-frequency; the
Rust shell must throttle them (~20 Hz) before emitting, or the webview will
churn. `transcript` streams as partials arrive (Moonshine partials, Kokoro
sentence chunks).

## Frontend framework

**Svelte 5 + TypeScript.** Rationale:

- The panel is thin and highly reactive (mic level, streaming transcript).
  Svelte's compiled reactivity handles high-frequency updates with minimal
  overhead and boilerplate.
- Small bundle — matters for a lightweight client.
- Phase 2 (three.js + three-vrm in the webview) works fine with Svelte.

Alternatives: **Solid** (similar reactivity, even smaller) is a fine choice;
**React** is the ecosystem default but heavier than this panel needs. The
choice is low-stakes — the contract above is framework-agnostic.

## Phase 2 transition (orb → face)

The orb's slot, state machine, and event surface are all reused. Phase 2
replaces the orb's *renderer* with the VRM face:

| State | Orb (phase 1) | Face (phase 2) |
|-------|---------------|----------------|
| `idle` | blue breathing | relaxed, occasional blink, wandering gaze |
| `listening` | blue, mic-scaled | attentive, gaze at camera |
| `thinking` | amber pulse | eyes up/away, slight brow furrow |
| `speaking` | teal, audio-scaled | lip sync (visemes), expressive |

The `state` event already carries everything the face needs; phase 2 adds a
`blendshapes` event (ARKit 52-shape vector) alongside `audio_level` for lip
sync. The layout, state machine, and Tauri shell are unchanged.

## Open questions for Lark

1. **Window chrome** — frameless (custom titlebar, more "presence widget") or
   standard OS chrome (simpler, more native)? My lean: frameless for the
   presence feel, but it's more work.
2. **Transcript persistence** — should the transcript persist across sessions
   (SQLite, per chatgpt_ui.md's recommendation) or be ephemeral (cleared on
   quit)? My lean: ephemeral for v1; persistence is a later feature.
3. **Tray behavior** — should the app minimize to tray (orb stays alive in the
   menu bar) or quit on close? My lean: minimize to tray, since the wake word
   should stay armed.
