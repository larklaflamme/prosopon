# 19 — Client Setup Guide (Mac + Ubuntu → End-to-End Test)

Date: 2026-09-10 (revised)
Author: Skye Laflamme
Status: verified against the actual repo state (client-core, src-tauri, configs, icons)
Depends on: `13-install-guide.md`, `17-client-implementation.md`

## Read this first — which machine am I on?

This guide covers **two machines**. Every command block below is tagged with
`# ON: <machine>` so you never have to guess where to run it.

| Machine    | Hostname                             | Role                   | What runs here                                                           |
| ---------- | ------------------------------------ | ---------------------- | ------------------------------------------------------------------------ |
| **Mac**    | your M1 Max MacBook                  | the *client*           | Tauri shell, `client-core`, Moonshine (STT), openWakeWord, mic + speaker |
| **Ubuntu** | `g3-h100-small-dal-1` (64.34.82.229) | the *server* + dev box | `prosopon-server`, Kokoro (TTS), Ollama (cognition), the loopback test   |

The goal, in order:
1. **Mac** — build and run the real client (Tauri shell + transport + audio).
2. **Ubuntu** — run the loopback test to prove client↔server WebRTC interop.
3. **End-to-end** — Mac client connects to the live server, first real turn.

---

## The honest state (what's already done vs. pending)

| Piece                                  | Status                                    |
| -------------------------------------- | ----------------------------------------- |
| Server (WebRTC + signaling + pipeline) | ✅ built, 13/13 tests, live-tested         |
| `client-core` (transport)              | ✅ built, 3/3 tests + loopback 1/1         |
| App icons                              | ✅ generated (`src-tauri/icons/` complete) |
| Tauri shell (state machine + orb)      | ✅ compiled (state machine + orb + wake-word command) |
| Audio playback (Ogg Opus → speaker)    | ❌ not built                               |
| STT (Moonshine)                        | ❌ not built                               |
| Wake word (openWakeWord)               | ✅ built (sidecar + bridge) — untested on real mic |
| Mic capture (cpal)                     | ✅ built (16 kHz mono f32) — untested on real mic |

The *transport* is proven. The *GUI shell* and the *audio I/O* are the
remaining work, and they can only be finished on the Mac.

---

## Part 0 — Shared prerequisites (both machines)

`client-core` is **platform-agnostic** (pure Rust, no GUI, no audio). It
compiles identically on Mac and Ubuntu. The only shared prerequisite is the
Rust toolchain.

### 0.1 Rust toolchain — `# ON: Mac` and `# ON: Ubuntu` (identical)

```bash
# ON: Mac AND Ubuntu (run once on each)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustc --version   # confirm: expect 1.98.0 or newer
```

### 0.2 Xcode Command Line Tools — `# ON: Mac` only

macOS needs the CLT for the C toolchain (Tauri + `cpal` link against system
frameworks):

```bash
# ON: Mac
xcode-select --install
```

Ubuntu needs nothing extra here — `build-essential` is already present.

---

## Part 1 — Shared: clone + build `client-core` (both machines)

This is the part that is **identical on Mac and Ubuntu**. Do it on whichever
machine you're working on.

### 1.1 Clone the repo

```bash
# ON: Mac AND Ubuntu (identical)
git clone git@github.com:larklaflamme/prosopon.git ~/prosopon
cd ~/prosopon
```

> On the Mac, the repo already lives at `/Users/ki11erc0der/Workspace/prosopon`.
> Use that path instead of `~/prosopon` if you prefer — the steps are the same.

### 1.2 Build + test `client-core`

```bash
# ON: Mac AND Ubuntu (identical)
cd ~/prosopon/client-core
cargo build
cargo test          # expect 3/3 pass
```

This proves the transport crate compiles on your platform. It is the
**shared** foundation — everything below it is platform-specific.

---

## Part 2 — Mac-specific: the Tauri shell + audio sidecars

Everything in this part runs **only on the Mac**. It cannot run on the Ubuntu
box (no `webkit2gtk`, no mic/speaker).

