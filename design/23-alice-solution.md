Of course! Here's the full English translation with markdown diagrams throughout.

---

# Ideal Tech Stack for an Embedded AI Voice Agent

## Architectural Overview: The Three-Tier System

A smart speaker like Alice is not a single device — it's a three-tier stack. Each tier has distinct latency, power, and compute requirements. The key insight: **audio processing must happen on deterministic hardware, while AI logic runs on Linux**.

```
┌─────────────────────────────────────────────────────────────────┐
│                    THREE-TIER ARCHITECTURE                       │
│                                                                 │
│  TIER 1: AUDIO DSP (Deterministic, <1ms)                       │
│  ┌───────────────────────────────────────────────────────┐     │
│  │  XMOS XU316 (Bare-metal / RTOS)                        │     │
│  │  • AEC (Acoustic Echo Cancellation)                    │     │
│  │  • Beamforming (Microphone Array)                      │     │
│  │  • Noise Suppression                                   │     │
│  │  • AGC (Automatic Gain Control)                       │     │
│  │  • Dereverberation                                     │     │
│  │  • Far-field capture (up to 5m)                        │     │
│  └────────────────────────┬──────────────────────────────┘     │
│                           │ I2S / USB Audio Class 2.0            │
│  TIER 2: APPLICATION PROCESSOR (Linux, 10–500ms)               │
│  ┌───────────────────────────────────────────────────────┐     │
│  │  Rockchip RK3588 (Yocto Linux)                         │     │
│  │  • Wake word detection                                 │     │
│  │  • Streaming ASR                                       │     │
│  │  • LLM inference                                       │     │
│  │  • Streaming TTS                                       │     │
│  │  • Full-duplex FSM (dialogue manager)                  │     │
│  │  • Barge-in handling                                   │     │
│  │  • NPU for AI acceleration (6 TOPS)                    │     │
│  └────────────────────────┬──────────────────────────────┘     │
│                           │ WebSocket / gRPC                    │
│  TIER 3: CLOUD (Optional, 200ms+)                              │
│  ┌───────────────────────────────────────────────────────┐     │
│  │  Cloud GPU (RTX 4090 / H100)                           │     │
│  │  • Heavy LLM inference (if edge can't handle it)       │     │
│  │  • High-quality TTS refinement                         │     │
│  │  • Data not suitable for edge (music, news, weather)   │     │
│  └───────────────────────────────────────────────────────┘     │
└─────────────────────────────────────────────────────────────────┘
```

---

## Tier 1: Audio DSP — The Front End

### Why a Dedicated DSP Is Necessary

Acoustic Echo Cancellation (AEC) and beamforming require **deterministic execution timing**. On Linux, even with `PREEMPT_RT`, the scheduler can preempt your audio thread for several milliseconds, causing artifacts in echo cancellation and missed wake words. Commercial smart speakers (Echo, HomePod, Yandex Station) all use a dedicated audio DSP for exactly this reason. Community discussions confirm that "XMOS DSP handles AEC perfectly, unlike ALSA or WebRTC on Linux." [web_0]

### Recommended Hardware

| Component             | Recommendation                               | Alternative            | Rationale                                                                                                                                                                |
| --------------------- | -------------------------------------------- | ---------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **Audio DSP**         | **XMOS XU316**                               | XVF3800 (pre-licensed) | 16-core RISC, 3200 MIPS, deterministic. AEC + beamforming + NS + AGC + dereverb in a single chip. The reference hardware for smart speakers in 2025–2026. [web_1][web_2] |
| **Microphones**       | **4× MEMS PDM** (Infineon XENSIV or Knowles) | 2× minimum             | PDM MEMS provide channel-to-channel consistency critical for beamforming. 4 mics enable 360° capture. SNR ≥ 65 dB. [web_3]                                               |
| **Speaker amplifier** | **TAS2780** (25W)                            | Any I2S Class-D amp    | Digital I2S input, integrates with the same I2S audio path                                                                                                               |
| **Audio codec**       | **PCM5122** (line out)                       | ES8388                 | DAC for line-out if not using a direct amplifier                                                                                                                         |

### DSP Software

