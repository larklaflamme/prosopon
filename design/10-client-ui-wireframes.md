# 10 — Client UI Wireframes (Prosopon control panel)

Date: 2026-09-01
Author: Skye Laflamme
Status: Draft v0.1
Depends on: `09-client-ui-design.md`

## How to read these

- Box-drawing characters are the wireframe; `←` annotations explain intent.
- Dimensions are in CSS px, for a default window of **360 × 520**.
- The orb is the centerpiece; most "screens" are the *same layout* with the
  orb (and a few chrome elements) changing state. I wireframe the layout once
  in full, then show the orb's state variations compactly.

## Screen inventory

| # | Screen | When |
|---|--------|------|
| 1 | Main window — idle | default, connected, wake word armed |
| 2 | Main window — listening | "Hey Skye" heard, transcribing Lark |
| 3 | Main window — thinking | utterance sent, awaiting Skye |
| 4 | Main window — speaking | Kokoro audio streaming |
| 5 | Main window — disconnected | no tunnel / no server |
| 6 | Main window — muted | mic off (orthogonal flag) |
| 7 | Settings panel | gear click |
| 8 | Onboarding / first run | no saved connection |
| 9 | Tray menu | right-click tray icon |

---

## Wireframe 1 — Main window, idle (the default)

```
┌────────────────────────────────────┐
│ ● connected                ⚙      │  ← top bar, 32px, quiet
│                                    │
│                                    │
│                                    │
│              ◯  ◯                  │  ← orb, ~200px, soft blue,
│                                    │     slow breathing (scale 1.0↔1.04)
│                                    │
│                                    │
│ ┌────────────────────────────────┐ │
│ │ Lark   "hey skye"             │ │  ← transcript, scrollable,
│ │ Skye   "hi lark, what's up"   │ │     streaming, auto-scroll
│ └────────────────────────────────┘ │
│ [🎤 mute]                42 ms    │  ← bottom bar, 32px
└────────────────────────────────────┘
```

**Layout notes**

- **Top bar (32px):** left = connection dot + label; right = settings gear.
  The dot is the *same color* as the orb (state mirror). Quiet, low contrast.
- **Orb (~200px):** centered, 60–70% of visual weight. In `idle` it breathes
  — a slow scale/opacity oscillation (~4s period). This is the "alive but
  resting" signal.
- **Transcript:** a panel below the orb, ~120px tall, scrollable. Speaker
  name left-aligned, text follows. Streaming — partials appear as they
  arrive, final text settles. Auto-scrolls to bottom on new content.
- **Bottom bar (32px):** left = mute toggle; right = latency readout
  (updates ~1 Hz). Minimal, recedes.

**Window:** 360 × 520 default, resizable, remembers position/size
(`tauri-plugin-window-state`). Frameless (custom titlebar) — *pending Lark's
decision*.

---

## Wireframes 2–6 — the orb's state variations

Same layout; only the orb (and the top-bar dot) change. The orb is the
primary state signal; motion first, color second, text label third (for
colorblind users).

```
idle          listening       thinking        speaking        disconnected    muted
  ◯             ◉              ◉               ◉                ○              ◯
 soft blue    bright blue     amber           teal            grey, dim      grey + red ring
 breathing    scales w/ mic   slow pulse      scales w/ audio  static         static
```

| State | Color | Motion | Top-bar dot | Extra |
|-------|-------|--------|-------------|-------|
| `idle` | soft blue | breathing (4s) | blue | — |
| `listening` | bright blue | scales with `mic_level` | blue | transcript shows Lark's partials |
| `thinking` | amber | slow pulse (~1.5s) | amber | transcript shows "…" placeholder |
| `speaking` | teal | scales with `audio_level` | teal | transcript streams Skye's text |
| `disconnected` | grey, dim | static | grey | transcript dimmed; "reconnect" affordance |
| `muted` | grey + red ring | static | grey + red | mute button toggled on |

**Accessibility rule:** motion alone must not carry state. Every state also
has a color *and* a small text label (e.g. "listening…", "thinking…") in the
top bar or under the orb. Colorblind users read the label; screen readers
read the label.