### 2.1 Tauri CLI

```bash
# ON: Mac
cargo install tauri-cli
```

### 2.2 Build the Tauri shell (first compile)

```bash
# ON: Mac
cd ~/prosopon
cargo tauri dev
```

This is the first real compile of `src-tauri/`. Expect it to surface the
integration errors that couldn't be caught on the headless box — the
`connect_webrtc` / `send_text` commands in `src-tauri/src/lib.rs` are written
but unverified.

### 2.3 Icons — already done ✅

`src-tauri/icons/` is complete (generated 2026-09-09): `32x32.png`,
`128x128.png`, `128x128@2x.png`, `icon.png`, `icon.ico`, `icon.icns`, plus
iOS/Android/Store assets. **No action needed.** If you ever regenerate from a
new source image:

```bash
# ON: Mac (or Ubuntu — the CLI is cross-platform)
cd ~/prosopon
npx tauri icon src-tauri/icons/app-icon.png
```

### 2.4 Python venv for the audio sidecars — `# ON: Mac`

Moonshine (STT) and openWakeWord (wake word) are Python sidecars that run
on-device. **Pin Python 3.11** — both are ONNX Runtime stacks, and 3.11 is the
version with full prebuilt-wheel coverage for macOS arm64 (3.13 risks a source
build of `onnxruntime`).

```bash
# ON: Mac
# ensure 3.11 is installed (Homebrew): brew install python@3.11
mkdir -p ~/Workspace/prosopon/client && cd ~/Workspace/prosopon/client
python3.11 -m venv .venv
source .venv/bin/activate
pip install --upgrade pip
```

> Use `python3.11` explicitly, not bare `python3` — on macOS `python3` may
> resolve to whatever Homebrew last installed (possibly 3.13).

### 2.5 STT + wake word packages

```bash
# ON: Mac (inside the activated venv)
pip install moonshine        # streaming STT, MIT — confirm against usefulsensors/moonshine README
pip install openwakeword     # wake word, MIT — confirm against dscripka/openWakeWord README
```

### 2.6 Models

```bash
# ON: Mac
mkdir -p ~/prosopon/client/models
# Moonshine base model + openWakeWord "hey skye" model.
# Exact URLs from the respective project READMEs.
```

### 2.7 Client config

Create `~/prosopon/client/config.yaml`:

```yaml
signaling:
  url: "https://ac1.ravennest.science:29435/offer"   # the server's HTTPS signaling endpoint
  auth_token: "" # must match the server's signaling.auth_token: openssl rand -hex 32
  # auth_token must match server/src/signaling.rs:57
webrtc:
  stun_servers:
    - "stun:stun.l.google.com:19302"
wake_word:
  python: "client/.venv/bin/python"   # the venv Python (has openwakeword)
  model: "hey_jarvis"                 # placeholder; "hey skye" model comes later
  threshold: 0.5
  sidecar_path: "../sidecar/wake_word.py"   # relative to src-tauri/ (cargo tauri dev cwd)
```

---

### 2.8 Mic permission (macOS TCC) — `# ON: Mac`

Mic capture is now in `client-core` (`mic.rs`, cpal, 16 kHz mono f32). Two
things gate actual mic access on macOS:

1. **`NSMicrophoneUsageDescription`** — already in `src-tauri/Info.plist`
   (referenced by `bundle.macOS.infoPlist` in `tauri.conf.json`). Without it
   the app hard-crashes on first mic access with a TCC violation. **No action
   needed** — it's in the repo.

2. **The user grant** — the first time the app touches the mic, macOS shows a
   permission prompt. Grant it. If you miss or deny it, re-enable in:

   ```
   System Settings → Privacy & Security → Microphone → enable "Prosopon"
   ```

**Dev-mode gotcha (important):** under `cargo tauri dev` the app runs as a
plain binary, not a signed `.app` bundle, so macOS may attribute the mic
permission to the *terminal* that launched it (Terminal.app / iTerm2) rather
than to "Prosopon". If the prompt never appears or capture returns silence,
check that the terminal itself has Microphone access in the same pane. A
proper `cargo tauri build` (signed `.app`) avoids this entirely.