| Layer                      | Package                            | Description                                                                                                                            |
| -------------------------- | ---------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| **AEC + Beamforming + NS** | **XMOS `fwk_voice`**               | Proprietary XMOS library with open SDK. AEC, beamforming, dereverberation, NS, AGC. Runs bare-metal with deterministic timing. [web_0] |
| **Alternative**            | **XMOS XVF3800 firmware**          | Pre-licensed, ready-to-use. AEC, multi-beam beamforming, NS, AGC, 60 dB AGC. More expensive but faster to deploy. [web_4][web_2]       |
| **Interface**              | **USB Audio Class 2.0** or **I2S** | XMOS → Application processor. USB UAC2 for plug-and-play; I2S for lower latency                                                        |

### Why Not Software AEC on Linux?

WebRTC AEC and Speex AEC exist as Linux libraries, but they suffer from OS scheduling jitter. The ReSpeaker/XMOS community resolved this: AEC "runs on the XMOS chip, unlike Linux web_rtc or Speex libraries. This is due to the RTOS microcontroller with exact timing of procedures, unlike a scheduled OS like Linux preempt." [web_0]

---

## Tier 2: Application Processor — The Brain

### Recommended Hardware

| Component      | Recommendation                 | Alternative                               | Rationale                                                                                                                                                                                     |
| -------------- | ------------------------------ | ----------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **SoC**        | **Rockchip RK3588**            | NXP i.MX 8M Plus; NVIDIA Jetson Orin Nano | 8-core ARM (4× A76 + 4× A55), up to 32 GB LPDDR5, **6 TOPS NPU**, Mali-G610 GPU. Best price/performance for edge AI in 2025–2026. Hardware NPU acceleration for ASR/TTS/LLM via RKNN. [web_5] |
| **RAM**        | **8 GB LPDDR5**                | 16 GB for 7B models                       | 8 GB suffices for ASR + TTS + 3–4B LLM (4-bit quantized). 16 GB for 7B.                                                                                                                       |
| **Storage**    | **32 GB eMMC + NVMe SSD slot** | 64 GB eMMC                                | eMMC for root FS; NVMe for model files (Whisper ~1.5 GB, LLM ~4 GB, TTS ~200 MB)                                                                                                              |
| **Networking** | **Wi-Fi 6 + Gigabit Ethernet** | Wi-Fi 5                                   | For cloud offloading when edge capacity is exceeded                                                                                                                                           |

### Why the RK3588?

1. **Integrated NPU (6 TOPS)** can accelerate ASR and TTS models via RKNN, reducing CPU load. The `rknn_model_zoo` repository already provides converted Whisper and VITS TTS models. [web_5]
2. **8 ARM cores** provide parallelism: ASR on 2 cores, LLM on 4 cores, TTS on 2 cores
3. **Price**: RK3588 boards (Orange Pi 5 Plus, Radxa ROCK 5B) cost \$80–150, vs. \$200+ for Jetson Orin Nano
4. **Real-world proof**: The MTS AI team already built a working Russian voice assistant on the RK3588 with ASR + LLM + TTS running locally. [web_5]

### Operating System

| Component           | Recommendation                          | Rationale                                                                                                                    |
| ------------------- | --------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| **OS**              | **Yocto Project (Kirkstone/Scarthgap)** | Industry standard for embedded Linux distributions. Modular, customizable image size, BSP support for RK3588. [web_6][web_7] |
| **Kernel**          | **Linux 6.1+ with PREEMPT_RT**          | Real-time preemptible kernel for audio buffer handling. Not perfect (hence Tier 1 DSP), but reduces jitter                   |
| **Audio subsystem** | **ALSA + PipeWire**                     | PipeWire for low-level audio routing; ALSA for direct hardware device access                                                 |
| **Init**            | **systemd**                             | Standard service management for pipeline processes                                                                           |
| **OTA updates**     | **RAUC or Mender**                      | Secure A/B updates for field deployment                                                                                      |

### Why Yocto Instead of Debian/Ubuntu?

Yocto builds a **minimal, customized image** with only what you need. A stock Ubuntu Server install is ~2 GB; a Yocto image can be ~200 MB. For an embedded device with eMMC storage, this matters. Yocto also provides reproducible builds, BSP isolation, and license compliance. [web_6]

---

## Tier 2 Software Pipeline

The full pipeline has six stages, each with specific latency requirements:

```
┌─────────────────────────────────────────────────────────────────┐
│              SOFTWARE PIPELINE (TIER 2)                          │
│                                                                 │
│  Clean Audio (from Tier 1 DSP)                                  │
│           │                                                      │
│           ▼                                                      │
│  ┌─── 1. Wake Word ───────────────────────────────────┐        │
│  │  openWakeWord (Python/C++, ~2MB, <50ms)              │        │
│  │  Always listening. Triggers on activation phrase.    │        │
│  └───────────────────────────┬──────────────────────┘        │
│                              │ Activated                         │
│  ┌───────────────────────────▼──────────────────────┐        │
│  │  2. Streaming ASR ───────────────────────────────┐        │
│  │  Vosk (K2-FSA, ~50MB, ~200ms)                        │        │
│  │  Or: sherpa-onnx Zipformer (streaming)              │        │
│  │  Or: whisper.cpp tiny (pseudo-streaming)            │        │
│  │  Outputs partial transcriptions in real time         │        │
│  └───────────────────────────┬──────────────────────┘        │
│                              │ Text stream                      │
│  ┌───────────────────────────▼──────────────────────┐        │
│  │  3. FSM / Turn Management ───────────────────────┐        │
│  │  Neural FSM dialogue manager (C++)                   │        │
│  │  States: SPEAK / LISTEN                              │        │
│  │  Handles barge-in, backchannels, floor yielding       │        │
│  └───────────────────────────┬──────────────────────┘        │
│                              │ Dialogue tokens                  │
│  ┌───────────────────────────▼──────────────────────┐        │
│  │  4. LLM Inference ───────────────────────────────┐        │
│  │  llama.cpp (C++, 4-bit GGUF)                          │        │
│  │  Qwen2.5-7B-Instruct (4.4GB Q4_K_M)                  │        │
│  │  Or: Qwen3-8B, Phi-3-mini (3B), Gemma-3-4B          │        │
│  │  Streams tokens as they generate                     │        │
│  └───────────────────────────┬──────────────────────┘        │
│                              │ Text tokens                      │
│  ┌───────────────────────────▼──────────────────────┐        │
│  │  5. Streaming TTS ───────────────────────────────┐        │
│  │  Kokoro-82M via sherpa-onnx (C++)                    │        │
│  │  82M params, ~150ms TTFT                             │        │
│  │  Or: VITS (VITS-Russian for Russian)                 │        │
│  │  Real-time audio chunks                              │        │
│  └───────────────────────────┬──────────────────────┘        │
│                              │ Audio PCM                        │
│  ┌───────────────────────────▼──────────────────────┐        │
│  │  6. Audio Output ────────────────────────────────┐        │
│  │  ALSA → I2S/USB → Tier 1 DSP → Speaker             │        │
│  │  AEC loop: reference signal fed back to Tier 1      │        │
│  │  DSP for echo cancellation reference                │        │
│  └──────────────────────────────────────────────────────┘        │
└─────────────────────────────────────────────────────────────────┘
```

---

## Specific Technology Choices by Stage

### Stage 1: Wake Word Detection

| Category        | Recommendation                                                                                                                                                                                    |
| --------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Package**     | **openWakeWord** (Python, C++ bindings)                                                                                                                                                           |
| **Model size**  | ~2 MB                                                                                                                                                                                             |
| **Latency**     | <50 ms                                                                                                                                                                                            |
| **Rationale**   | Open source, custom wake words, continuous monitoring, low power consumption. Runs on the edge (application processor), not in the cloud. Integrates tightly with Home Assistant/Wyoming. [web_8] |
| **Alternative** | **ESP-SR WakeNet** (Espressif, for ESP32-S3) — if wake word runs on a satellite device. **Porcupine** (Picovoice) — commercial, closed source, but excellent accuracy.                            |
| **For Russian** | Train a custom openWakeWord model on Russian wake word samples (e.g., "Alisa"). Requires ~5–10 hours of collected samples.                                                                        |

### Stage 2: Streaming ASR (Speech Recognition)

This is the **most critical component** for a smart speaker. Streaming is essential — you cannot wait 10 seconds for the user to finish speaking and then process the entire utterance. Whisper, despite its quality, is not suitable for streaming by default. [web_5]