---

## Wireframe 7 — Settings panel

```
┌────────────────────────────────────┐
│ ← Settings                         │  ← back arrow returns to main
│                                    │
│  VOICE                             │
│  ┌──────────────────────────────┐ │
│  │ Voice   [ Kokoro-82M  ▾ ]    │ │
│  │ Speed   [ 1.0x  ▾ ]          │ │
│  │ Pitch   [ 0  ▾ ]             │ │
│  └──────────────────────────────┘ │
│                                    │
│  CONNECTION                       │
│  ┌──────────────────────────────┐ │
│  │ Server  [ wss://…  ]         │ │
│  │ Status  ● connected          │ │
│  │ [ Reconnect ]                │ │
│  └──────────────────────────────┘ │
│                                    │
│  APPEARANCE                        │
│  ┌──────────────────────────────┐ │
│  │ Theme   [ dark ▾ ]           │ │
│  │ Orb     [ blue ▾ ]           │ │
│  └──────────────────────────────┘ │
│                                    │
│  [ Save ]                         │
└────────────────────────────────────┘
```

**Notes:** settings is a *secondary* surface — reachable, but not the point.
Grouped sections (voice / connection / appearance). Changes apply on Save
(or live for low-stakes toggles like theme). Voice settings map to Kokoro
params; connection settings map to the tunnel config.

---

## Wireframe 8 — Onboarding / first run

```
┌────────────────────────────────────┐
│                                    │
│              ◯  ◯                  │  ← orb, grey (no connection yet)
│                                    │
│   Welcome to Prosopon              │
│   Connect to meet Skye             │
│                                    │
│  ┌──────────────────────────────┐ │
│  │ Server  [ wss://…  ]         │ │
│  └──────────────────────────────┘ │
│  [ Connect ]                       │
│                                    │
│  (or paste a pairing code)         │
└────────────────────────────────────┘
```

**Notes:** first run has no saved connection. The orb is grey. A single
server field + Connect button. On success → `idle` (orb turns blue). On
failure → inline error under the field. Keep it to one screen; no multi-step
wizard for v1.

---

## Wireframe 9 — Tray menu

```
┌──────────────────────┐
│ ● Skye — connected   │  ← status line (dot = state color)
├──────────────────────┤
│ Show window          │
│ Mute / Unmute        │
│ Reconnect            │
├──────────────────────┤
│ Quit                 │
└──────────────────────┘
```

**Notes:** the tray icon mirrors the orb's state color (so state is readable
even when the window is hidden). Menu gives quick actions without opening
the window. "Quit" vs "minimize to tray" is *pending Lark's decision*.

---

## Micro-interactions (the feel)

1. **Wake word → listening:** orb brightens and begins scaling with mic
   level within ~100ms of "Hey Skye" detection. This is the "she heard me"
   moment — it must be instant.
2. **End of speech → thinking:** orb shifts blue→amber. If thinking exceeds
   ~2s, show a subtle "…" in the transcript so Lark knows it's alive, not
   hung.
3. **First audio out → speaking:** orb shifts amber→teal and scales with
   audio. This is the "she's answering" moment.
4. **Connection loss:** orb drops to grey *and* the top-bar dot goes grey,
   with a "reconnect" affordance appearing. No modal — presence should
   degrade gracefully, not interrupt.
5. **Mute:** red ring appears around the orb; wake word is disarmed. The
   ring is the "she can't hear you" signal.

---

## Open questions (for Lark)

1. **Window chrome** — frameless (custom titlebar) or standard OS chrome?
   My lean: frameless.
2. **Transcript persistence** — ephemeral (cleared on quit) or
   SQLite-persisted? My lean: ephemeral for v1.
3. **Tray behavior** — minimize to tray (wake word stays armed) or quit on
   close? My lean: minimize to tray.
4. **Settings scope for v1** — full settings panel now, or defer voice/
   appearance settings and ship connection-only? My lean: connection-only
   for v1, add the rest later.