**Smoke test:** invoke the `record_mic(seconds)` Tauri command — it captures
N seconds and returns the sample count + RMS level. A non-zero RMS means the
mic is open and delivering samples.

---

### 2.9 Wake word (openWakeWord) — `# ON: Mac`

The wake-word detector is now built: `client-core/src/wake_word.rs` spawns the
Python sidecar (`sidecar/wake_word.py`), streams the mic to it, and flips the
state machine to `Listening` on each detection.

**Manual setup (three things):**

1. **Install openWakeWord** (already in §2.5):
   ```bash
   # ON: Mac (inside the activated venv)
   pip install openwakeword
   ```

2. **The model.** openWakeWord ships bundled models (`hey_jarvis`, `alexa`,
   …) but **no "Hey Skye" model**. For M0, use a placeholder (e.g.
   `hey_jarvis`) to prove the pipeline; a custom "hey skye" model is trained
   later. The model name is set in `config.yaml` (see §2.7).

3. **The Python interpreter.** The detector runs `wake_word.python` (from
   `config.yaml`) — it must be the **venv's** Python (which has
   `openwakeword`), not the system `python3`. Set it to
   `client/.venv/bin/python` (see §2.7).

**Smoke test:** invoke the `start_wake_word` Tauri command. It spawns the
sidecar, opens the mic, and logs `[prosopon] start_wake_word: listening`.
Say the wake phrase; the orb should flip to `Listening` (bright blue). If the
sidecar fails to spawn, check that `wake_word.python` points at the venv and
that `openwakeword` is importable there.

---


### 2.10 Wake-word debugging — what to look for (learned the hard way)

This section exists because the wake word *looked* broken when it was actually
working. The failure was in knowing **which signal to read**, not in the code.
Keep this next to you when the wake word misbehaves.

**The two diagnostic tools now in the code:**

1. **Mic level meter** (in `client-core/src/wake_word.rs`, writer thread) —
   logs once per second, in the app’s own process:
   ```
   [prosopon] mic level: rms=0.021954 peak=0.231874 samples=16043
   ```
   This is the *only* ground truth for “is the app actually hearing audio?”
   It runs inside the app, so it reflects the app’s TCC context — not your
   SSH session’s.

2. **Per-frame score logging** (sidecar `--debug` flag, wired from Rust) —
   prints the model’s score for every 80 ms frame to **stderr**:
   ```
   score=0.0000
   score=0.4824
   score=0.9991
   ```

**The two-stream gotcha (this is the one that cost us an hour):**

The sidecar writes to **two different streams**, and they go to different
places:

| Stream  | What it carries        | Where it goes                          |
| ------- | ---------------------- | -------------------------------------- |
| stderr  | `score=...` lines      | inherited by the terminal → **you see them** |
| stdout  | `WAKE` lines           | piped to the Rust reader thread → **never printed** |

So **“WAKE” will never appear in the terminal.** It is written on every frame
above threshold, but it goes into the Rust side, not your screen. Do not wait
for “WAKE” — it is a red herring.

**The correct success signal:**

On each wake event the Rust side applies `Transition::WakeWord` and flips the
state machine **Idle → Listening**, then emits a state event. Look for:

```
[prosopon] state event: {state: "listening", ...}
```

That line — not “WAKE” — is the confirmation that the full chain works:
mic → model → sidecar → Rust → state machine.

**The SSH confound (do not repeat this mistake):**

