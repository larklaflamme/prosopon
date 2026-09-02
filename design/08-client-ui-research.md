# 08 — Client UI Research

## Question

Which Rust UI library should the Prosopon client use for its control panel
(connection status, mic level, transcript, mute button)?

## The two rendering problems (don't conflate)

1. **Control panel** (phase 1, now) — thin, mostly text, needs to feel
   responsive.
2. **Avatar face** (phase 2, later) — a 3D model with blendshapes, lip sync,
   gaze. That's a *renderer*, not a UI library.

The library chosen now is for #1. #2 is a separate decision (bevy, or WebGL
inside the Tauri webview) made when we get there.

## Decision: Tauri

**Tauri** — Rust shell + the *system* WebView (WKWebView on macOS).

### Why Tauri

- **Web-quality UI without a browser.** Tauri embeds the system WebView in a
  native app; there is no browser security model to fight (no autoplay
  policy, no sandboxed mic permissions, no cross-origin rules). The Rust
  shell owns the mic, speaker, and filesystem directly.
- **Less effort than egui.** egui (pure Rust, immediate mode) hand-builds
  every widget and won't look polished without real work. Tauri gives a
  familiar, rich UI for far less.

### The tradeoff

The UI is HTML/CSS/JS, not Rust. The Rust shell handles audio I/O,
transport, and state; the webview renders the panel.

## The OpenAI/Anthropic finding — CORRECTED 2026-09-01

> **STATUS: verified against live sources 2026-09-01 (post proxy fix).**
> The earlier "both are Tauri" claim was WRONG. Corrected below.

**What's actually true (verified):**

- **Claude desktop app is Electron, NOT Tauri.** Confirmed by multiple
  sources, including a primary-source quote from Boris Cherney of the
  Claude Code team (via dbreunig.com, "Why is Claude an Electron App?",
  Feb 2026): *"Some of the engineers working on the app worked on Electron
  back in the day, so preferred building non-natively. It's also a nice way
  to share code so we're guaranteed that features across web and desktop
  have the same look and feel."* Also corroborated by theagenttimes.com
  ("Electron Wins Again: Claude's Desktop App...") and ubos.tech.
- **ChatGPT macOS desktop app: NOT confirmed as Tauri.** No live source
  found claiming the official app is Tauri. Search results return only
  *third-party* Tauri wrappers (litongjava/tauri-chatgpt,
  sonnylazuardi/chat-ai-desktop, etc.) — these are community reimplementations,
  not OpenAI's official app. The official app's framework remains unverified;
  do not assert Tauri for it.

**The pattern that survives correction (framework-agnostic):**

The "same look and feel" across platforms is achieved by shipping the **same
web frontend** (the React/HTML/CSS codebase powering chatgpt.com and
claude.ai) inside a **thin native shell**. The shell framework differs —
Claude uses Electron (Chromium), not Tauri (system WebView) — but the
*strategy* is identical: one web codebase, wrapped per-platform.

**Implication for Prosopon:** the webview-shell strategy is validated, but
the specific "Tauri is what the big labs use" justification is false. Tauri
remains a sound choice for *our* reasons (Rust shell owns audio/transport,
system WebView, no browser security model) — not because OpenAI/Anthropic
use it. They don't (Claude = Electron; ChatGPT = unverified, likely native).

## Alternatives considered (and rejected)

| Library | Verdict | Why |
|---------|---------|-----|
| **egui/eframe** | Rejected (was earlier candidate) | Pure Rust, no webview, but hand-builds every widget; won't look polished without real effort |
| **iced** | Rejected | Elm-style, retained mode; more boilerplate for a thin panel |
| **Slint** | Rejected | Declarative, native-ish, but GPL/paid licensing tax |
| **Native Cocoa** (`cacao`, `objc2`) | Rejected | Most Mac-native, but most work; locks to macOS |
| **Druid** | Rejected | Development stalled (medium confidence) |

## Open item

- ~~Verify the Tauri claim~~ — DONE 2026-09-01. Claude = Electron (confirmed);
  ChatGPT = unverified, no Tauri evidence. See corrected finding above.

*Last updated: 2026-09-01*