| Category       | Primary                                                                                                                                                                        | Secondary                                                                   |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------- |
| **Engine**     | **Vosk** (K2-FSA toolkit)                                                                                                                                                      | **sherpa-onnx** (Zipformer streaming)                                       |
| **Model size** | ~50 MB (small ru model)                                                                                                                                                        | ~100–300 MB (Zipformer streaming)                                           |
| **Latency**    | ~200 ms (streaming)                                                                                                                                                            | ~150–300 ms (streaming)                                                     |
| **Language**   | Russian model: `vosk-model-small-streaming-ru`                                                                                                                                 | Multilingual Zipformer models                                               |
| **Runtime**    | ONNX Runtime (on CPU)                                                                                                                                                          | ONNX Runtime + RKNN NPU                                                     |
| **Rationale**  | Vosk is designed for streaming recognition with minimal latency. Works offline. Optimized for embedded. The working Russian assistant on RK3588 from MTS AI uses Vosk. [web_5] | sherpa-onnx supports RK3588 NPU, more accurate on noisy audio, but heavier. |

**When NOT to use whisper.cpp:** It's excellent for offline transcription, but not for real-time streaming. "No streaming recognition — you have to send the whole phrase of 10 or 20 seconds, then wait a few seconds — for a voice assistant this is unusable." Whisper also tends to loop or hallucinate on silence. It's better suited for **refinement**, not primary real-time recognition.

### Stage 3: FSM / Dialogue Management (Full-Duplex)

This is where the Neural FSM architecture we discussed earlier applies. For an embedded smart speaker in the style of Alice:

| Category           | Recommendation                                                                                                                                                     |
| ------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **Architecture**   | **Neural FSM** (Wang et al., NeurIPS 2024)                                                                                                                         |
| **Implementation** | Custom C++ module                                                                                                                                                  |
| **Concept**        | Two-state finite state machine (SPEAK/LISTEN). The LLM generates control tokens `[S.SPEAK]`, `[C.SPEAK]`, `[S.LISTEN]`, `[C.LISTEN]` to trigger state transitions. |
| **Training**       | Fine-tune the LLM (20 steps of SFT on ~5,000 examples) to emit control tokens.                                                                                     |
| **Role**           | Decides when to interrupt the user, yield the floor, continue speaking through noise/backchannels, or respond to interruptions.                                    |

For a first version (simpler than full Neural FSM):

| Category                   | Simple Alternative                                                                                           |
| -------------------------- | ------------------------------------------------------------------------------------------------------------ |
| **Backchannel dictionary** | Match partial ASR output against known backchannels ("uh-huh", "okay", "yes"). On match → continue speaking. |
| **Interruption detector**  | If ASR produces substantive text during SPEAK state → stop TTS, switch to LISTEN.                            |
| **Noise detector**         | If Tier 1 DSP signals a non-speech acoustic event → continue speaking.                                       |
| **Implementation**         | ~200 lines of C++ or Python. Much simpler than full FSM fine-tuning.                                         |

### Stage 4: LLM Inference

| Category         | Primary                                                                                                                                                          | Secondary                                |
| ---------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------- |
| **Engine**       | **llama.cpp** (C/C++, ~91K GitHub stars)                                                                                                                         | **RKNN-LLM** (Rockchip NPU acceleration) |
| **Model format** | GGUF Q4_K_M (4-bit)                                                                                                                                              | RKNN INT8                                |
| **Model size**   | 4.4 GB (7B at Q4)                                                                                                                                                | ~3.5 GB (7B at INT8)                     |
| **Model**        | **Qwen2.5-7B-Instruct**                                                                                                                                          | Qwen3-8B, Phi-3-mini (3B), Gemma-3-4B    |
| **Decode speed** | 20–50 tokens/s (RK3588, 8 threads)                                                                                                                               | 30–60 tokens/s (NPU)                     |
| **Rationale**    | llama.cpp is the de facto standard for edge inference. Runs on CPU without GPU. Q4_K_M retains ~95% quality. Streams tokens for low-latency TTS. [web_9][web_10] |