Testing mic capture over SSH is **meaningless**. A Python process launched via
SSH is attributed to the SSH daemon, which has its own TCC context and no mic
permission — so it reads silence *regardless* of whether the app has
permission. It cannot distinguish “the app is muted” from “my test process
is muted.” The only valid measurement is the in-app level meter (tool #1).

**Decision tree when the wake word “doesn’t work”:**

1. **No `mic level` lines at all** → the writer thread isn’t running / mic
   failed to open. Check the `start_wake_word` error path.
2. **`mic level` shows `rms=0.000000` while you talk** → the app itself is
   muted. It’s a TCC issue (see §2.8 dev-mode gotcha). Reset with
   `tccutil reset Microphone science.ravennest.prosopon` and relaunch to force
   the prompt.
3. **`mic level` climbs (rms > ~0.001) but `score=` stays ~0.0000** → audio
   reaches the sidecar but in a degraded form (a pipe bug). Chase the
   f32→int16 conversion and the 48k→16k resample.
4. **`score=` climbs to 0.3+ but no `state event`** → the model hears you;
   the bug is in `Transition::WakeWord` handling or `emit_state`, not the
   audio path.

**Reference numbers (from a working run):** a clean “hey jarvis” scores
0.38–0.99; live voice peaks at 0.99+; threshold is 0.3 (config.yaml). If
scores plateau just under threshold, it’s a pronunciation/threshold tuning
question, not a bug.

---

## Part 3 — Ubuntu-specific: the loopback test

This part runs **only on the Ubuntu box**, because it needs the server's
Kokoro (`:21802`) and Ollama (`:11434`) on the same machine. It is the
highest-value verification available before the real Mac↔server test: it
proves the client and server WebRTC code interoperate, candidate exchange
works, and chunked audio round-trips losslessly.

### 3.1 Run the loopback test

```bash
# ON: Ubuntu
cd ~/prosopon/client-core
cargo test --features live-tests --test loopback   # expect 1/1 pass
```

Requires Kokoro and Ollama to be up on the same box (they are — see
`13-install-guide.md`).

### 3.2 What to develop on Ubuntu

- Transport logic, signaling, chunking — all in `client-core`.
- Anything that doesn't need a mic, speaker, or GUI.

### 3.3 What you *cannot* do on Ubuntu

- Compile the Tauri shell (needs `webkit2gtk` / macOS WebView).
- Test audio playback, STT, wake word, mic capture (need real devices).

---

## Part 4 — End-to-end test (Mac ↔ server)

### 4.1 Server side — `# ON: Ubuntu` (one-time)

1. **Set a non-empty auth token** in `server/config.yaml`:

   ```yaml
   signaling:
     auth_token: "<generate a strong secret>"
   ```

2. **Open the firewall** (STUN discovers the mapping but does not relay):

   ```bash
   # ON: Ubuntu
   sudo ufw allow 29434/udp   # WebRTC ICE / data channel
   sudo ufw allow 29435/tcp   # HTTPS signaling
   ```

3. **Run the server:**

   ```bash
   # ON: Ubuntu
   cd ~/prosopon/server
   cargo build --release
   ./target/release/prosopon-server
   ```

   The signaling endpoint serves HTTPS using the existing Let's Encrypt cert
   for `ac1.ravennest.science` (paths already in `config.yaml`).

### 4.2 Client side — `# ON: Mac`

1. Set `signaling.url` to `https://ac1.ravennest.science:29435/offer` and
   `signaling.auth_token` to the same secret (see §2.7).
2. Run `cargo tauri dev`.
3. Speak → STT → text → server → TTS → audio back.

### 4.3 The symmetric-NAT caveat

STUN (server-reflexive candidates) works for cone NAT — most home/office
networks. If the client sits behind a *symmetric* NAT, STUN alone is
insufficient and a TURN relay is required. M0 assumes cone NAT; TURN is a
later addition if the first test fails on NAT traversal.

---

## The likely first-test failure points (be ready for these)

1. **Tauri shell compile errors** — the `connect_webrtc` / `send_text`
   commands are unverified; expect type/borrow fixes on first Mac build.
2. **NAT traversal** — if the data channel never opens, it's symmetric NAT
   (needs TURN), not a code bug.
3. **Auth mismatch** — empty `auth_token` on either side will fail the
   signaling handshake.
4. **Audio playback** — not built yet; the first end-to-end test will get
   text → server → audio *bytes* back, but playing them needs the rodio/
   symphonia decode step that's still pending.
