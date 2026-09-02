# 13 — Install Guide (Server + macOS Client)

Ordered, step-by-step install for the Prosopon voice client. **Revised
2026-09-01** after verifying the actual running state of the server and
settling the STT question.

**Settled architecture (this revision):**
- **STT runs on the client (macOS) — Moonshine.** No audio crosses the wire
  before understanding; only text goes to the server.
- **TTS runs on the server — Kokoro (already in Docker).**
- **Cognition runs on the server — Ollama (already running, localhost:11434).**
- **The server does NOT do STT.** The Whisper STT container is running on the
  box but is *not used* by this project.

**Target hardware (from 04):**
- Server: Ubuntu (this box), Rust 1.98.0 confirmed.
- Client: MacBook Pro 16" 2021, M1 Max, 32GB, macOS arm64.

---

## Verified running state (2026-09-01, checked live)

| Service | Container / process | Host port | Status | Used by Prosopon? |
|---------|--------------------|-----------|--------|-------------------|
| Kokoro TTS | `secretai-kokoro-tts-1` (`ghcr.io/remsky/kokoro-fastapi-gpu:latest`) | 21802 | healthy | **Yes — TTS** |
| Ollama | native, v0.33.1 | 11434 | running | **Yes — cognition** |
| Whisper STT | `secretai-stt-service-1` | 21801 | healthy | **No** (client does STT) |

### Kokoro TTS — OpenAI-compatible API

- Base: `http://localhost:21802`
- Endpoints: `/v1/audio/speech`, `/v1/models`, `/v1/audio/voices`, `/health`
- Models: `tts-1`, `tts-1-hd`, `kokoro`, `gpt-4o-mini-tts`
- Voices: full Kokoro set, incl. `af_sky`, `af_heart`, `af_nova`, `af_nicole`,
  `af_bella`, `af_sarah`, `am_onyx`, `bf_emma`, `bm_george`, …
- Swagger UI: `http://localhost:21802/docs`

### Ollama — cognition

- Base: `http://localhost:11434`
- Full model list (from `ollama list`, 2026-09-01):

| Model | Size | Notes |
|-------|------|-------|
| `muse-glimmer:latest` | 18 GB | 27.9B, tools + thinking + vision (thinking model — dropped for M0) |
| `qwen3:30b` | 18 GB | strong general |
| `qwen3:32b` | 20 GB | strong general |
| `qwen3.6:35b` | 23 GB | newer qwen3 line |
| `gemma4:31b` | 19 GB | strong general |
| `gemma4:26b` | 17 GB | strong general |
| `qwen3:235b` | 142 GB | largest local |
| `gpt-oss:120b` | 65 GB | large local |
| `qwen3-coder:latest` | 18 GB | code-focused |
| `qwen3-coder-next:q4_K_M` | 51 GB | code-focused |
| `qwen3-next:80b-a3b-thinking` | 50 GB | thinking model |
| `llama3.2-vision:latest` | 7.8 GB | vision |
| `llama3.2:latest` | 2.0 GB | small general |
| `qwen2.5:3b` | 1.9 GB | small general |
| `mathstral:7b` | 4.1 GB | math |
| `tinyllama:latest` | 637 MB | tiny |
| `verif_sys:latest` | 637 MB | verification |
| `nomic-embed-text:latest` | 274 MB | embeddings |
| `nomic-embed-text-v2-moe:latest` | 957 MB | embeddings |
| `mxbai-embed-large:latest` | 669 MB | embeddings |
| `kimi-k3:cloud` | — | remote 2.81T |
| `deepseek-v4-pro:cloud` | — | remote |
| `deepseek-v4-flash:cloud` | — | remote |
| `kimi-k2.7-code:cloud` | — | remote code |

---

## Part A — Server side (Ubuntu)

The server runs **only the Rust WebRTC server**, which contacts the
*existing* Kokoro container (TTS) and the *existing* Ollama (cognition) on
localhost. **No Python venv, no Kokoro install, no espeak-ng, no model
downloads, no STT** — all of that is already running or lives on the client.

### A1. Confirm the services are up (sanity check)

```bash
curl -s http://localhost:21802/health        # expect {"status":"healthy"}
curl -s http://localhost:11434/api/tags      # expect a models list
```

If Kokoro is down, restart the container rather than reinstalling:

```bash
docker restart secretai-kokoro-tts-1
```

### A2. Rust server crate

```bash
mkdir -p ~/prosopon/server && cd ~/prosopon/server
cargo init --name prosopon-server
# add deps: webrtc 0.21, tokio, reqwest (for Kokoro/Ollama HTTP), serde
cargo add webrtc@0.21 tokio --features tokio/full
cargo add reqwest --features reqwest/json
cargo add serde --features serde/derive
```

### A3. Server config (endpoints)

The server reads these from `config.yaml` (or env), pointing at the running
services on its own localhost:

```yaml
tts:
  base_url: "http://localhost:21802"
  model: "kokoro"          # or tts-1 / tts-1-hd
  voice: "af_heart"         # default; configurable via config.yaml
cognition:
  base_url: "http://localhost:11434"
  model: "qwen2.5:3b"     # fast-but-dumb default (M0); configurable via config.yaml
webrtc:
  listen_port: 29434       # the UDP ICE port the client connects to
```

### A4. Build & run

```bash
cargo build --release
./target/release/prosopon-server
```