**Why Qwen2.5-7B?**
- Excellent Russian performance (trained on multilingual data)
- 7B at 4-bit quantization fits in 8 GB RAM alongside ASR/TTS
- Fine-tunable for persona (like Alice's personality) via LoRA
- Base model for the FLAIR architecture (if you want to add latent reasoning)

**For memory-constrained devices (4 GB RAM):**
- **Phi-3-mini** (3B, ~1.8 GB Q4) — Microsoft, excellent quality/size ratio
- **Gemma-3-4B** (~2.5 GB Q4) — Google, lightweight, strong
- **BitNet b1.58** (2B, ~0.4 GB) — native 1-bit model, 29 ms latency on CPU, 6× lower energy. Best for extreme edge constraints. [web_10]

### Stage 5: Streaming TTS (Text-to-Speech)

| Category        | Primary                                                                                                                                                                                        | Secondary                   |
| --------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------- |
| **Model**       | **Kokoro-82M**                                                                                                                                                                                 | **VITS** (Russian)          |
| **Runtime**     | **sherpa-onnx** (C++)                                                                                                                                                                          | sherpa-onnx or ONNX Runtime |
| **Size**        | 82M parameters (~320 MB)                                                                                                                                                                       | ~100–200 MB                 |
| **Latency**     | ~150 ms TTFT                                                                                                                                                                                   | ~200 ms                     |
| **License**     | Apache 2.0                                                                                                                                                                                     | Depends on model            |
| **Rationale**   | Kokoro is the gold standard for local TTS in 2026. 82M parameters, cloud-quality synthesis rivaling ElevenLabs. Streaming generation, runs on CPU. Works through sherpa-onnx. [web_11][web_12] |
| **For Russian** | The `rknn_model_zoo` repository includes VITS MMS TTS (multilingual), convertible for Russian. The MTS AI team reports it "handles its tasks" with a light accent. [web_5]                     |

**TTS is where cloud processing benefits most.** If the device is online, high-quality cloud TTS should be the priority offload target. Local TTS serves as a fallback and for low-latency responses.

### Stage 6: Orchestration (The Glue)

| Category            | Recommendation                                                                                                   |
| ------------------- | ---------------------------------------------------------------------------------------------------------------- |
| **Core**            | **C++ pipeline** (GStreamer or custom)                                                                           |
| **Audio transport** | GStreamer with `appsrc`/`appsink` for AI model injection                                                         |
| **IPC**             | ZeroMQ or shared memory for ASR→LLM→TTS pipeline                                                                 |
| **Streaming**       | Event-driven async queues. ASR emits partial text → LLM starts generation → TTS begins synthesis on first tokens |

**Python prototype (for development):**

| Category        | Recommendation                                                                                                                                                                          |
| --------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Framework**   | **Pipecat** (13K stars, BSD-2 license)                                                                                                                                                  |
| **Rationale**   | Transport-agnostic processors, built-in VAD, supports Whisper, Kokoro, Deepgram, ElevenLabs. Best flexibility for iterating on dialogue behavior before committing to C++ architecture. |
| **Alternative** | **LiveKit Agents** (11K stars, Apache 2.0) — if you need WebRTC support for remote audio streams.                                                                                       |

---

## Programming Languages by Layer

| Layer                        | Language                                              | Rationale                                                                                                                       |
| ---------------------------- | ----------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| **Tier 1 DSP (XMOS)**        | **XC** (XMOS C-like) + C                              | Deterministic multithreading. XC compiles to xCORE ISA.                                                                         |
| **Tier 2 pipeline core**     | **C++** (C++17/20)                                    | Zero overhead, memory control, direct ALSA/ONNX access. llama.cpp, sherpa-onnx are native C++.                                  |
| **Tier 2 AI layer (models)** | **Python** (prototype) → **C++** (production)         | Prototype in Python (sherpa-onnx Python bindings, llama.cpp Python server). Deploy in C++ to eliminate GIL and memory overhead. |
| **Wake word**                | **Python** (openWakeWord) or **C** (WakeNet on ESP32) | Minimal load, runs in a background thread.                                                                                      |
| **Orchestration/services**   | **Rust** (optional)                                   | Memory safety for network layer, OTA handlers, config management. Functionally replaces Python/C++ for infrastructure code.     |
| **Cloud**                    | **Python** (FastAPI/gRPC)                             | Fast iteration, easy AI ecosystem integration.                                                                                  |

---

## Full Package List

### Embedded Device (Tier 2)

| Category           | Package                              | Version      | Purpose                               |
| ------------------ | ------------------------------------ | ------------ | ------------------------------------- |
| **ASR**            | `vosk` (Python + native)             | 0.3.45+      | Streaming recognition, offline        |
| **ASR runtime**    | `sherpa-onnx`                        | 1.10+        | Unified ASR/TTS/VAD engine            |
| **LLM**            | `llama.cpp`                          | b3800+       | Inference, streaming token generation |
| **LLM alt**        | `rknn-llm`                           | Rockchip SDK | NPU acceleration                      |
| **TTS**            | `kokoro-onnx`                        | v1.0+        | Lightweight neural TTS                |
| **TTS runtime**    | `sherpa-onnx`                        | 1.10+        | Cross-platform TTS inference          |
| **Wake word**      | `openWakeWord`                       | 2.0+         | Custom wake words                     |
| **VAD**            | `silero-vad` (via sherpa-onnx)       | v5           | Voice activity detection              |
| **Audio**          | `ALSA`, `PipeWire`                   | System       | Audio capture/playback                |
| **ML runtime**     | `onnxruntime`                        | 1.19+        | Engine for ASR/TTS/VAD models         |
| **Audio analysis** | `librosa` (Python, prototyping only) | 0.10+        | Audio analysis during development     |
| **Networking**     | `libwebsockets` or `cpp-httplib`     | Latest       | WebSocket for cloud communication     |
| **Serialization**  | `protobuf`                           | 3.25+        | gRPC compact RPC for ASR↔LLM↔TTS      |
| **Config**         | `nlohmann/json`                      | 3.11+        | JSON configuration management         |
| **Logging**        | `spdlog`                             | 1.14+        | Async C++ logging                     |
| **OTA**            | `RAUC`                               | 1.12+        | A/B updates, secure boot              |

### Cloud (Tier 3, optional)

| Category       | Package                               | Purpose                                            |
| -------------- | ------------------------------------- | -------------------------------------------------- |
| **LLM server** | `vLLM` or `SGLang`                    | High-throughput inference with continuous batching |
| **TTS**        | `StyleTTS 2` or commercial API        | Higher-quality synthesis than Kokoro               |
| **API**        | `FastAPI` + `uvicorn`                 | HTTP/WebSocket endpoints                           |
| **Queues**     | `Redis`                               | Session queues, dialogue caching                   |
| **Deployment** | `Docker` + `NVIDIA Container Toolkit` | Containerized deployment with GPU support          |

### Development & Testing

| Category           | Package                   | Purpose                             |
| ------------------ | ------------------------- | ----------------------------------- |
| **Prototyping**    | `Pipecat`                 | Fast iteration of dialogue behavior |
| **Testing**        | `pytest`                  | Integration tests                   |
| **ASR evaluation** | `jiwer`                   | WER calculation                     |
| **Benchmarking**   | `llama-bench` (llama.cpp) | Measure tokens/s                    |
| **Profiling**      | `perf`, `hotspot`         | CPU profiling                       |
| **Build**          | `Yocto BitBake` + `CMake` | Cross-compilation                   |

---

## Physical Hardware Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│               PHYSICAL DEVICE ARCHITECTURE                      │
│                                                                 │
│                  ┌───────┐ ┌───────┐ ┌───────┐ ┌───────┐       │
│                  │ Mic 1 │ │ Mic 2 │ │ Mic 3 │ │ Mic 4 │       │
│                  │ MEMS  │ │ MEMS  │ │ MEMS  │ │ MEMS  │       │
│                  │ PDM   │ │ PDM   │ │ PDM   │ │ PDM   │       │
│                  └───┬───┘ └───┬───┘ └───┬───┘ └───┬───┘       │
│                      │         │         │         │            │
│                      └─────────┴─────────┴─────────┘            │
│                                 │ PDM bus                        │
│                      ┌──────────▼──────────┐                    │
│                      │   XMOS XU316 DSP    │                    │
│                      │   16 cores, 800 MHz  │                    │
│                      │   • AEC (full duplex)│                    │
│                      │   • Beamforming      │                    │
│                      │   • Noise suppression│                    │
│                      │   • AGC              │                    │
│                      │   • Dereverb         │                    │
│                      └──────────┬──────────┘                    │
│                                 │ I2S (48 kHz, 16-bit)           │
│                      ┌──────────▼──────────┐                    │
│                      │   RK3588 SoC         │                    │
│                      │   4× A76 + 4× A55   │                    │
│                      │   8 GB LPDDR5        │                    │
│                      │   6 TOPS NPU         │                    │
│                      │   Mali-G610 GPU      │                    │
│                      │                      │                    │
│                      │   Yocto Linux        │                    │
│                      │   • openWakeWord     │                    │
│                      │   • Vosk ASR         │                    │
│                      │   • llama.cpp LLM    │                    │
│                      │   • Kokoro TTS       │                    │
│                      │   • FSM manager      │                    │
│                      └──────────┬──────────┘                    │
│                                 │ I2S                            │
│                      ┌──────────▼──────────┐                    │
│                      │  TAS2780 Class-D    │                    │
│                      │  amplifier, 25W     │                    │
│                      └──────────┬──────────┘                    │
│                                 │                                │
│                      ┌──────────▼──────────┐                    │
│                      │  3" full-range      │                    │
│                      │  speaker, 4Ω, 25W   │                    │
│                      └─────────────────────┘                    │
│                                                                 │
│  AEC reference: RK3588 also sends TTS reference signal to      │
│  XMOS DSP (via separate I2S channel) for adaptive echo          │
│  cancellation filter reference.                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## Edge vs. Cloud Compute Split Strategy

Not everything should run on the edge. Here's a pragmatic split:

| Component             | Edge (on-device)  | Cloud (optional)        | Rationale                                                         |
| --------------------- | ----------------- | ----------------------- | ----------------------------------------------------------------- |
| **AEC / Beamforming** | ✅ Always          | ❌ Never                 | Must be local — <1ms latency                                      |
| **Wake word**         | ✅ Always          | ❌ Never                 | Privacy + offline operation                                       |
| **VAD**               | ✅ Always          | ❌                       | Local latency                                                     |
| **ASR (primary)**     | ✅ Always          | ❌                       | Local Vosk handles 95% of queries                                 |
| **ASR (refinement)**  | Optional          | ✅ For complex queries   | Cloud whisper-large for hard queries                              |
| **LLM (dialogue)**    | ✅ For simple chat | ✅ For complex reasoning | 3B–7B locally for fast responses; cloud 70B+ for hard tasks       |
| **TTS**               | ✅ Default         | ✅ High quality          | Kokoro locally for ~150ms TTFT; cloud StyleTTS2 for natural voice |
| **Music/streaming**   | ❌                 | ✅                       | External services (Yandex Music, Spotify)                         |
| **Smart home**        | ✅ Locally         | ❌                       | Local control (Matter/Thread)                                     |

**Cloud escalation architecture:**

```
┌──────────────────────────────────────────────────────────────┐
│            DYNAMIC ESCALATION                                │
│                                                              │
│  User says: "What's the weather?"                           │
│  → Edge: Vosk ASR → Local LLM → Local TTS                    │
│  → Latency: ~500ms (fully local)                             │
│                                                              │
│  User says: "Write an essay about the causes of              │
│  World War I"                                                │
│  → Edge: Vosk ASR → Cloud LLM 70B → Local TTS               │
│  → Latency: ~1.2s (cloud for reasoning)                      │
│                                                              │
│  User says: "Tell me a voice story"                         │
│  → Edge: Vosk ASR → Local LLM → Cloud TTS                   │
│  → Latency: ~800ms (cloud for voice quality)                 │
│                                                              │
│  Rule: Run locally by default.                              │
│  Escalate to cloud only when:                               │
│  1. Query complexity exceeds edge LLM capacity               │
│  2. High-quality TTS is desired                             │
│  3. External data is needed (music, news, weather)          │
└──────────────────────────────────────────────────────────────┘
```

---

## Latency Budget (Target: <800ms for Voice Response)

| Stage                   | Latency | Cumulative | Optimization                            |
| ----------------------- | ------- | ---------- | --------------------------------------- |
| **AEC + capture** (DSP) | 5 ms    | 5 ms       | Hardware DSP                            |
| **Wake word**           | 50 ms   | 55 ms      | openWakeWord                            |
| **VAD + ASR**           | 200 ms  | 255 ms     | Vosk streaming                          |
| **ASR endpointing**     | 300 ms  | 555 ms     | Wait 300ms of silence for utterance end |
| **LLM first token**     | 200 ms  | 755 ms     | llama.cpp prefill                       |
| **TTS TTFT**            | 150 ms  | 905 ms     | Kokoro streaming                        |
| **Audio playback**      | 10 ms   | 915 ms     | ALSA                                    |

This is ~900ms from end of user speech to first speaker audio. To achieve <500ms (the natural conversation threshold):

| Optimization                   | Savings       | How                                              |
| ------------------------------ | ------------- | ------------------------------------------------ |
| **Speculative decoding**       | -100ms on LLM | Draft 0.5B model generates, 7B verifies          |
| **Aggressive ASR endpointing** | -150ms        | 150ms silence threshold instead of 300ms         |
| **TTS pre-warm**               | -50ms         | Pre-load Kokoro model into RAM at startup        |
| **LLM prefill parallelism**    | -50ms         | Stream ASR tokens into LLM before utterance ends |
| **Full-duplex (FSM)**          | -300ms        | Model begins generation DURING user speech       |

With full-duplex FSM (as discussed in the Neural FSM paper): the LLM sees streaming ASR tokens and begins generating a response as soon as it has enough information — potentially **before** the user finishes speaking. This brings perceived latency down to **~400ms** or less, which fits within natural conversational rhythm.

---

## Technology Summary Table

| Layer            | Component        | Technology          | Language     | Size     | Latency      |
| ---------------- | ---------------- | ------------------- | ------------ | -------- | ------------ |
| **Tier 1 DSP**   | AEC/Beam/NS      | XMOS `fwk_voice`    | XC/C         | Firmware | <1 ms        |
| **Tier 1 DSP**   | Chip             | XMOS XU316          | —            | —        | —            |
| **Tier 2 OS**    | Operating system | Yocto Linux 6.1+ RT | —            | ~200 MB  | —            |
| **Tier 2 OS**    | SoC              | RK3588 (8 GB)       | —            | —        | —            |
| **Tier 2 Wake**  | Wake word        | openWakeWord        | Python/C     | ~2 MB    | <50 ms       |
| **Tier 2 ASR**   | Recognition      | Vosk (K2-FSA)       | C++/Python   | ~50 MB   | ~200 ms      |
| **Tier 2 LLM**   | Inference        | llama.cpp           | C++          | ~4.4 GB  | ~200 ms TTFT |
| **Tier 2 LLM**   | Model            | Qwen2.5-7B Q4_K_M   | GGUF         | —        | 20–50 t/s    |
| **Tier 2 TTS**   | Synthesis        | Kokoro-82M          | C++ (sherpa) | ~320 MB  | ~150 ms      |
| **Tier 2 FSM**   | Dialogue         | Neural FSM          | C++          | ~500 LoC | <5 ms        |
| **Tier 2 Orch.** | Pipeline         | GStreamer + ZMQ     | C++          | —        | <10 ms       |
| **Tier 3 Cloud** | LLM server       | vLLM                | Python       | —        | ~150 ms      |
| **Tier 3 Cloud** | TTS              | StyleTTS2           | Python       | —        | ~200 ms      |

---

## Development Phases

**Phase 1 (MVP — fully local, simple dialogue):**
- RK3588 + XMOS XU316 + 4 microphones
- openWakeWord → Vosk → llama.cpp (Phi-3-mini 3B) → Kokoro TTS
- Simple VAD-based FSM (no LLM fine-tuning)
- Fully offline, ~800ms latency
- ~3 months, 2–3 developers

**Phase 2 (Full-duplex — Neural FSM):**
- Fine-tune LLM for control tokens (Neural FSM, 20 steps SFT)
- Barge-in handling with semantic understanding
- Speculative decoding for latency reduction
- Cloud escalation for complex queries
- ~3 additional months

**Phase 3 (Latent reasoning — FLAIR):**
- FLAIR training with Global-aware Expert
- Model "thinks" during user speech
- 7B inference with NPU acceleration
- Cloud expert only for training, inference fully on edge
- ~6 additional months + GPU training access

---

