# 19 — Client Setup Guide (Mac → Ubuntu → End-to-End Test)

Date: 2026-09-08
Author: Skye Laflamme
Status: verified against the actual repo state (client-core, src-tauri, configs)
Depends on: `13-install-guide.md`, `17-client-implementation.md`

The goal, in order:
1. **Mac** — set up the real client (the M1 Max MacBook) so the Tauri shell
   can build and run.
2. **Ubuntu** — set up a dev box to develop and test `client-core` (the
   transport) without needing the Mac.
3. **End-to-end** — connect the client to the live server and get the first
   real turn through.

---

## The honest state (what's already done vs. pending)

| Piece | Status |
|-------|--------|
| Server (WebRTC + signaling + pipeline) | ✅ built, 13/13 tests, live-tested |
| `client-core` (transport) | ✅ built, 3/3 tests + loopback 1/1 |
| Tauri shell (state machine + orb) | ⚠️ written, **never compiled** (needs Mac) |
| Audio playback (Ogg Opus → speaker) | ❌ not built |
| STT (Moonshine) | ❌ not built |
| Wake word (openWakeWord) | ❌ not built |
| Mic capture (cpal) | ❌ not built |

So the *transport* is proven. The *GUI shell* and the *audio I/O* are the
remaining work, and they can only be finished on the Mac.

---

## Part 1 — Mac client setup (the real target)

Target: MacBook Pro 16" 2021, M1 Max, 32 GB, macOS arm64.

### 1.1 Xcode Command Line Tools

```bash
xcode-select --install
```

### 1.2 Rust toolchain

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustc --version   # confirm it installed
```

### 1.3 Tauri prerequisites + CLI

Tauri 2 on macOS needs the system WebView (present on macOS by default) and
the CLI:

```bash
cargo install tauri-cli
```

### 1.4 Clone the repo

```bash
git clone git@github.com:larklaflamme/prosopon.git ~/prosopon
cd ~/prosopon
```

(Adjust the remote URL to the actual repo.)

### 1.5 Verify `client-core` compiles on the Mac

Before touching the GUI, confirm the transport crate builds on arm64:

```bash
cd ~/prosopon/client-core
cargo build
cargo test          # expect 3/3 pass
```

### 1.6 Build the Tauri shell (first compile)

```bash
cd ~/prosopon
cargo tauri dev
```

This is the first real compile of `src-tauri/`. Expect it to surface the
integration errors that couldn't be caught on the headless box — the
`connect_webrtc` / `send_text` commands in `src-tauri/src/lib.rs` are written
but unverified.

### 1.7 Icons

`tauri.conf.json` references `icons/*`. Generate them from a source PNG:

```bash
cargo tauri icon path/to/icon.png
```

### 1.8 Python venv (for Moonshine + openWakeWord)

```bash
mkdir -p ~/prosopon/client && cd ~/prosopon/client
python3 -m venv .venv
source .venv/bin/activate
pip install --upgrade pip
```

### 1.9 STT + wake word (exact package names to confirm at install time)

```bash
pip install moonshine        # streaming STT, MIT — confirm against usefulsensors/moonshine README
pip install openwakeword     # wake word, MIT — confirm against dscripka/openWakeWord README
```

### 1.10 Models

```bash
mkdir -p ~/prosopon/client/models
# Moonshine base model + openWakeWord "hey skye" model.
# Exact URLs from the respective project READMEs.
```

### 1.11 Client config

Create `~/prosopon/client/config.yaml`:

```yaml
signaling:
  url: "https://ac1.ravennest.science:29435/offer"   # the server's HTTPS signaling endpoint
  auth_token: "<shared secret>"                       # must match the server's signaling.auth_token
webrtc:
  stun_servers:
    - "stun:stun.l.google.com:19302"
```

---

## Part 2 — Ubuntu dev setup (develop `client-core` without the Mac)

The point of this box: `client-core` is deliberately GUI-free so the
transport can be developed and tested headlessly. The Tauri shell *cannot*
build here (no `webkit2gtk`), but everything below it can.

### 2.1 Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustc --version
```

### 2.2 Clone + build

```bash
git clone git@github.com:larklaflamme/prosopon.git ~/prosopon
cd ~/prosopon/client-core
cargo build
cargo test          # 3/3 pass
```

### 2.3 The loopback test (the high-value one)

This runs the *real* client transport against the *real* server crate — full
WebRTC, real Kokoro + Ollama, chunked audio round-trip:

```bash
cd ~/prosopon/client-core
cargo test --features live-tests --test loopback   # 1/1 pass
```

This requires the server's Kokoro (`:21802`) and Ollama (`:11434`) to be up
on the same box. It proves the two WebRTC implementations interoperate.

### 2.4 What to develop here

- Transport logic, signaling, chunking — all in `client-core`.
- Anything that doesn't need a mic, speaker, or GUI.

### 2.5 What you *cannot* do here

- Compile the Tauri shell (needs `webkit2gtk` / macOS WebView).
- Test audio playback, STT, wake word, mic capture (need real devices).

---

## Part 3 — End-to-end test (Mac ↔ server)

### 3.1 Server side (one-time, on the Ubuntu server)

1. **Set a non-empty auth token** in `server/config.yaml`:

   ```yaml
   signaling:
     auth_token: "<generate a strong secret>"
   ```

2. **Open the firewall** (STUN discovers the mapping but does not relay):

   ```bash
   sudo ufw allow 29434/udp   # WebRTC ICE / data channel
   sudo ufw allow 29435/tcp   # HTTPS signaling
   ```

3. **Run the server:**

   ```bash
   cd ~/prosopon/server
   cargo build --release
   ./target/release/prosopon-server
   ```

   The signaling endpoint serves HTTPS using the existing Let's Encrypt cert
   for `ac1.ravennest.science` (paths already in `config.yaml`).

### 3.2 Client side (Mac)

1. Set `signaling.url` to `https://ac1.ravennest.science:29435/offer` and
   `signaling.auth_token` to the same secret (see §1.11).
2. Run `cargo tauri dev`.
3. Speak → STT → text → server → TTS → audio back.

### 3.3 The symmetric-NAT caveat

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