---

## Part B — Client side (macOS, M1 Max)

The client runs: wake word (openWakeWord), STT (Moonshine), the Tauri shell,
and the WebRTC client. It sends **text** to the server and receives **audio**
back.

### B1. Xcode Command Line Tools

```bash
xcode-select --install
```

### B2. Rust toolchain

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
# match server: rustup default 1.98.0 (or latest stable)
rustc --version
```

### B3. Tauri CLI

```bash
cargo install tauri-cli
```

### B4. Python venv (for Moonshine + openWakeWord)

```bash
mkdir -p ~/prosopon/client && cd ~/prosopon/client
python3 -m venv .venv
source .venv/bin/activate
pip install --upgrade pip
```

### B5. Moonshine STT

```bash
# Moonshine — streaming STT, MIT. Exact package name to confirm against
# the usefulsensors/moonshine README at install time.
pip install moonshine
```

### B6. openWakeWord (wake word "Hey Skye")

```bash
# openWakeWord — MIT, fully local. Exact package name to confirm against
# the dscripka/openWakeWord README at install time.
pip install openwakeword
```

### B7. Download models

```bash
mkdir -p ~/prosopon/client/models
# Moonshine base model + openWakeWord "hey skye" model.
# Exact URLs from the respective project READMEs.
```

### B8. Build the Tauri scaffold

```bash
cd ~/prosopon
cargo tauri dev        # first run compiles; needs the icons (see B9)
```

### B9. Icons

`tauri.conf.json` references `icons/*`. Generate them:

```bash
cargo tauri icon path/to/icon.png
```

### B10. Network reachability (STUN + firewall) — revised 2026-09-02

The client reaches the server's WebRTC port only — Kokoro and Ollama are
contacted by the *server* on its own localhost, not by the client.

**The earlier SSH-tunnel guidance was wrong and is removed.** SSH tunnels
forward TCP, but WebRTC's data channel is SCTP-over-DTLS-over-**UDP** — an
SSH tunnel cannot carry it. The correct approach is:

1. **STUN for candidate discovery** (configured, default Google STUN). Both
   peers query a public STUN server to learn their public (server-reflexive)
   IP:port mapping. This is already wired into `config.yaml` under
   `webrtc.stun_servers` on both sides.

2. **Open the UDP ICE port on the server firewall.** STUN only *discovers*
   the mapping; it does not relay media. The server's UDP port `29434` must
   be reachable from the internet for the data channel to flow. Example
   (ufw):

   ```bash
   sudo ufw allow 29434/udp   # WebRTC ICE / data channel
   sudo ufw allow 29435/tcp   # HTTP signaling (offer/answer)
   ```

3. **Symmetric-NAT caveat.** STUN (server-reflexive candidates) works for
   cone NAT, which covers most home/office networks. If the client sits
   behind a *symmetric* NAT, STUN alone is insufficient and a TURN relay is
   required. M0 assumes cone NAT; TURN is a later addition if needed.

4. **Set a non-empty `signaling.auth_token` before exposing the server.**
   With the ports open to the internet, the shared-secret token is the gate:
   the client presents `Authorization: Bearer <token>` on `POST /offer`, and
   the server rejects anything else with an empty 401. The server prints a
   startup warning if the token is empty. The data channel (UDP 29434) needs
   no separate rule — its DTLS handshake verifies certificate fingerprints
   exchanged over the authenticated signaling channel, so an unauthenticated
   peer can never open it.

   ```yaml
   signaling:
     listen_port: 29435
     auth_token: "replace-with-a-long-random-secret"
     tls:
       cert: "/etc/letsencrypt/live/ac1.ravennest.science/fullchain.pem"
       key:  "/etc/letsencrypt/live/ac1.ravennest.science/privkey.pem"
   ```

   **HTTPS (recommended for direct connection).** The signaling endpoint
   supports TLS: set `signaling.tls.cert` and `signaling.tls.key` to the
   certificate chain and private key paths, and the server serves HTTPS
   instead of plain HTTP. The client then points `signaling.url` at
   `https://ac1.ravennest.science:29435/offer`. With HTTPS, the auth token is encrypted in
   transit, closing the cleartext-eavesdropping gap for clients on untrusted
   networks (coffee-shop Wi-Fi). Leave both paths empty for plain-HTTP
   localhost dev.

The client connects to the server's public IP over UDP 29434 (data channel)
and TCP 29435 (signaling).

---

## Decisions made (2026-09-01)

1. **Voice** — `af_heart` (default). Lark's deliberate choice (not `af_sky`). **Configurable** via
   `config.yaml` under `tts.voice`.

2. **Cognition model** — `qwen2.5:3b` (1.9 GB, non-thinking, ~7 ms warm TTFT).
   Fast-but-dumb default for M0 — the pipeline is the deliverable, the model
   is a swap. **Configurable** via `config.yaml` under `cognition.model`;
   swap to `qwen3:30b` (think:false) for the smart tier later, no rebuild.

## Honesty note

- **Verified live this session:** the running containers, their ports, the
  Kokoro OpenAI-compatible API surface, and the full Ollama model list — all
  via `curl` against the actual services.
- **From training, not verified:** the exact `pip` package names
  (`moonshine`, `openwakeword`), the `cargo add` feature flags, and the
  `tauri icon` command syntax. Confirm against current project READMEs at
  install time.
