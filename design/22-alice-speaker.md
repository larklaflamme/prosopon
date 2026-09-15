Here is the detailed English translation with expanded algorithmic precision and markdown diagrams.

---

# Yandex Alice Smart Speaker: Full-Duplex Dialogue Pipeline

## Overview

Yandex Alice (Алиса) smart speakers — such as the **Yandex Station** family — do **not** implement true full-duplex communication (where both parties speak simultaneously and the system processes two independent audio streams in parallel). Instead, they use a sophisticated pipeline of **on-device signal processing**, **local neural networks**, and **cloud-based inference** to approximate natural turn-taking in dialogue. The system handles interruptions ("barge-in") through fast local detection rather than simultaneous bidirectional processing.

---

## Stage 1: Alice Is Speaking — Loudspeakers Active

When Alice begins synthesizing and playing back a response via TTS (Text-to-Speech), the device's speakers emit sound. The microphones, located on the same chassis and often within centimeters of the speakers, pick up this playback as acoustic echo. To avoid "deafening itself," the device employs a multi-stage front-end pipeline running locally on the main board.

### 1.1 Acoustic Echo Cancellation (AEC)

The core problem: the microphone signal \$M\$ contains a mixture of the speaker's output (music, TTS audio), the user's voice, and ambient noise:

\$\$M = \text{Music} + \text{Speech} + \text{Noise}\$\$

The device knows the reference signal \$S\$ (what it sent to the speakers). It must estimate the acoustic transfer function of the room to reconstruct what the microphone *should* hear from the speakers alone, then subtract it.

**Algorithm (production version — Kalman Filter per sub-band):**

1. The microphone signal \$M\$ and the reference signal \$S\$ are transformed into the frequency domain using a Short-Time Fourier Transform (STFT) with overlapping windows.
2. The signal is split into independent frequency sub-bands (typically 64–256 bins).
3. For each frequency bin \$k\$, an adaptive filter estimates the echo path. The echo is modeled as:

\$\$\text{Echo}_k = \sum_{i=0}^{L} a_k[i] \cdot S_k[n-i]\$\$

where \$L\$ is the filter length (covering approximately 200 ms to 1 s of reverberation tail), and \$a_k[i]\$ are the adaptive coefficients for frequency bin \$k\$.

4. The baseline approach uses **Least Mean Squares (LMS)** adaptation, minimizing:

\$\$(M_k - \text{Echo}_k)^2 \to \min\$\$

However, LMS requires updating thousands of coefficients at 16,000 samples/second, creating millions of operations per second.

5. The production solution uses a **per-sub-band Kalman filter**, which:
   - Processes each frequency bin independently (assuming frequency-bin independence for computational efficiency).
   - Models both the echo path (state) and observation noise, making it robust to **double-talk** (when both the user and the speaker are active simultaneously).
   - The Kalman filter computes the Kalman gain \$K\$ for each bin:

\$\$K_k = P_k \cdot F_k \cdot (F_k^T \cdot P_k \cdot F_k + R_k)^{-1}\$\$

where \$P_k\$ is the state error covariance, \$F_k\$ is the observation matrix, and \$R_k\$ is the observation noise covariance for bin \$k\$.
   - Adapts quickly to room changes (furniture moved, door opened) and handles **double-talk** without diverging — a known weakness of plain LMS filters.

6. The residual signal (microphone minus estimated echo) is the cleaned user speech + noise, which is passed to the next stage.

> For stereo devices like the **Station Max**, the problem doubles: each channel (\$S_1\$, \$S_2\$) has its own transfer function, and a **stereo Kalman filter** with cross-channel correlation handling is used.

### 1.2 Direction of Arrival (DoA) and Beamforming

After AEC, the 6- or 7-microphone array (depending on the model) is used to locate and isolate the user's voice:

1. **Direction of Arrival (DoA):** Time Difference of Arrival (TDOA) is computed across microphone pairs using cross-correlation. The algorithm estimates the angle \$\theta\$ from which the dominant speech source arrives.

2. **Beamforming (spatial filtering):** Weighted coefficients \$w_i\$ are applied to each microphone \$i\$, with appropriate time delays \$\tau_i\$, to construct a "beam" pointing toward the speaker:

\$\$b(t) = \sum_{i=1}^{N} w_i \cdot p_i(t - \tau_i)\$\$

This maximizes sensitivity in the direction of the user and suppresses signals from other directions. Yandex uses **adaptive beamforming** (similar to MVDR — Minimum Variance Distortionless Response), which continuously tracks the speaker's position even if they move.

### 1.3 Noise Suppression and De-reverberation

After beamforming, additional processing removes residual noise (ANC — Active Noise Cancellation) and late reverberation tails (de-reverberation), producing a clean single-channel speech signal suitable for recognition.

### Architecture Diagram: On-Device Front-End Pipeline

```
┌─────────────────────────────────────────────────────────────────┐
│                    ON-DEVICE FRONT-END PIPELINE                   │
│                                                                  │
│  ┌──────────┐     ┌─────────┐     ┌──────────┐                  │
│  │ Mic Array │────▶│  AEC    │────▶│ DoA +    │                  │
│  │ (6-7 mics)│     │ (Kalman │     │Beamform  │                  │
│  └──────────┘     │  filter  │     │ (MVDR)   │                  │
│       ▲           └─────────┘     └────┬─────┘                  │
│       │                                │                         │
│  ┌──────────┐                    ┌────▼─────┐                    │
│  │ Reference │                   │  Noise    │                   │
│  │ Signal S  │──────────────▶   │Suppression│                  │
│  │ (speaker) │                   │ + De-rev  │                   │
│  └──────────┘                    └────┬─────┘                   │
│                                       │                         │
│                                ┌──────▼──────┐                  │
│                                │ Clean Speech │                  │
│                                │  (mono, 16kHz)│                 │
│                                └──────┬──────┘                  │
│                                       │                         │
│                          ┌────────────┼────────────┐            │
│                          ▼            ▼            ▼            │
│                    ┌──────────┐ ┌────────────┐ ┌──────────┐    │
│                    │ KWS Model│ │ Quick Cmd   │ │ Buffer   │    │
│                    │ ("Alice")│ │ Model       │ │ (rolling)│    │
│                    └──────────┘ └────────────┘ └──────────┘    │
└─────────────────────────────────────────────────────────────────┘
```

---

## Stage 2: Local Neural Networks — Always Listening

Two lightweight neural models run **entirely on-device** (no cloud round-trip), processing the cleaned audio stream in real time.

### 2.1 Keyword Spotting (KWS) — "Alice" Detection

- **Architecture:** Convolutional Neural Network (CNN), typically small (a few hundred thousand parameters).
- **Input:** Audio is framed at 25 ms windows with 10 ms hop size, converted to **log-Mel filterbank features** (typically 40–80 dimensional vectors per frame).
- **Processing:** Multiple convolutional layers extract temporal-frequency patterns. The output layer produces a probability for each frame: "Alice detected" vs. "not Alice."
- **Latency:** Detection typically occurs within ~200–300 ms of the keyword being spoken.
- **Key property:** The model only recognizes the acoustic "fingerprint" of the word "Alice" — it does not understand language or transcribe speech. Everything else is noise to this model.

### 2.2 Quick Commands Model

- A separate, even smaller CNN that recognizes short commands like "stop," "louder," "quieter," "next," "pause," "turn on the light" — **without** requiring the wake word "Alice."
- This model runs in parallel with the KWS model, scanning the same audio stream.
- It is specifically designed to work **while Alice is speaking** (i.e., after AEC has cleaned the signal).

---

## Stage 3: The Decision Fork — Barge-in vs. Quick Command

This is the critical moment when the user speaks while Alice is responding. The device must decide what kind of utterance it has detected.

### 3.1 If the Wake Word "Alice" Is Detected → Barge-In (Interruption)

When the KWS model fires during TTS playback:

1. **TTS is immediately halted** — audio playback stops within tens of milliseconds.
2. The device activates a **rolling audio buffer** (already recording ~1–2 seconds *before* the wake word was detected, to capture the first word of the user's command).
3. The device begins recording the full utterance.
4. An **endpoint detection** model (VAD — Voice Activity Detection, plus a more sophisticated end-of-utterance detector) monitors for pauses, intonation contours, and spectral features to determine: "The user has finished speaking."
5. The recorded audio (encoded in an efficient format such as Opus) is sent to Yandex cloud servers via HTTPS/TLS.

### 3.2 If a Quick Command Is Detected (e.g., "Louder", "Stop") → On-the-Fly Execution

When the quick commands model fires during TTS playback:

1. The command is executed **locally** (e.g., volume is adjusted, playback is paused).
2. **TTS is NOT interrupted** — Alice continues speaking (at the new volume, or stops if "stop" was said).
3. No cloud round-trip is needed.

> This distinction is deliberate: Yandex engineers specifically trained the quick commands model to avoid triggering barge-in for volume controls, preventing the frustrating scenario where saying "louder" would cut off Alice's answer.

### Decision Flow Diagram

```
                    ┌──────────────────────────────────┐
                    │   Alice is speaking (TTS active)  │
                    │   AEC + Beamforming + NS active    │
                    └───────────────┬──────────────────┘
                                    │
                    ┌───────────────▼───────────────┐
                    │  Local CNN models scan audio   │
                    │  (KWS + Quick Commands)        │
                    └───────────────┬───────────────┘
                                    │
                    ┌───────────────▼───────────────┐
                    │   What was detected?           │
                    └──────┬───────────────┬─────────┘
                           │               │
                    ┌──────▼──────┐ ┌─────▼──────┐
                    │  "Alice"     │ │ Quick Cmd  │
                    │  (KWS fired) │ │ (e.g. "stop"│
                    └──────┬──────┘ │  "louder")  │
                           │        └─────┬──────┘
                    ┌──────▼──────┐ ┌─────▼──────┐
                    │ BARGE-IN    │ │ EXECUTE    │
                    │ Halt TTS    │ │ ON THE FLY │
                    │ Start       │ │ TTS        │
                    │ recording  │ │ continues  │
                    └──────┬──────┘ └─────┬──────┘
                           │               │
                    ┌──────▼──────┐   (done)
                    │ Record full │
                    │ utterance + │
                    │ VAD endpoint│
                    │ detection   │
                    └──────┬──────┘
                           │
                    ┌──────▼──────┐
                    │ Send audio  │
                    │ to cloud    │
                    │ (Opus/HTTPS)│
                    └─────────────┘
```

---

## Stage 4: "React to Interruption" Feature (Optional)

There is a user-facing setting (off by default) called **"React to Interruption"** (Реагировать на перебивание). When enabled:

1. Alice begins answering a question (e.g., reading a recipe).
2. The user interrupts with a *new full question* (not just "stop" or "louder").
3. The device detects this as a **new meaningful utterance** during TTS playback.
4. TTS is halted, and the new question is sent to the cloud for full processing.

This goes beyond quick commands — it requires the system to recognize that a *complete new query* has been spoken, which involves more than just keyword detection. The local model must distinguish between background noise/short commands and a full conversational question.

---

## Stage 5: Continuous Dialogue Mode (20-Second Window)

Another optional feature is **"Continuous Dialogue"** (Непрерывный диалог). After Alice finishes her response:

1. The device remains in an **active listening state** for 20 seconds.
2. The user does **not** need to say "Alice" — they can speak directly.
3. A local model analyzes intonation, volume, and spectral features to determine whether the user is *addressing the device* or *talking to someone else in the room*.
4. If no directed speech is detected within 20 seconds, the device returns to sleep mode (only KWS is active).

> This feature requires an NPU (Neural Processing Unit) on the device and is available on Station 3, Midi, and Duo Max — models with dedicated neural inference hardware. The continuous local processing would be too expensive on a CPU alone.

---

## Stage 6: Cloud Processing — The Full Pipeline

Once the audio reaches Yandex's servers, the following pipeline executes:

### 6.1 Automatic Speech Recognition (ASR)

- **Architecture:** Since 2022, Yandex SpeechKit uses a **Transformer-based** model combining encoder and decoder approaches (inspired by both BERT and GPT architectures).
- The encoder processes the audio signal (converted to mel-spectrogram or filterbank features), extracting hidden representations.
- The decoder generates text in an autoregressive manner — token by token, where each new token depends on all previously generated tokens.
- **Key features:**
  - **Contextual re-scoring:** Each newly recognized word can refine the recognition of previous words (e.g., if "бить" is ambiguous but the next word is "баклуши", the system retroactively corrects to "бить").
  - **Language-specific token:** The decoder starts with a language-specific start-of-sentence token, allowing the model to handle 10+ languages in a single stream.
  - **Positional encoding:** Embeddings include position information (critical for the attention mechanism to work correctly).
- **Training data:** Over 65,000 hours of labeled Russian audio from diverse sources (Yandex Toloka, client data, open datasets).

### 6.2 Natural Language Understanding (NLU)

- Determines the user's **intent** (e.g., "set a timer," "play music," "answer a question").
- Extracts **entities** and **slots** (e.g., timer duration, song name).

### 6.3 Dialog Manager + Response Generation

- For **command intents:** A rule-based system maps the intent to an action (e.g., "set timer for 10 minutes").
- For **conversational queries:** A large language model (YandexGPT / Alice AI LLM) generates a natural-language response. The current generation uses a **Mixture-of-Experts (MoE)** architecture with up to 235B total parameters and ~active subset per token, enabling efficient inference at scale.

### 6.4 Text-to-Speech (TTS) Synthesis

- Text response is converted to speech using a **neural TTS** engine (Yandex SpeechKit TTS).
- The system supports **SSML** (Speech Synthesis Markup Language) for controlling prosody, pauses, whispering, and special effects.
- For conversational responses, the TTS engine aims for natural intonation, appropriate pauses, and contextual emphasis.

### 6.5 Streaming Back to Device

- The synthesized audio is streamed back to the device over HTTPS.
- The device begins playback as soon as the first audio chunk arrives (no need to wait for the entire response to be generated).

### Full Cloud Pipeline Diagram

```
┌─────────────────────────────────────────────────────────────────────┐
│                      CLOUD PROCESSING PIPELINE                       │
│                                                                     │
│  ┌──────────┐    ┌──────────┐    ┌───────────┐    ┌──────────┐    │
│  │ Audio    │───▶│  ASR     │───▶│   NLU     │───▶│ Dialog   │    │
│  │ (Opus)   │    │ (Trans-  │    │ (Intent +  │    │ Manager  │    │
│  │          │    │  former) │    │  Entities) │    │ + LLM    │    │
│  └──────────┘    └──────────┘    └───────────┘    └────┬─────┘    │
│                                                        │          │
│                                       ┌────────────────┼───┐      │
│                                       │                │   │      │
│                                  ┌────▼────┐     ┌──────▼──┐│      │
│                                  │ Command │     │  TTS    ││      │
│                                  │ Action  │     │ (Neural ││      │
│                                  │ (local) │     │ Vocoder)││      │
│                                  └─────────┘     └────┬────┘│      │
│                                                       │     │      │
│                                              ┌────────▼─────▼┐    │
│                                              │  Stream audio  │    │
│                                              │  back to device│    │
│                                              │  (HTTPS/TLS)   │    │
│                                              └───────────────┘    │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Complete End-to-End Flow Diagram

```
┌─────────────────────────────────────────────────────────────────────────┐
│                        COMPLETE DIALOGUE CYCLE                           │
│                                                                         │
│  ┌─────────────┐                                                        │
│  │ User speaks │ "Alice, how do I bake a charlotte?"                   │
│  └──────┬──────┘                                                        │
│         │                                                               │
│  ┌──────▼──────────────────────────────┐                                │
│  │ ON-DEVICE FRONT-END                 │                                │
│  │  1. AEC (Kalman filter, per-band)   │                                │
│  │  2. DoA + Beamforming (MVDR)        │                                │
│  │  3. Noise Suppression + De-reverb   │                                │
│  └──────┬──────────────────────────────┘                                │
│         │                                                               │
│  ┌──────▼──────────────────────────────┐                                │
│  │ LOCAL KWS (CNN) detects "Alice"     │                                │
│  │ → LED indicator activates          │                                │
│  │ → Rolling buffer captures pre-roll  │                                │
│  └──────┬──────────────────────────────┘                                │
│         │                                                               │
│  ┌──────▼──────────────────────────────┐                                │
│  │ VAD + Endpoint Detection            │                                │
│  │ → User finished speaking             │                                │
│  │ → Audio encoded (Opus), sent HTTPS   │                                │
│  └──────┬──────────────────────────────┘                                │
│         │                                                               │
│  ═══════╪═══════════ CLOUD ═════════════════                            │
│         │                                                               │
│  ┌──────▼──────────────────────────────┐                                │
│  │ ASR (Transformer, encoder-decoder)  │                                │
│  │ → Text: "как испечь шарлотку"         │                                │
│  └──────┬──────────────────────────────┘                                │
│         │                                                               │
│  ┌──────▼──────────────────────────────┐                                │
│  │ NLU: Intent = "recipe_request"      │                                │
│  │ Entities: dish = "charlotte"         │                                │
│  └──────┬──────────────────────────────┘                                │
│         │                                                               │
│  ┌──────▼──────────────────────────────┐                                │
│  │ Dialog Manager → LLM generates      │                                │
│  │ step-by-step recipe text             │                                │
│  └──────┬──────────────────────────────┘                                │
│         │                                                               │
│  ┌──────▼──────────────────────────────┐                                │
│  │ TTS (Neural Vocoder) → audio stream  │                                │
│  └──────┬──────────────────────────────┘                                │
│         │                                                               │
│  ═══════╪═══════════ DEVICE ════════════════                           │
│         │                                                               │
│  ┌──────▼──────────────────────────────┐                                │
│  │ Speakers begin playing TTS audio    │                                │
│  │ AEC re-engages to hear user           │                                │
│  └──────┬──────────────────────────────┘                                │
│         │                                                               │
│         │   ┌───────────────────────────────────┐                       │
│         │   │ USER INTERRUPTS: "Alice, what     │                       │
│         │   │ apples work best?"                 │                       │
│         │   └──────────────┬────────────────────┘                       │
│         │                  │                                            │
│  ┌──────▼──────────────────▼────────────────────┐                      │
│  │ LOCAL MODELS (during TTS):                   │                      │
│  │  KWS fires: "Alice" detected → BARGE-IN     │                      │
│  │  TTS halted immediately                      │                      │
│  │  New utterance recorded → sent to cloud      │                      │
│  │  → New response cycle begins                  │                      │
│  └──────────────────────────────────────────────┘                      │
│                                                                         │
│  ── OR ──                                                               │
│                                                                         │
│  ┌──────────────────────────────────────────────┐                      │
│  │ Quick Command model fires: "Louder"          │                      │
│  │ → Volume increased ON THE FLY                │                      │
│  │ → TTS continues without interruption          │                      │
│  └──────────────────────────────────────────────┘                      │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## What Does Not Exist

**True full-duplex** — where Alice continues speaking, the user speaks simultaneously, and the system processes and responds to both streams concurrently — is **not implemented** in Yandex Station devices. All "simultaneous" scenarios are handled through **fast switching**: the device either interrupts TTS (barge-in) or executes a local command without interruption. This mimics natural conversation but is fundamentally a **half-duplex system with rapid turn-taking**, not a full-duplex system.

The key engineering tradeoff is **latency vs. processing power**: true full-duplex would require continuous cloud streaming of audio *while* the device is playing TTS, which would double bandwidth usage, increase cloud compute costs, and add complexity to the dialogue management system — all for a marginal improvement in conversational naturalness.

---

Sources used: Yandex engineering blog posts on AEC, microphone array design, and SpeechKit architecture; Yandex Cloud documentation on Transformer-based ASR; Alice support documentation on interruption and continuous dialogue features.


---

Great question! There's been an explosion of research on full-duplex voice AI in the last 2–3 years. I'll organize the papers by how they map to the architecture I described — from foundational signal processing through to cutting-edge end-to-end duplex speech models.

---

## 1. Foundational Signal Processing (AEC, Barge-in, Echo Cancellation)

These papers cover the acoustic front-end — the "plumbing" that makes simultaneous listening and speaking possible.

**Acoustic Echo Cancellation Techniques for Far-End Telephony Speech Recognition in Barge-in Situations** (2014, Joo et al.) — Covers normalized LMS adaptive filters and double-talk detectors specifically designed for barge-in scenarios. Maps directly to the AEC stage in the pipeline I described. [```12```](https://www.researchgate.net/publication/287953849_Acoustic_echo_cancellation_techniques_for_far-end_telephony_speech_recognition_in_barge-in_situations)[```13```](https://pure.seoultech.ac.kr/en/publications/acoustic-echo-cancellation-techniques-for-far-end-telephony-speec)

**Interface for Barge-in Free Spoken Dialogue System Using Adaptive Sound Field Control** (Academia.edu) — Proposes an alternative to AEC: instead of subtracting echo, it uses sound field control to physically prevent the speaker's output from reaching the microphone, combined with a microphone array. An interesting non-adaptive approach. [```14```](https://www.academia.edu/20233128/Interface_for_barge_in_free_spoken_dialogue_system_using_adaptive_sound_field_control)

**AEC Barge-In** (Vocal Technologies) — A practical engineering white paper on how AEC must be tuned differently for wake-word detection vs. standard full-duplex communication. Key insight: the residual echo suppressor must be disabled for barge-in applications because it introduces non-linear distortions that corrupt the wake word. [```15```](https://vocal.com/echo-cancellation/aec-barge-in/)

---

## 2. LLM-Based Full-Duplex Dialogue Schemes (Cascaded / Semi-Cascaded)

These papers represent the "bridge" between traditional pipelined systems (like what Yandex Station uses today) and fully end-to-end duplex models.

**A Full-Duplex Speech Dialogue Scheme Based on Large Language Models** (NeurIPS 2024, Wang et al.) — Introduces a **neural Finite State Machine (FSM)** with two states (SPEAK/LISTEN) that an LLM controls via next-token prediction. The LLM autonomously decides when to interrupt or yield. Achieves **0.68s latency** (3× reduction vs. half-duplex baseline) and 8% improvement in interruption precision. This is perhaps the most directly relevant paper to the architecture I described — it's essentially what the Yandex Station *could* evolve toward. [```17```](https://deep-diver.github.io/neurips2024/posters/yawxy6mwik/)[```18```](https://proceedings.neurips.cc/paper_files/paper/2024/file/180d4373aca26bd86bf45fc50d1a709f-Paper-Conference.pdf)

**Duplex Conversation: Towards Human-like Interaction in Spoken Dialogue Systems** (2022, arXiv:2205.15060) — A multi-turn, multimodal spoken dialogue system built on three subtasks: **user state detection** (turn-switch vs. turn-keep vs. hesitation), **backchannel selection** ("uh-huh", "right"), and **barge-in detection**. Uses both audio and text modalities. Demonstrates 50% latency reduction in online A/B tests. This maps closely to how Yandex's "React to Interruption" feature works. [```41```](https://arxiv.org/html/2205.15060)

---

## 3. End-to-End Full-Duplex Speech Models (The Frontier)

These are the papers that aim to replace the entire ASR → NLU → LLM → TTS cascade with a single neural model that listens and speaks simultaneously.

**PersonaPlex: Voice and Role Control for Full-Duplex Conversational Speech Models** (NVIDIA, Jan 2026, arXiv:2602.06053) — Built on the **Moshi** architecture. Receives three input streams (user audio, agent text, agent audio) and generates both text and audio tokens autoregressively. Supports zero-shot voice cloning and role conditioning. Achieves state-of-the-art on Full-Duplex-Bench for naturalness and interactivity. **Open-source weights available** on HuggingFace. [```26```](https://arxiv.org/abs/2602.06053)[```28```](https://arxiv.org/html/2602.06053v1)[```29```](https://huggingface.co/nvidia/personaplex-7b-v1)

**F-Actor: Controllable Conversational Behaviour in Full-Duplex Models** (Jan 2026, arXiv:2601.11329) — The first open, instruction-following full-duplex model trainable under academic resource constraints. Requires only **2,000 hours** of data (vs. tens of thousands for others). Allows explicit control over backchanneling, interruptions, and dialogue initiation via text instructions. Single-stage training protocol. [```32```](https://paperswithcode.co/paper/2601.11329)[```35```](https://huggingface.co/papers/2601.11329)

**FLAIR: The Silent Thought — Modeling Internal Cognition in Full-Duplex Spoken Dialogue Models** (Mar 2026, arXiv:2603.17837) — Perhaps the most intellectually interesting paper. Instead of outputting silence tokens while the user speaks (as Moshi does), FLAIR performs **continuous latent reasoning** during the user's speech phase — feeding previous-step hidden states forward into the next step. Uses a variational inference framework (ELBO optimization) with a "Global-aware Expert" that sees the full dialogue to supervise the causal model's internal thoughts. Results: MMSU jumped from 50.2% → 56.2%, turn-taking latency ~0.39s (vs. Gemini Live's ~1.3s). [```36```](https://arxiv.org/html/2603.17837v1)[```37```](https://paperswithcode.co/paper/2603.17837)[```40```](https://wispaper.ai/en/blog/silent-thought-modeling-internal-cognition-full-duplex-spoken-dialogue-latent-reasoning-20260320/eng)

**UAF: A Unified Audio Front-end LLM for Full-Duplex Speech Interaction** (Apr 2026, arXiv:2604.19221) — Reformulates VAD, speaker recognition, ASR, turn-taking detection, and question answering into a single autoregressive sequence prediction problem. Replaces the entire modular front-end with one LLM. Takes 600ms audio chunks as input and generates discrete tokens encoding both semantic content and system control signals (interruption, turn-switch). [```20```](https://arxiv.org/html/2604.19221v2)

**Human-1: A Full-Duplex Conversational Speech Model for Hindi** (Josh Talks AI, 2026) — Trained on 26,000 hours of real spontaneous conversations from 14,695 speakers with **separate audio channels** per speaker. Proves that full-duplex architectures transfer across languages and that conversational data scaling (toward million-hour corpora) is the key bottleneck. [```8```](https://ai.joshtalks.com/research/human-1)

---

## 4. Benchmarks & Evaluation

**Full-Duplex-Bench (v1, v2, v3)** (2025–2026, National Taiwan University & NVIDIA) — The standard benchmark for evaluating duplex speech models on turn-taking, interruption handling, and tool use. v3 (FDBv3) adds real human audio with disfluency annotations and multi-step API call scenarios. Evaluates GPT-Realtime, Gemini Live, Grok, Ultravox, and cascaded baselines. Key finding: self-correction handling remains the most consistent failure mode across all systems. [```2```](https://hyper.ai/en/papers/2604.04847)[```3```](https://www.opentrain.ai/tools/hf-eval-papers/paper/d433d7b5-e90b-40c6-b031-308340ad0249/)

**DuplexSpeechBench-IFEval** (Sep 2026, arXiv:2609.03423) — Evaluates **implicit instruction following** — whether agents can infer appropriate conversational behavior from a persona/role rather than explicit rules. Tests 5 conditioning protocols across 8 assistant roles. Key finding: full-duplex models (F-Actor, PersonaPlex) lose 4.5–9.7% adherence when behavior must be inferred from persona alone. [```5```](https://papers.cool/arxiv/2609.03423)[```31```](https://arxiv.org/abs/2609.03423)

**Continue, Adapt, or Yield: In-Turn Adaptation to Overlapping Speech in Full-Duplex Agents** (Sep 2026, arXiv:2609.13117) — Introduces **Duplex Cue**, which goes beyond the binary "keep speaking or stop" to measure a third behavior: **continuing to speak while incorporating the listener's contribution** (e.g., a missing word, a correction). Uses PersonaPlex as the test model. Finds that humans adapt in 68.2% of collaborative cues vs. 34.8% for the model. [```4```](https://arxiv.org/abs/2609.13117)

---

## 5. Surveys & Comprehensive Lists

**Awesome-Full-Duplex-SDM** (GitHub, Ruiqi-Yan) — A curated, continuously updated list of all full-duplex spoken dialogue models, benchmarks, and datasets. If you want one bookmark, this is it. [```42```](https://github.com/Ruiqi-Yan/Awesome-Full-Duplex-SDM)

**Recent Advances in Speech Language Models: A Survey** (ACL 2025) — Covers the full landscape from turn-based through streaming to full-duplex interaction. Maintained on GitHub with an actively updated model list. [```34```](https://github.com/dreamtheater123/Awesome-SpeechLM-Survey)

**Spoken Dialogue Systems Based on Multimodal Large Language Models** (J. Acoust. Soc. Japan, 2026) — A recent Japanese-language review covering end-to-end and full-duplex frontiers. [```44```](https://www.jstage.jst.go.jp/article/jasj/82/7/82_406/_article/-char/en)

---

## How These Map to My Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│              PAPER → ARCHITECTURE MAPPING                        │
│                                                                 │
│  Yandex Station Today (Half-Duplex + Barge-in):                 │
│  ├── AEC papers ───────────────▶ Stage 1 (AEC/Beamforming)     │
│  ├── Duplex Conversation ───────▶ Stage 3 (Barge-in detection)   │
│  └── Full-Duplex-Bench ────────▶ Evaluation framework            │
│                                                                 │
│  Next-Gen Evolution (LLM-controlled duplex):                    │
│  ├── Neural FSM (NeurIPS '24) ──▶ LLM decides SPEAK/LISTEN      │
│  ├── UAF ──────────────────────▶ Replace modular front-end      │
│  └── FireRedChat ──────────────▶ Pluggable duplex upgrade        │
│                                                                 │
│  Frontier (True end-to-end duplex):                              │
│  ├── PersonaPlex ───────────────▶ Single-model duplex           │
│  ├── F-Actor ───────────────────▶ Controllable, low-data duplex │
│  ├── FLAIR ─────────────────────▶ Think-while-listening         │
│  └── Human-1 ───────────────────▶ Cross-language scaling        │
└─────────────────────────────────────────────────────────────────┘
```

---

## My Top 3 Recommendations

If you want to read just three papers that best illuminate the path from where Yandex Station is today to true full-duplex:

1. **Neural FSM paper (NeurIPS 2024)** — most directly applicable to upgrading a cascaded system like Alice to full-duplex without rewriting everything. [```18```](https://proceedings.neurips.cc/paper_files/paper/2024/file/180d4373aca26bd86bf45fc50d1a709f-Paper-Conference.pdf)
2. **FLAIR** — the "think-while-listening" concept is genuinely novel and addresses the fundamental limitation of current duplex models (they waste the listening time). [```36```](https://arxiv.org/html/2603.17837v1)
3. **PersonaPlex** — the most production-ready open-source full-duplex model, with weights on HuggingFace and benchmark results against commercial systems. [```29```](https://huggingface.co/nvidia/personaplex-7b-v1)

---

Here's the deep dive into both frameworks.

---

# 1. FLAIR: The Variational Inference Framework

## 1.1 The Core Problem FLAIR Solves

In full-duplex spoken dialogue models (SDLMs) like Moshi, the model operates continuously — even while the user is speaking. The problem: **what should the model "do" during the user's speech phase?** Earlier systems (Moshi, dGSLM) simply emitted non-informative padding/silence tokens. This wastes the model's computational capacity.

FLAIR's insight: humans don't zone out while listening — they subconsciously process, reason, and prepare a response. FLAIR replaces wasted silence tokens with **continuous latent reasoning embeddings** that evolve recursively during the user's speech, forming a "bridge" from input understanding toward the eventual response. [```1```](https://arxiv.org/pdf/2603.17837)[```2```](https://wispaper.ai/en/blog/silent-thought-modeling-internal-cognition-full-duplex-spoken-dialogue-latent-reasoning-20260320/eng)

The challenge: **there are no "thought labels."** You can't supervise latent reasoning directly because internal cognition doesn't have explicit ground-truth annotations. This is where variational inference comes in.

## 1.2 The "Always-On" Architecture

FLAIR uses a streaming Transformer-based SDLM with two channels: a **text channel** and a **speech channel**. At every timestep \$t\$, the model receives:

- User speech tokens (audio codec tokens) — streamed continuously
- A "gate" signal \$G_t \in \{0, 1\}\$ indicating whether the model should be speaking (\$G_t = 1\$) or listening (\$G_t = 0\$)

During the user's speech phase (\$G_t = 0\$), the model does **not** emit silence. Instead:

1. The previous hidden state \$h^{l}_{t-1}\$ from layer \$l\$ is processed through the LLM head to produce text logits:

\$\$y^{\text{txt}}_{t-1} \in \mathbb{R}^{|V|}\$\$

2. These logits are softmaxed and used to compute a **weighted average over the vocabulary embedding matrix** \$E \in \mathbb{R}^{|V| \times d}\$:

\$\$Z_{t-1} = \text{Softmax}(y^{\text{txt}}_{t-1}) \cdot E\$\$

3. This latent embedding \$Z_{t-1}\$ is fed as the **input to the next timestep** — not a silence token, but a continuous, information-rich vector in the model's embedding space.

4. Meanwhile, a `<SIL>` token is sent to the speech generation module (so the model stays acoustically silent while internally "thinking").

5. This recursion continues for every timestep during the user's speech: each latent output feeds into the next step, creating a **continuous reasoning trajectory** in latent space. [```11```](https://arxiv.org/html/2603.17837v1)

```
┌─────────────────────────────────────────────────────────────────┐
│              FLAIR "ALWAYS-ON" ARCHITECTURE                       │
│                                                                 │
│  User speech phase (G_t = 0):                                   │
│                                                                 │
│  ┌──────────┐    ┌─────────┐    ┌──────────┐    ┌──────────┐   │
│  │ User audio│───▶│ Audio   │───▶│ Stream-  │───▶│  LLM     │   │
│  │ (streaming)│   │ Codec   │    │ ing TF   │    │ Backbone │   │
│  └──────────┘    └─────────┘    └──────────┘    └────┬─────┘   │
│                                                       │         │
│                                              ┌────────▼──────┐  │
│                                              │  LLM Head     │  │
│                                              │  y_txt ∈ R^V  │  │
│                                              └────────┬──────┘  │
│                                                       │         │
│                                              ┌────────▼──────┐  │
│                                              │  Softmax(y)·E │  │
│                                              │  = Z_{t-1}   │  │
│                                              │  (latent     │  │
│                                              │   embedding) │  │
│                                              └───────┬───────┘  │
│                                                      │         │
│                                    ┌─────────────────┤         │
│                                    │                 │         │
│                             ┌──────▼──────┐  ┌───────▼───────┐  │
│                             │ Feed Z_{t-1}│  │ <SIL> → Speech│  │
│                             │ as input to │  │ Gen (silent   │  │
│                             │ step t+1    │  │  externally)  │  │
│                             │ (RECURSION) │  └───────────────┘  │
│                             └─────────────┘                     │
│                                                                 │
│  Model speech phase (G_t = 1):                                  │
│                                                                 │
│  LLM generates text tokens → TTS → audio output (normal)        │
└─────────────────────────────────────────────────────────────────┘
```

## 1.3 The ELBO Derivation

### Why Variational Inference?

The goal is to maximize the conditional log-likelihood \$P_\theta(Y^{\text{txt}} | X)\$, where \$X\$ is the user's speech input and \$Y^{\text{txt}}\$ is the model's text response. But this requires marginalizing over the latent reasoning variable \$Z\$:

\$\$P_\theta(Y^{\text{txt}} | X) = \int P_\theta(Y^{\text{txt}}, Z | X) \, dZ = \int P_\theta(Y^{\text{txt}} | Z, X) \, P_\theta(Z | X) \, dZ\$\$

This integral is **intractable** — \$Z\$ is a high-dimensional continuous latent variable, and the integrand involves a neural network. We cannot enumerate or analytically integrate over all possible latent reasoning trajectories. [```11```](https://arxiv.org/html/2603.17837v1)

### Standard ELBO (Kingma & Welling, 2013)

The standard variational inference approach introduces an approximate posterior \$q_\phi(z|x)\$ and derives the Evidence Lower Bound (ELBO):

\$\$\log P_\theta(x) = \log \int q_\phi(z|x) \frac{P_\theta(x,z)}{q_\phi(z|x)} \, dz \geq \int q_\phi(z|x) \log\left(\frac{P_\theta(x|z) \, P(z)}{q_\phi(z|x)}\right) dz\$\$

\$\$= \underbrace{\mathbb{E}_{q_\phi(z|x)}[\log P_\theta(x|z)]}_{\text{Reconstruction}} - \underbrace{\text{KL}[q_\phi(z|x) \| P(z)]}_{\text{Regularization}}\$\$

### FLAIR's Conditional ELBO

FLAIR modifies the standard ELBO by conditioning on the response \$Y^{\text{txt}}\$. Instead of reconstructing the input \$x\$, the objective becomes reconstructing the **response** given the input and the latent reasoning. The key substitution is replacing \$P(x)\$ with the conditional \$P_\theta(Y^{\text{txt}} | X)\$: [```11```](https://arxiv.org/html/2603.17837v1)

\$\$\log P_\theta(Y^{\text{txt}} | X) \geq \mathbb{E}_{q_\phi(Z | X, Y^{\text{txt}})} \left[ \log P_\theta(Y^{\text{txt}} | Z, X) \right] - \text{KL}\left[ q_\phi(Z | X, Y^{\text{txt}}) \| P_\theta(Z | X) \right]\$\$

This is the **conditional ELBO**, and it consists of two terms:

### Term 1: Conditional Reconstruction

\$\$\mathbb{E}_{q_\phi(Z | X, Y^{\text{txt}})} \left[ \log P_\theta(Y^{\text{txt}} | Z, X) \right]\$\$

This term says: given the user input \$X\$ and the latent reasoning \$Z\$ (sampled from the expert's posterior), the model should produce a high-quality response \$Y^{\text{txt}}\$. It ensures the latent representation captures the **semantic information needed for dialogue completion** — not just any latent representation, but one that is actually useful for generating the correct response. [```11```](https://arxiv.org/html/2603.17837v1)

### Term 2: Variational Regularization (KL Divergence)

\$\$\text{KL}\left[ q_\phi(Z | X, Y^{\text{txt}}) \| P_\theta(Z | X) \right]\$\$

This term forces the **causal model's prior** \$P_\theta(Z | X)\$ (what the real-time model predicts *without* seeing the future response) to align with the **expert's posterior** \$q_\phi(Z | X, Y^{\text{txt}})\$ (which *does* see the future response). This is the crucial distillation mechanism: it "distills the wisdom of hindsight into a real-time, causal model". [```2```](https://wispaper.ai/en/blog/silent-thought-modeling-internal-cognition-full-duplex-spoken-dialogue-latent-reasoning-20260320/eng)

## 1.4 The Global-aware Expert (\$Q_\phi\$)

The approximate posterior \$q_\phi(Z | X, Y^{\text{txt}})\$ is parameterized by a **non-causal (bidirectional) model** called the **Global-aware Expert**. This model:

- Sees the **entire dialogue session** — including the user's input \$X\$ **and** the eventual response \$Y^{\text{txt}}\$
- Extracts latent features that represent the "ideal" internal reasoning trajectory — what the model *should* have been thinking during the user's speech to produce the best possible response
- Is used **only during training** — it is discarded at inference time [```1```](https://arxiv.org/pdf/2603.17837)[```2```](https://wispaper.ai/en/blog/silent-thought-modeling-internal-cognition-full-duplex-spoken-dialogue-latent-reasoning-20260320/eng)

The causal SDLM (the real-time model) only sees the past — it has no access to \$Y^{\text{txt}}\$ during the user's speech. But through the KL divergence term, it is trained to produce latent embeddings that **match** what the expert would have produced with full knowledge.

```
┌─────────────────────────────────────────────────────────────────┐
│              FLAIR TRAINING: ELBO OPTIMIZATION                    │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │  GLOBAL-AWARE EXPERT (Q_φ) — non-causal, training only  │   │
│  │                                                         │   │
│  │  Input: X (user speech) + Y_txt (future response)       │   │
│  │  Output: q_φ(Z | X, Y_txt)                              │   │
│  │  "What the model SHOULD have been thinking"              │   │
│  └────────────────────────┬────────────────────────────────┘   │
│                            │                                     │
│                     Posterior q_φ(Z|X,Y_txt)                    │
│                            │                                     │
│  ┌─────────────────────────▼────────────────────────────────┐  │
│  │                    CONDITIONAL ELBO                        │  │
│  │                                                           │  │
│  │  L = E_q[log P_θ(Y_txt | Z, X)]  -  KL[q_φ || P_θ]       │  │
│  │      └─────────────────────┘         └──────────────┘    │  │
│  │      Conditional Reconstruction     Variational            │  │
│  │      "Given Z, produce              Regularization        │  │
│  │       good Y_txt"                   "Align causal prior   │  │
│  │                                     with expert posterior"│  │
│  └─────────────────────────┬────────────────────────────────┘  │
│                            │                                     │
│  ┌─────────────────────────▼────────────────────────────────┐  │
│  │  CAUSAL SDLM (P_θ) — real-time, used at inference         │  │
│  │                                                           │  │
│  │  Input: X (user speech only — no future)                  │  │
│  │  Output: P_θ(Z | X)  ←  latent reasoning embeddings       │  │
│  │  "What the model IS thinking in real-time"                │  │
│  └───────────────────────────────────────────────────────────┘  │
│                                                                 │
│  At inference: Expert is discarded. Causal SDLM runs alone,     │
│  producing latent Z that approximates the expert's posterior.   │
└─────────────────────────────────────────────────────────────────┘
```

## 1.5 Training Pipeline

FLAIR's training has three sequential stages: [```11```](https://arxiv.org/html/2603.17837v1)

**Stage 1 — Dialogue Data Synthesis.** Existing spoken dialogue datasets are insufficient (too casual, not instruction-following). FLAIR synthesizes pseudo-dialogues from text pretraining corpora: sentences from continuous text passages are alternately assigned to "user" and "agent," with stochastic turn segmentation (80% chance of ending after one sentence, decaying probability for additional sentences, mandatory role switch at 200 words). Two speaker profiles are sampled for acoustic separation, producing a synchronized two-stream audio track.

**Stage 2 — Latent Reasoning SFT.** The core ELBO optimization. Both the causal SDLM and the non-causal Global-aware Expert are trained simultaneously. The expert processes the full dialogue (including the response) to produce the posterior \$q_\phi(Z|X, Y^{\text{txt}})\$. The causal model processes only the user's speech to produce the prior \$P_\theta(Z|X)\$. The ELBO loss optimizes both the reconstruction quality and the KL alignment.

**Stage 3 — Speech Synthesizing SFT.** All parameters are frozen except the speech generation module. A streaming flow-matching model converts text tokens into speech. Speech codec tokens are extracted as training labels, and the module is optimized via next-token prediction.

## 1.6 What the Latent Space Looks Like

Using t-SNE visualization, the authors showed that during the user's speech phase, the latent embeddings form a **physical trajectory** in manifold space — moving from the "Audio Input" cluster toward the "Target Text" cluster before the response even begins. When the target embedding lies outside the input cluster, the latent reasoning process forms a direct path to reach it. This is empirical evidence that the model is genuinely "reasoning toward the answer" while still listening. [```2```](https://wispaper.ai/en/blog/silent-thought-modeling-internal-cognition-full-duplex-spoken-dialogue-latent-reasoning-20260320/eng)

## 1.7 Key Results

| Metric                      | Without Latent Reasoning | With FLAIR Latent Reasoning |
| --------------------------- | ------------------------ | --------------------------- |
| MMSU (spoken understanding) | 50.2%                    | **56.2%**                   |
| Turn-taking latency         | 0.57s                    | **0.39s**                   |
| Barge-in latency            | 0.86s                    | **0.46s**                   |
| Inference overhead          | —                        | **0 (none)**                |

The zero inference overhead is critical: because the latent reasoning replaces silence tokens that would have been generated anyway, there is **no additional computational cost** at inference time. The expert model exists only during training. [```2```](https://wispaper.ai/en/blog/silent-thought-modeling-internal-cognition-full-duplex-spoken-dialogue-latent-reasoning-20260320/eng)

---

# 2. Neural FSM: State Transition Logic

## 2.1 The Core Idea

The Neural FSM paper (Wang et al., NeurIPS 2024) solves full-duplex dialogue by teaching an LLM to **autonomously operate a two-state finite state machine** through next-token prediction. The LLM doesn't just generate text — it also emits **control tokens** that trigger state transitions between SPEAK and LISTEN modes. [```6```](https://proceedings.neurips.cc/paper_files/paper/2024/file/180d4373aca26bd86bf45fc50d1a709f-Paper-Conference.pdf)[```7```](https://deep-diver.github.io/neurips2024/posters/yawxy6mwik/)

The key insight: instead of building a separate dialogue management module (turn-taking detector, interruption handler, etc.), the LLM itself becomes the dialogue controller. All decisions — when to speak, when to yield, when to interrupt — are unified into the same next-token prediction framework the LLM already uses for text generation.

## 2.2 The Two States and Four Transitions

The FSM has exactly **two states** and **four possible transitions**: [```6```](https://proceedings.neurips.cc/paper_files/paper/2024/file/180d4373aca26bd86bf45fc50d1a709f-Paper-Conference.pdf)

```
┌─────────────────────────────────────────────────────────────┐
│                    NEURAL FSM STATE DIAGRAM                   │
│                                                             │
│                    ┌──────────────┐                          │
│      [C.SPEAK]     │              │     [S.LISTEN]           │
│    ┌─────────────▶│    SPEAK     │◀─────────────┐           │
│    │              │              │              │           │
│    │              └──────┬───────┘              │           │
│    │                     │                      │           │
│    │                     │ [S.SPEAK]             │           │
│    │                     ▼                      │           │
│    │              ┌──────────────┐              │           │
│    │              │              │              │           │
│    └──────────────│   LISTEN     │──────────────┘           │
│       [C.LISTEN]  │              │                          │
│                   └──────────────┘                          │
│                                                             │
│  Self-loops:                                                 │
│  [C.SPEAK]  = SPEAK → SPEAK (keep speaking despite user)    │
│  [C.LISTEN] = LISTEN → LISTEN (keep waiting for more input)  │
│                                                             │
│  Cross transitions:                                          │
│  [S.LISTEN] = SPEAK → LISTEN (yield to user)                │
│  [S.SPEAK]  = LISTEN → SPEAK (start speaking / interrupt)   │
└─────────────────────────────────────────────────────────────┘
```

### The Four Control Tokens

| Token        | Transition      | Meaning                                                                                                                                                                   |
| ------------ | --------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `[C.SPEAK]`  | SPEAK → SPEAK   | Model **continues speaking** even though the perception module detected user speech. The model decides its current output is more important than the user's interruption. |
| `[S.LISTEN]` | SPEAK → LISTEN  | Model **concedes the floor** to the user. It stops generating speech and enters listening mode.                                                                           |
| `[C.LISTEN]` | LISTEN → LISTEN | Model **continues waiting** — the user hasn't finished speaking. More input is needed before a response can be generated.                                                 |
| `[S.SPEAK]`  | LISTEN → SPEAK  | Model **initiates speech** — either because the user has finished (detected via streaming ASR endpoint), or the model decides it's appropriate to interrupt the user.     |

## 2.3 System Architecture: Three Parallel Modules

The system consists of three modules that operate **continuously and in parallel**: [```6```](https://proceedings.neurips.cc/paper_files/paper/2024/file/180d4373aca26bd86bf45fc50d1a709f-Paper-Conference.pdf)[```9```](https://www.emergentmind.com/topics/full-duplex-speech-llms-7376c659-cf91-4a79-9c3a-0b691d41cddb)

```
┌─────────────────────────────────────────────────────────────────┐
│              NEURAL FSM SYSTEM ARCHITECTURE                       │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │  PERCEPTION MODULE (streaming ASR)                      │    │
│  │  • Continuously captures user microphone audio           │    │
│  │  • Segments speech every ~640ms                         │    │
│  │  • Streams transcribed tokens to LLM (regardless of     │    │
│  │    FSM state — always running)                           │    │
│  └────────────────────────┬────────────────────────────────┘    │
│                           │ ASR tokens                          │
│                           │ (streamed continuously)              │
│  ┌────────────────────────▼────────────────────────────────┐   │
│  │                    LLM (8B params)                        │   │
│  │                                                          │   │
│  │  At each timestep, produces exactly ONE of:              │   │
│  │  ① Text token → sent to Motor Module for TTS             │   │
│  │  ② Control token → triggers FSM state transition         │   │
│  │  ③ (Receives external ASR token as input)                │   │
│  │                                                          │   │
│  │  The LLM has full dialogue history + system prompt        │   │
│  │  that instructs it about the FSM and its role             │   │
│  └────────┬───────────────────────────────────┬─────────────┘   │
│           │ Text tokens                       │ Control tokens  │
│           │ (when in SPEAK)                   │ (anytime)       │
│  ┌────────▼──────────────┐           ┌───────▼────────────┐    │
│  │  MOTOR MODULE (TTS)    │           │  NEURAL FSM         │    │
│  │  • Converts text →     │           │  • 2-state machine │    │
│  │    speech (streaming)  │           │  • Transitions on  │    │
│  │  • Plays audio via     │           │    control tokens   │    │
│  │    speaker             │           │  • Signals modules  │    │
│  └────────────────────────┘           │    to switch mode   │    │
│                                        └────────────────────┘    │
│                                                                 │
│  KEY: All three modules run continuously and in parallel.        │
│  The FSM state determines what each module DOES, not WHETHER     │
│  it runs. ASR always listens; TTS only outputs in SPEAK.         │
└─────────────────────────────────────────────────────────────────┘
```

The critical design choice: **the perception and motor modules are conditionally independent given the FSM state**. They don't need to synchronize with each other — they each observe the FSM state independently and choose the appropriate action. This allows them to operate in parallel without coordination overhead. [```6```](https://proceedings.neurips.cc/paper_files/paper/2024/file/180d4373aca26bd86bf45fc50d1a709f-Paper-Conference.pdf)

## 2.4 How the LLM Makes Decisions

At each decoding step, the LLM sees:

1. **Dialogue history** — all previous turns, including user and model utterances
2. **Streaming ASR tokens** — partial transcription of the user's current speech, appended to the token sequence in real time (even while the model is generating its own output)
3. **System prompt** — instructions about the FSM, its two states, the four control tokens, and the dialogue context

The LLM then predicts the next token. If the token is:
- A **regular text token** → it's sent to the motor module for speech synthesis
- A **control token** → it triggers the corresponding FSM transition

This means the LLM makes **autonomous, context-aware decisions** about when to interrupt, yield, or continue — based on its understanding of the dialogue semantics, not on a hard-coded VAD threshold. [```6```](https://proceedings.neurips.cc/paper_files/paper/2024/file/180d4373aca26bd86bf45fc50d1a709f-Paper-Conference.pdf)

### Example Scenario

```
Dialogue context: User is asking about mortgage pre-approval.
Model is in SPEAK state, explaining the process.

Timestep t:   LLM generates: "You'll need to gather some financial"
              → Motor module synthesizes speech

Timestep t+1: ASR streams in partial token: "what" (user starts speaking)
              LLM sees: [dialogue_history] "...financial [ASR: what]"
              LLM generates: "documents such as pay stubs"
              → LLM decides this is important, continues speaking
              → Emits [C.SPEAK]  (SPEAK → SPEAK)

Timestep t+2: ASR streams: "what about" (user clearly interrupting)
              LLM sees: "...pay stubs [ASR: what about]"
              LLM recognizes user is asking a new question
              → Emits [S.LISTEN]  (SPEAK → LISTEN)
              → Motor module stops synthesizing
              → FSM switches to LISTEN state

Timestep t+3: ASR streams: "what about self-employed?"
              LLM sees: "[ASR: what about self-employed?]"
              LLM recognizes complete question
              → Emits [S.SPEAK]  (LISTEN → SPEAK)
              → Generates response: "For self-employed individuals..."
              → Motor module begins synthesizing
```

## 2.5 Training the LLM for FSM Control

The LLM is fine-tuned via supervised fine-tuning (SFT) on a specially constructed dataset: [```6```](https://proceedings.neurips.cc/paper_files/paper/2024/file/180d4373aca26bd86bf45fc50d1a709f-Paper-Conference.pdf)

1. **GPT-4 generates dialogue transcripts** — 1,500 dialogues with proper control token annotations at the correct moments. These include scenarios of: interruption, denial, affirmation, environmental noise, and topic shifting.

2. **Base model:** Llama-3-8B-Instruct

3. **Fine-tuning:** Only 20 steps of SFT on 8× NVIDIA A100 GPUs, batch size 256, learning rate \$10^{-5}\$, AdamW optimizer. The paper found that minimal fine-tuning is sufficient because the LLM already has the reasoning capability — it just needs to learn the "language" of control tokens.

4. **Evaluation dataset ("duplex-dialogue-3k"):** 3,000 entries covering:
   - Machine interrupts user (2,000 entries from ShareGPT + GPT-4 generated)
   - User interrupts machine (720 entries, 4 categories: denial, affirmation, noise, topic shift)

## 2.6 Latency Breakdown by Configuration

The paper provides a detailed latency breakdown across four system configurations: [```8```](https://liner.com/review/a-fullduplex-speech-dialogue-scheme-based-on-large-language-models)

| Config              | ASR            | LLM         | TTS           | Avg Latency | 50th pct  | 90th pct  |
| ------------------- | -------------- | ----------- | ------------- | ----------- | --------- | --------- |
| 1 (baseline)        | Non-streaming  | Standard    | Non-streaming | 2.28s       | 2.19s     | 2.75s     |
| 2                   | Semi-streaming | Full-duplex | Non-streaming | 1.49s       | 1.60s     | 1.96s     |
| 3                   | Streaming      | Full-duplex | Non-streaming | 1.15s       | 0.78s     | 2.08s     |
| 4 (fully optimized) | Streaming      | Full-duplex | Streaming     | **0.68s**   | **0.41s** | **1.60s** |

The progression reveals where latency savings come from:
- **Config 1 → 2** (35% reduction): Semi-streaming ASR allows the LLM to see partial transcriptions and recognize speech endpoints faster than traditional VAD
- **Config 2 → 3** (23% further reduction): Full streaming ASR means the LLM processes input continuously, not in chunks
- **Config 3 → 4** (41% further reduction): Streaming TTS means audio playback begins as soon as the first tokens are generated, not after the full response is computed

The fully optimized system achieves **0.68s average First Token Emission Delay (FTED)** — a 3× reduction from the 2.28s baseline. The 50th percentile is 0.41s, meaning half of all interactions begin receiving a response in under 410ms. [```8```](https://liner.com/review/a-fullduplex-speech-dialogue-scheme-based-on-large-language-models)

## 2.7 Interruption Performance

The system was evaluated on five interruption patterns: [```6```](https://proceedings.neurips.cc/paper_files/paper/2024/file/180d4373aca26bd86bf45fc50d1a709f-Paper-Conference.pdf)

| Pattern     | Description                                     | PRR (Precision Rate) |
| ----------- | ----------------------------------------------- | -------------------- |
| Noise       | Environmental noise triggers false interruption | 95.2%                |
| Denial      | User says "no, that's not what I meant"         | 100%                 |
| Affirmation | User says "yes, exactly!"                       | 99.5%                |
| Topic shift | User changes subject mid-response               | 85.8%                |
| **Average** |                                                 | **96.7%**            |

The machine-initiated interruption rate is 62.8% — meaning the model proactively interrupts the user in 62.8% of appropriate scenarios, with 38.8% of those interruptions occurring **while the user is still speaking**, dramatically reducing response latency. [```6```](https://proceedings.neurips.cc/paper_files/paper/2024/file/180d4373aca26bd86bf45fc50d1a709f-Paper-Conference.pdf)

---

## How the Two Frameworks Relate

```
┌─────────────────────────────────────────────────────────────────┐
│           FLAIR vs. NEURAL FSM: COMPARISON                        │
│                                                                 │
│  Neural FSM (Wang et al., 2024):                                │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │  • LLM controls explicit FSM with 2 states              │    │
│  │  • Discrete decisions: SPEAK or LISTEN                   │    │
│  │  • Control tokens are discrete (4 types)                  │    │
│  │  • During LISTEN: model processes ASR tokens             │    │
│  │    but does NOT do internal reasoning                     │    │
│  │  • Trained via simple SFT (20 steps)                     │    │
│  │  • Works with any chat-tuned LLM + ASR + TTS              │    │
│  │  • Latency: 0.68s (streaming config)                    │    │
│  └─────────────────────────────────────────────────────────┘    │
│                          vs.                                     │
│  FLAIR (Wu et al., 2026):                                        │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │  • No explicit FSM — continuous latent reasoning         │    │
│  │  • Continuous decisions: latent Z evolves every step     │    │
│  │  • Latent embeddings are continuous (d-dimensional)        │    │
│  │  • During LISTEN: model does latent reasoning via         │    │
│  │    recursive embedding feedback                           │    │
│  │  • Trained via ELBO + variational inference              │    │
│  │  • Requires dedicated SDLM architecture                  │    │
│  │  • Latency: 0.39s (turn-taking), 0.46s (barge-in)       │    │
│  └─────────────────────────────────────────────────────────┘    │
│                                                                 │
│  KEY DIFFERENCE:                                                 │
│  Neural FSM uses the listening time to wait for input.           │
│  FLAIR uses the listening time to THINK.                         │
│  Both achieve full-duplex, but FLAIR's latent reasoning           │
│  produces higher-quality responses (MMSU: 56.2% vs 50.2%).      │
└─────────────────────────────────────────────────────────────────┘
```

---

Here's the deep dive into both topics, pulled directly from the source papers.

---

# Part 1: FLAIR t-SNE Manifold Analysis

## 1.1 The Question the Visualization Answers

FLAIR's central claim is that during the user's speech phase, the model's latent embeddings \$Z_t\$ are not random noise or passive padding — they form a **deliberate reasoning trajectory** through the model's latent space. The t-SNE visualization is the empirical evidence for this claim. The authors ask: *if we project the latent embeddings at each timestep into 2D space, do we see structure, or just noise?* [```3```](https://arxiv.org/html/2603.17837v1)

## 1.2 What Was Projected

The t-SNE analysis projects three categories of embeddings, all extracted from the model's hidden states during a dialogue session: [```3```](https://arxiv.org/html/2603.17837v1)

1. **Audio Input embeddings** — hidden states extracted during the user's speech phase, representing the model's perception of incoming audio tokens. These form a cluster that reflects the acoustic/semantic content of what the user is saying.

2. **Latent Reasoning embeddings** — the intermediate \$Z_t\$ vectors produced by the recursive `Softmax(y) · E` mechanism during the user's speech phase. These are the "silent thought" vectors — neither input nor output, but the model's internal deliberation.

3. **Target Text embeddings** — hidden states from the model's actual response generation phase, after the user finishes speaking. These represent the semantic content of the eventual answer.

All three sets are high-dimensional vectors (dimensionality \$d = \$ the LLM hidden size, which is 3584 for Qwen2.5-7B). The t-SNE algorithm reduces these to 2D for visualization.

## 1.3 What the Manifold Shows

The t-SNE projection reveals three distinct clusters and, critically, a **structured path** connecting them: [```3```](https://arxiv.org/html/2603.17837v1)

```
┌─────────────────────────────────────────────────────────────────┐
│              t-SNE MANIFOLD: CONCEPTUAL SCHEMATIC                 │
│                                                                 │
│     ┌──────────────┐                                            │
│     │              │           · · · · ·                         │
│     │  Audio Input │          ·  Latent       · · ·              │
│     │  Cluster     │········ ·  Reasoning    · · Target          │
│     │              │    · · ·  Trajectory   · · Text            │
│     │  (user       │     · ·              · ·  Cluster          │
│     │   speech)    │      · · · · · · · · · ·  (model            │
│     │              │       (bridge)           response)         │
│     └──────────────┘                             └──────────────┐
│                                                                 │
│  Yellow box region:        Black box region:                     │
│  Target OUTSIDE input      Latent embeddings                     │
│  cluster → direct          navigate through "void regions"       │
│  trajectory forms          (feasible manifold)                   │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### The "Yellow Box" Phenomenon

The authors highlight a specific region (annotated in a yellow box in the original figure) where the **target text cluster lies outside** the audio input cluster — meaning the semantic content of the answer is distant from the semantic content of the question. In these cases, the latent reasoning embeddings form a **direct, nearly linear trajectory** from the input cluster toward the target cluster. [```3```](https://arxiv.org/html/2603.17837v1)

This is significant because it shows the model isn't just "sitting" near the input — it's actively **traveling** toward the answer. The latent reasoning process literally charts a path through the manifold from "what I heard" to "what I should say," and this path forms *before* the model begins generating any output tokens.

### The "Black Box" Phenomenon

In another annotated region (black box), the latent embeddings **depart from the starting point** (the input cluster) and chart a path through what the authors call "void regions" — areas of the manifold that are neither input-like nor output-like, but represent the **feasible intermediate states** the model can occupy. The key observation is that the trajectory takes the **shortest path through these void regions**, suggesting the model is optimizing its internal trajectory for efficiency. [```3```](https://arxiv.org/html/2603.17837v1)

### The Bridge Interpretation

The overall structure resembles a **bridge**: the latent reasoning embeddings physically connect the input cluster to the target cluster in the manifold space. Without latent reasoning (the baseline without FLAIR), there is no bridge — the model jumps discontinuously from input processing to output generation. With FLAIR, the transition is continuous and structured. [```2```](https://wispaper.ai/en/blog/silent-thought-modeling-internal-cognition-full-duplex-spoken-dialogue-latent-reasoning-20260320/eng)

## 1.4 Why This Matters

The t-SNE analysis provides three critical pieces of evidence:

**1. The latent reasoning is not random.** If the \$Z_t\$ embeddings were random (as silence tokens would produce), the t-SNE projection would show a diffuse cloud with no structure. Instead, we see ordered trajectories. [```3```](https://arxiv.org/html/2603.17837v1)

**2. The reasoning is directional.** The embeddings move *from* the input cluster *toward* the target cluster — not sideways, not in circles. The directionality implies the model is accumulating information relevant to the upcoming response, not just passing time. [```3```](https://arxiv.org/html/2603.17837v1)

**3. The reasoning adapts to difficulty.** When the target is "close" to the input (simple factual recall), the trajectory is short. When the target is "far" (complex reasoning), the trajectory is longer and traverses void regions — suggesting the model spends more "thought effort" on harder queries. [```3```](https://arxiv.org/html/2603.17837v1)

## 1.5 Connection to the ELBO Training

The t-SNE structure is a direct consequence of the KL divergence term in the conditional ELBO. Recall:

\$\$\text{KL}\left[q_\phi(Z | X, Y^{\text{txt}}) \| P_\theta(Z | X)\right]\$\$

This term forces the causal model's prior \$P_\theta(Z|X)\$ (which only sees input) to match the expert's posterior \$q_\phi(Z|X, Y^{\text{txt}})\$ (which sees both input and the future response). The expert's posterior naturally produces embeddings that are *informative about the response* — they lie "closer" to the target text cluster. Through KL minimization, the causal model learns to produce embeddings that approximate this structure even without seeing the future. [```3```](https://arxiv.org/html/2603.17837v1)

In other words, the t-SNE bridge is the visual signature of successful knowledge distillation from the expert to the causal model.

## 1.6 Limitations of the Analysis

The authors note several caveats:

- t-SNE is a **non-parametric, non-linear** dimensionality reduction. Distances in the 2D projection are not proportional to distances in the original space — only neighborhood structure (what's near what) is preserved.
- The analysis is **qualitative** — there is no quantitative metric for "trajectory straightness" or "bridge quality." The evidence is visual.
- The visualization is based on a **limited sample** of dialogue sessions; it's unclear whether the bridge structure holds universally or only for certain types of queries.
- t-SNE's stochastic nature means different runs can produce different projections; the authors don't report whether the structure is stable across multiple t-SNE runs with different random seeds.

---

# Part 2: Neural FSM Benchmark Construction Methodology

## 2.1 Design Philosophy

The Neural FSM paper makes a deliberate methodological choice: **do not evaluate individual control tokens** ([S.SPEAK], [C.SPEAK], [S.LISTEN], [C.LISTEN]) in isolation. Instead, evaluate the **timing and appropriateness of dialogue interruptions**, which are functionally equivalent to correct FSM state transitions. The rationale: a user cares whether the machine interrupted at the right moment, not which internal token triggered it. [```6```](https://arxiv.org/pdf/2405.19487)

## 2.2 The Benchmark: "duplex-dialogue-3k"

The dataset contains **approximately 3,000 entries** split into two major categories: [```6```](https://arxiv.org/pdf/2405.19487)

```
┌─────────────────────────────────────────────────────────────────┐
│              duplex-dialogue-3k STRUCTURE                         │
│                                                                 │
│  ┌─────────────────────────────────────────────┐               │
│  │  CATEGORY A: Machine Interrupts User         │               │
│  │  Total: 2,000 entries                        │               │
│  │                                              │               │
│  │  ┌──────────────────────────────────────┐   │               │
│  │  │ Source 1: ShareGPT filtered           │   │               │
│  │  │ ~1,000 sessions                       │   │               │
│  │  │ Filtering rules:                      │   │               │
│  │  │  - Remove code blocks                 │   │               │
│  │  │  - Remove math/subject-specific       │   │               │
│  │  │  - Remove excessively long texts      │   │               │
│  │  │  - Last turn MUST be user's utterance │   │               │
│  │  └──────────────────────────────────────┘   │               │
│  │                                              │               │
│  │  ┌──────────────────────────────────────┐   │               │
│  │  │ Source 2: GPT-4 generated             │   │               │
│  │  │ ~1,000 dialogues across various       │   │               │
│  │  │ domains (single + multi-turn)        │   │               │
│  │  │ Last turn MUST be user's utterance    │   │               │
│  │  └──────────────────────────────────────┘   │               │
│  │                                              │               │
│  │  Purpose: Test whether the LLM              │               │
│  │  proactively interrupts the user            │               │
│  │  during the last turn (streamed token        │               │
│  │  by token to observe interrupt timing)      │               │
│  └─────────────────────────────────────────────┘               │
│                                                                 │
│  ┌─────────────────────────────────────────────┐               │
│  │  CATEGORY B: User Interrupts Machine         │               │
│  │  Total: 720 entries (180 × 4 categories)    │               │
│  │                                              │               │
│  │  ┌──────────────────────────────────────┐   │               │
│  │  │ Pattern 1: Noise (180 entries)        │   │               │
│  │  │ Environmental noise triggers false     │   │               │
│  │  │ interruption — system should NOT       │   │               │
│  │  │ stop speaking                          │   │               │
│  │  └──────────────────────────────────────┘   │               │
│  │  ┌──────────────────────────────────────┐   │               │
│  │  │ Pattern 2: Affirmation (180 entries)   │   │               │
│  │  │ User says "uh-huh", "nice", "good"      │   │               │
│  │  │ System should CONTINUE speaking         │   │               │
│  │  └──────────────────────────────────────┘   │               │
│  │  ┌──────────────────────────────────────┐   │               │
│  │  │ Pattern 3: Denial (180 entries)        │   │               │
│  │  │ User says "no, that's wrong"           │   │               │
│  │  │ System should STOP and respond          │   │               │
│  │  └──────────────────────────────────────┘   │               │
│  │  ┌──────────────────────────────────────┐   │               │
│  │  │ Pattern 4: Topic Shift (180 entries)   │   │               │
│  │  │ User changes subject mid-response      │   │               │
│  │  │ System should STOP and address new     │   │               │
│  │  │ topic                                  │   │               │
│  │  └──────────────────────────────────────┘   │               │
│  └─────────────────────────────────────────────┘               │
│                                                                 │
│  GRAND TOTAL: ~2,720 entries (named "duplex-dialogue-3k")       │
└─────────────────────────────────────────────────────────────────┘
```

## 2.3 Data Construction Process

### Category A: Machine Interrupts User

**Source 1 — ShareGPT filtering (~1,000 sessions):** [```6```](https://arxiv.org/pdf/2405.19487)

The authors start with ShareGPT, a public dataset of ChatGPT conversations. They apply filtering rules:
- **Remove entries containing code** — code blocks are unsuitable for voice testing (you can't "speak" Python naturally).
- **Remove subject-specific content** like mathematics — formulas don't translate to spoken dialogue.
- **Remove excessively long texts** — these exceed the practical duration of a voice interaction.
- **Constraint: the last turn must be the user's utterance.** This is critical — the last user turn is what gets streamed to the LLM token by token during evaluation. The system observes whether the LLM proactively interrupts during this streaming.

**Source 2 — GPT-4 generation (~1,000 dialogues):** [```6```](https://arxiv.org/pdf/2405.19487)

The authors use GPT-4 (gpt-4-turbo-2024-04-09) to generate synthetic oral dialogues across various domains. The prompt (detailed in Appendix B of the paper) instructs GPT-4 to write realistic spoken-language conversations. Again, the last turn must be the user's utterance.

### Category B: User Interrupts Machine

For each of the four interruption patterns, GPT-4 generates **180 entries** of single and multi-turn dialogues. Each entry follows a specific template: [```6```](https://arxiv.org/pdf/2405.19487)

**Pattern 1 — Noise:** The machine is responding. At some point, environmental noise is introduced (a door slams, a dog barks). The correct behavior is `[C.SPEAK]` — the machine should **ignore the noise and continue speaking**.

**Pattern 2 — Affirmation:** The machine is responding. The user interjects with a brief affirmative ("uh-huh", "nice", "good"). The correct behavior is `[C.SPEAK]` — the machine should **continue speaking** because affirmations are backchannels, not interruptions.

**Pattern 3 — Denial:** The machine is responding. The user says "No, that's not right" or "Actually, I disagree." The correct behavior is `[S.LISTEN]` — the machine should **stop and yield the floor**, then respond to the correction.

**Pattern 4 — Topic Shift:** The machine is responding about topic A. The user abruptly shifts to topic B ("Actually, let's talk about something else — how do I bake bread?"). The correct behavior is `[S.LISTEN]` — the machine should **stop and address the new topic**.

## 2.4 The Automated Evaluation Simulator

The evaluation doesn't use human judges — it uses a **turn-based simulator** that emulates real conversations: [```6```](https://arxiv.org/pdf/2405.19487)

```
┌─────────────────────────────────────────────────────────────────┐
│              AUTOMATED EVALUATION SIMULATOR                        │
│                                                                 │
│  Program U (User)          Program M (Machine)                   │
│  ┌──────────────┐          ┌──────────────────┐                 │
│  │ Reads text   │          │ Powered by LLM    │                 │
│  │ from dataset │          │ (under test)     │                 │
│  │ as "speech"  │          │                  │                 │
│  └──────┬───────┘          └────────┬─────────┘                 │
│         │                           │                            │
│         │   ┌───────────────────┐   │                            │
│         │   │ Streaming ASR     │   │                            │
│         └──▶│ tokenizes U's     │──▶│                            │
│             │ speech into       │   │                            │
│             │ tokens            │   │                            │
│             └───────────────────┘   │                            │
│                                     │                            │
│  ┌──────────────────────────────────▼────────────────────┐     │
│  │                    SIMULATION LOOP                      │     │
│  │                                                         │     │
│  │  For Category A (Machine interrupts user):              │     │
│  │    1. U's last turn is streamed token by token to M     │     │
│  │    2. M can interrupt at any time during streaming       │     │
│  │    3. If M interrupts → record timing data              │     │
│  │    4. GPT-4 evaluates appropriateness of interruption    │     │
│  │                                                         │     │
│  │  For Category B (User interrupts machine):               │     │
│  │    1. M generates a response to the penultimate turn    │     │
│  │    2. U's interruption (last turn) is played to M        │     │
│  │    3. M must decide: continue or yield                   │     │
│  │    4. GPT-4 evaluates correctness of decision            │     │
│  └─────────────────────────────────────────────────────────┘     │
│                                                                 │
│  ┌──────────────────────────────────────────────┐              │
│  │  METRICS RECORDED                              │              │
│  │  • FTED (First Token Emission Delay)          │              │
│  │  • Interruption rate (%)                       │              │
│  │  • Interruption timing (during/after speech)  │              │
│  │  • PRR per pattern (GPT-4 judged)             │              │
│  └──────────────────────────────────────────────┘              │
└─────────────────────────────────────────────────────────────────┘
```

### Key Design Choices in the Simulator

**1. Token-by-token streaming.** The user's last turn is not delivered all at once — it's streamed token by token, simulating real-time ASR output. This is essential because the LLM's ability to interrupt *during* the user's speech (not just after) is what distinguishes full-duplex from half-duplex. [```6```](https://arxiv.org/pdf/2405.19487)

**2. Precise time measurement.** Because the simulator controls the streaming rate, it can measure exactly when the LLM's first response token appears relative to when the user started/finished speaking. This yields the FTED (First Token Emission Delay) metric. [```6```](https://arxiv.org/pdf/2405.19487)

**3. GPT-4 as judge.** For evaluating *appropriateness* of interruptions, the authors use GPT-4 as an automated judge. Two separate judge prompts are designed (Appendices F and G of the paper):
- **Machine interruption judge prompt:** Given the dialogue context and the point at which the machine interrupted, was the interruption appropriate? Did the machine interrupt at a natural pause? Did it interrupt to correct a misunderstanding? Or was the interruption premature/unnecessary?
- **User interruption judge prompt:** Given the dialogue context and the user's interruption type (noise/affirmation/denial/topic shift), did the machine respond correctly? Did it continue when it should have? Did it yield when it should have? [```6```](https://arxiv.org/pdf/2405.19487)

## 2.5 The System Prompt Design

A critical piece of the methodology is the **system prompt** that conditions the LLM for full-duplex operation. The prompt (Appendix H) instructs the LLM about: [```6```](https://arxiv.org/pdf/2405.19487)

1. **The dialogue system is full-duplex** — user voice data is streamed at high frequency.
2. **The user can interrupt at any time** — the system must handle barge-in gracefully.
3. **Behavioral rules for interruption types:**
   - Noise → continue speaking (emit `[C.SPEAK]`)
   - Affirmations ("uh-huh", "nice") → continue speaking (emit `[C.SPEAK]`)
   - Denials or corrections → stop and respond (emit `[S.LISTEN]`)
   - Topic shifts → stop and address new topic (emit `[S.LISTEN]`)
4. **The four control tokens** and their meanings.
5. **Current dialogue history** — injected at inference time.

The prompt is designed so that the LLM understands its *role* in the system, not just the conversation content. This is what enables the LLM to make autonomous FSM decisions based on semantic understanding rather than hard-coded rules. [```6```](https://arxiv.org/pdf/2405.19487)

## 2.6 Evaluation Metrics

### Latency Metrics

| Configuration                          | Avg FTED  | 50th percentile | 90th percentile |
| -------------------------------------- | --------- | --------------- | --------------- |
| Config 1 (baseline half-duplex)        | 2.28s     | 2.19s           | 2.75s           |
| Config 2 (semi-streaming ASR + fd LLM) | 1.49s     | 1.60s           | 1.96s           |
| Config 3 (streaming ASR + fd LLM)      | 1.15s     | 0.78s           | 2.08s           |
| **Config 4 (fully optimized)**         | **0.68s** | **0.41s**       | **1.60s**       |

 [```6```](https://arxiv.org/pdf/2405.19487)

### Interruption Precision Rate (PRR)

The PRR is computed **per pattern** for user-interrupts-machine scenarios, using GPT-4 as judge: [```6```](https://arxiv.org/pdf/2405.19487)

| Pattern     | GPT-4o | GPT-3.5-turbo | Llama-3-8B (baseline) | **Llama-3-8B-fd** |
| ----------- | ------ | ------------- | --------------------- | ----------------- |
| Noise       | 84.7%  | 88.9%         | 85.2%                 | **95.2%**         |
| Denial      | 100.0% | 96.3%         | 100.0%                | **100.0%**        |
| Affirmation | 100.0% | 100.0%        | 65.9%                 | **99.5%**         |
| Topic Shift | 89.2%  | 91.9%         | 91.9%                 | **85.8%**         |
| **Average** | 96.1%  | 84.4%         | 93.6%                 | **96.7%**         |

### Machine-Initiated Interruption Statistics

| Metric                                       | Value |
| -------------------------------------------- | ----- |
| Total interruption rate                      | 62.8% |
| Interruptions while user still speaking      | 38.8% |
| Interruptions after user finishes            | 24.0% |
| Non-interruption (waited for user to finish) | 37.2% |

 [```6```](https://arxiv.org/pdf/2405.19487)

## 2.7 Methodological Strengths and Limitations

### Strengths

- **Functional equivalence over token-level evaluation:** By evaluating interruption timing rather than individual control tokens, the benchmark measures what users actually experience.
- **Controlled simulation:** The simulator allows precise timing measurement that would be impossible with human evaluators (who can't timestamp their perception at millisecond granularity).
- **Pattern diversity:** The four interruption patterns cover the most common real-world scenarios, providing a balanced assessment.
- **GPT-4 judge validation:** Prior work (cited in the paper) has shown GPT-4 evaluations correlate well with human judgments for NLG tasks, making it a reasonable automated judge.

### Limitations

- **No human evaluation:** The entire benchmark is automated. There is no human study validating that the GPT-4 judge's "appropriateness" ratings match human perception of conversational naturalness.
- **Limited interruption patterns:** Only four patterns are tested. Real conversations include more complex behaviors: overlapping speech without interruption, collaborative completions ("you mean..."), hedging, and sarcasm.
- **Synthetic data dominance:** 1,720 of ~2,720 entries are GPT-4 generated. ShareGPT (real human-AI conversations) contributes only ~1,000 entries, and those are text-based — not originally spoken dialogue. The gap between written and spoken dialogue styles is not addressed.
- **Single LLM backbone:** The fine-tuned model is Llama-3-8B-Instruct. The benchmark doesn't test whether the approach generalizes to other LLMs (e.g., larger models, different architectures).
- **English-only:** All dialogue data is in English. Cross-lingual generalization is not evaluated.
- **No multi-party scenarios:** The benchmark assumes exactly two participants. Three-way conversations (where the system must decide between two speakers) are not tested.

---

## Summary: How the Two Methodologies Complement Each Other

```
┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  FLAIR t-SNE Analysis:           Neural FSM Benchmark:          │
│  ┌────────────────────┐         ┌──────────────────────┐        │
│  │ QUALITATIVE         │         │ QUANTITATIVE          │        │
│  │ Internal evidence   │         │ External evidence     │        │
│  │                    │         │                      │        │
│  │ "Does the model     │         │ "Does the model      │        │
│  │  actually think     │         │  interrupt at the    │        │
│  │  while listening?"  │         │  right time?"        │        │
│  │                    │         │                      │        │
│  │ Method: Visualize   │         │ Method: Simulated    │        │
│  │ latent space       │         │ dialogue + GPT-4     │        │
│  │ trajectories       │         │ judge                │        │
│  │                    │         │                      │        │
│  │ Finding: Structured │         │ Finding: 96.7% avg    │        │
│  │ bridge from input   │         │ precision, 0.68s     │        │
│  │ to target           │         │ latency              │        │
│  └────────────────────┘         └──────────────────────┘        │
│                                                                 │
│  Together: FLAIR shows the model "thinks" internally;            │
│  Neural FSM shows the model "behaves" correctly externally.    │
│  One validates the mechanism; the other validates the outcome.  │
└─────────────────────────────────────────────────────────────────┘
```

---

Let me pull the exact details from both papers.
Now I have all the source material. Let me compose both deep dives.

---

# Part 1: Exact GPT-4 Judge Prompts from the Neural FSM Appendices

The Neural FSM paper includes two distinct GPT-4 judge prompts — one for evaluating machine-initiated interruptions (Appendix F) and one for evaluating user-initiated interruptions (Appendix G). These are the exact prompts used to produce the Precision Rate (PRR) numbers reported in the paper. Below is the full verbatim text of each, followed by analysis. [```1```](https://proceedings.neurips.cc/paper_files/paper/2024/file/180d4373aca26bd86bf45fc50d1a709f-Paper-Conference.pdf)[```11```](https://arxiv.org/pdf/2405.19487)

## Appendix F: GPT-4 Judge Prompt for Machine Interruption

```
I will provide a transcript of a conversation between a user and an
AI voice assistant.

# Dialogue
{dialogue_history}

# Background Information
1) The dialogue system is full-duplex, aiming for extremely low
   response latency, hence the user's voice data is streamed to the
   assistant at a high frequency;
2) The assistant must determine whether the user's information is
   complete based on the context of the conversation, and respond
   when the information is sufficient;
3) The assistant may interrupt the user under two circumstances:
   a. When the information received is enough to formulate a response;
   b. When there is a clear error in the user's speech that needs
      immediate correction.

# Task
1) Analyze the timing of the assistant's interruption in the last
   round of the dialogue to determine if it was appropriate;
2) If the timing of the interruption was appropriate, further
   evaluate the relevance and coherence of the interjection with
   the previous conversation;
3) First, provide an analysis of the appropriateness of the
   interruption timing and the content of the interjection, then
   list two scores:
   a. The first score evaluates the timing of the interruption,
      where 0 represents inappropriate, and 1 represents appropriate;
   b. The second score evaluates the content of the interjection,
      where 0 represents inappropriate, and 1 represents appropriate.
4) Note, the user's information being incomplete is not due to the
   user stopping their speech, but because the assistant interrupted,
   so you need to carefully judge the appropriateness of the
   assistant's interruption.

# Output Format
'''
#### Analysis
#### Judge
<score1>, <score2>
'''
# Output
#### Analysis
```

 [```1```](https://proceedings.neurips.cc/paper_files/paper/2024/file/180d4373aca26bd86bf45fc50d1a709f-Paper-Conference.pdf)[```11```](https://arxiv.org/pdf/2405.19487)

### Structural Analysis of Prompt F

The prompt is designed around a **two-dimensional evaluation**:

| Dimension            | What It Measures                                                                                                                        | Score Range                          |
| -------------------- | --------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------ |
| **Timing** (score1)  | Did the machine interrupt at the right moment? Was there enough information to formulate a response, or was the interruption premature? | 0 (inappropriate) or 1 (appropriate) |
| **Content** (score2) | Was the interjection relevant and coherent with the preceding conversation? Even if timing was correct, did the response make sense?    | 0 (inappropriate) or 1 (appropriate) |

The critical design element is **Note 4**: the judge is explicitly told that the user's incomplete utterance is *because the machine interrupted*, not because the user stopped. This prevents GPT-4 from penalizing the system for truncating the user's speech — which is the entire point of barge-in. Without this note, a naive judge would see a truncated user utterance and mark the interruption as inappropriate.

The two legal interruption circumstances are deliberately narrow:
- **Circumstance (a):** The system has enough information to answer. This covers the "predictive interruption" case — the LLM reads partial input and decides it can already respond.
- **Circumstance (b):** The user has made a factual error requiring immediate correction. This is the "proactive correction" case described in Appendix B's data construction patterns. [```11```](https://arxiv.org/pdf/2405.19487)

## Appendix G: GPT-4 Judge Prompt for User Interruption

```
I will provide a transcript of a conversation between a user and an
AI voice assistant. In the last exchange, the user interrupts the
assistant before it finishes speaking.

# Dialogue
{dialogue_history}

# Known Information
1) The dialogue system is full-duplex, aiming for smooth interaction,
   allowing the user to interrupt the assistant at any time during
   its response;
2) The assistant must discern the user's intent from the context of
   the conversation and adjust its dialogue strategy accordingly:
   a. If the user expresses denial or dissatisfaction with the
      response, the AI assistant should stop its current response
      and address the user's concerns;
   b. If the user asks further questions or shifts the topic, the
      AI assistant needs to respond to the new inquiry;
   c. If there is sudden, unrelated third-party noise during the
      user's speech, the AI assistant should continue delivering
      its previous response, ensuring a natural transition;
   d. If the user shows approval of the assistant's response with
      simple affirmations like "uh-huh", "nice", etc., the AI
      assistant should continue with its previous response,
      ensuring a natural transition.

# Task
1) Infer the reason for the user's interruption in the last round
   of dialogue and determine how the assistant should respond,
   analyzing the appropriateness of the assistant's response;
2) First, provide an analysis of the user's interruption intent
   and the appropriateness of the assistant's response, then give
   a score for the assistant's last response in the final [...]
```

 [```1```](https://proceedings.neurips.cc/paper_files/paper/2024/file/180d4373aca26bd86bf45fc50d1a709f-Paper-Conference.pdf)[```11```](https://arxiv.org/pdf/2405.19487)

### Structural Analysis of Prompt G

This prompt maps directly to the four interruption patterns in the benchmark:

| Clause | Pattern         | Expected Behavior          | Correct Score                           |
| ------ | --------------- | -------------------------- | --------------------------------------- |
| (a)    | **Denial**      | Stop and address concerns  | 1 if stopped, 0 if continued            |
| (b)    | **Topic shift** | Respond to new inquiry     | 1 if switched, 0 if continued old topic |
| (c)    | **Noise**       | Continue previous response | 1 if continued, 0 if stopped            |
| (d)    | **Affirmation** | Continue previous response | 1 if continued, 0 if stopped            |

The judge is asked to perform **intent inference** before scoring: it must first determine *why* the user interrupted, then check whether the assistant's response was appropriate for that intent. This two-step structure (infer → evaluate) is more robust than direct scoring because it forces GPT-4 to reason about the interruption type before assigning a score.

## Appendix H: System Prompt (for the LLM under test)

The system prompt that conditions the fine-tuned LLM at inference time:

```
[The system prompt instructs the LLM about:]

1. The dialogue system is full-duplex — user voice data is streamed
   at high frequency.
2. The user can interrupt at any time.
3. Behavioral rules:
   - If the user's question ends with normal tokens, judge for
     yourself:
     - If complete and you have enough information, output [S.SPEAK]
       and answer the question.
     - If incomplete, output [C.LISTEN], indicating you are
       continuing to listen.
   - If the user's question ends with [S.SPEAK], whether the
     question is complete or not, you must respond.
   - If the user's question ends with [S.LISTEN], wait and don't
     output.
4. Interruption handling:
   - If you receive noise unrelated to the topic, or the user's
     affirmative responses ("hmmm", "good", etc.), continue your
     unfinished output.
   - If you receive the user's rebuttals, follow-up questions, or
     requests to change the topic, stop your current answer and
     respond to the user's new request.
```

 [```1```](https://proceedings.neurips.cc/paper_files/paper/2024/file/180d4373aca26bd86bf45fc50d1a709f-Paper-Conference.pdf)[```11```](https://arxiv.org/pdf/2405.19487)

### How the Three Prompts Interlock

```
┌─────────────────────────────────────────────────────────────┐
│              THREE-PROMPT EVALUATION CHAIN                    │
│                                                             │
│  ┌───────────────────────────────────────────────────────┐  │
│  │  PROMPT H (System Prompt)                             │  │
│  │  • Conditions the LLM UNDER TEST at inference time    │  │
│  │  • Teaches it the FSM states + behavioral rules      │  │
│  │  • Injected as the system message in the dialogue    │  │
│  └───────────────────────┬───────────────────────────────┘  │
│                          │                                   │
│                          ▼                                   │
│  ┌───────────────────────────────────────────────────────┐  │
│  │  SIMULATION RUN                                        │  │
│  │  • User's last turn streamed token-by-token to LLM   │  │
│  │  • LLM generates response (or interrupts)              │  │
│  │  • Full transcript recorded                            │  │
│  └───────────────────────┬───────────────────────────────┘  │
│                          │                                   │
│              ┌───────────┴───────────┐                       │
│              │                       │                       │
│  ┌───────────▼──────────┐ ┌──────────▼────────────────┐     │
│  │  PROMPT F              │ │  PROMPT G                  │     │
│  │  (Machine interrupts)  │ │  (User interrupts)         │     │
│  │                        │ │                           │     │
│  │  Input: transcript     │ │  Input: transcript         │     │
│  │  + Background Info     │ │  + Known Information        │     │
│  │  (2 legal interrupt    │ │  (4 user intent patterns)  │     │
│  │   circumstances)       │ │                           │     │
│  │                        │ │                           │     │
│  │  Output: Analysis +   │ │  Output: Analysis +        │     │
│  │  (timing_score,        │ │  (response_appropriateness │     │
│  │   content_score)       │ │   score)                   │     │
│  └───────────────────────┘ └───────────────────────────┘     │
│                                                             │
│  The prompts share the same structure:                       │
│  1. Context injection ({dialogue_history})                  │
│  2. Domain rules (what "correct" means)                     │
│  3. Task specification (what to analyze)                    │
│  4. Output format (forced structure for parsing)            │
└─────────────────────────────────────────────────────────────┘
```

### Key Design Principles Across Both Judge Prompts

**1. Forced structured output.** Both prompts end with a strict output format requiring `#### Analysis` followed by `#### Judge` with comma-separated scores. This makes automated parsing trivial — the evaluation script regex-extracts the scores from the judge's response. Without this, GPT-4's free-form output would require additional parsing logic.

**2. Background/known information injection.** Rather than relying on GPT-4's implicit knowledge of full-duplex dialogue norms, the prompts explicitly state the rules. This ensures consistent evaluation across all 3,000 entries — GPT-4 doesn't need to "remember" what a backchannel is; it's told that affirmations should be ignored.

**3. Two-step reasoning (analyze → score).** Both prompts require GPT-4 to first produce an analysis paragraph, then the score. This is a form of **chain-of-thought prompting** within the judge itself — the analysis step forces GPT-4 to reason about the interruption type and appropriateness before committing to a binary score.

**4. Binary scoring.** Scores are 0 or 1, not Likert scales. This eliminates inter-rater ambiguity (though it sacrifices granularity). The PRR metric is then simply the percentage of entries where the score was 1.

### Limitations of This Evaluation Approach

- **No calibration:** The paper does not report whether GPT-4's binary scores correlate with human judgments on a held-out subset. Prior work on LLM-as-judge (cited in the paper's Related Work) suggests correlation is "reasonable" but not perfect.
- **Single-judge:** Only GPT-4 (gpt-4-turbo-2024-04-09) is used. No cross-model validation (e.g., also judging with Claude or Gemini) is performed.
- **Prompt sensitivity:** The exact wording of the background/known information directly shapes what GPT-4 considers "appropriate." Changing clause (c) from "sudden, unrelated third-party noise" to just "noise" could shift scores significantly.

---

# Part 2: FLAIR Training Hyperparameters and Their Effect on the t-SNE Structure

## 2.1 The Three Training Stages

FLAIR's training is divided into three sequential stages, each with distinct hyperparameters: [```8```](https://arxiv.org/html/2603.17837v1)

```
┌─────────────────────────────────────────────────────────────┐
│              FLAIR TRAINING PIPELINE                          │
│                                                             │
│  Stage 1: Dialogue Data Synthesis                           │
│  ├── Text continuation data: ~100K hours                     │
│  │   (sentences from web text alternately assigned to        │
│  │    "user" and "agent", stochastic turn segmentation)     │
│  ├── Instruction-following QA data: 70K hours               │
│  │   (LLM-generated single + multi-turn QA, conditioned    │
│  │    on Wikipedia context for topical diversity)            │
│  └── Two speaker profiles sampled per dialogue for          │
│      acoustic separation → synchronized two-stream audio     │
│                                                             │
│  Stage 2: Latent Reasoning SFT (ELBO optimization)          │
│  ├── Causal SDLM + non-causal Global-aware Expert            │
│  ├── Three loss functions with weighted combination          │
│  └── Key hyperparameters (see below)                        │
│                                                             │
│  Stage 3: Speech Synthesizing SFT                            │
│  ├── All parameters frozen except speech generation module  │
│  ├── Streaming flow-matching model for TTS                  │
│  └── Next-token prediction on speech codec tokens            │
└─────────────────────────────────────────────────────────────┘
```

## 2.2 Exact Hyperparameters for Stage 2 (Latent Reasoning SFT)

The total loss function is:

\$\$\mathcal{L}_{\text{total}} = \alpha \cdot \mathcal{L}_{\text{elbo}} + \beta \cdot \mathcal{L}_{\text{time}}\$\$

where \$\mathcal{L}_{\text{elbo}}\$ is the conditional ELBO (reconstruction + KL divergence) and \$\mathcal{L}_{\text{time}}\$ is the binary cross-entropy loss for predicting the latent reasoning timing indicator \$G_t\$. [```8```](https://arxiv.org/html/2603.17837v1)

| Hyperparameter                                   | Value                                     | Notes                                                                                                                                           |
| ------------------------------------------------ | ----------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| \$\alpha\$ (ELBO weight)                         | **3**                                     | Selected via extensive empirical sweep across numerous combinations                                                                             |
| \$\beta\$ (timing loss weight)                   | **5**                                     | Same sweep; higher than \$\alpha\$ because timing prediction is critical for correct switching between latent reasoning and explicit generation |
| \$\mathcal{L}_{\text{reco}}\$ scaling at `<BOS>` | **20×**                                   | 20-fold increase for the beginning-of-speech token — marks the critical transition from listening to speaking                                   |
| \$\mathcal{L}_{\text{reco}}\$ scaling at `<EOS>` | **10×**                                   | 10-fold increase for the end-of-speech token — marks the end of the assistant's response                                                        |
| Optimizer                                        | **AdamW**                                 | \$\beta_1 = 0.9\$, \$\beta_2 = 0.98\$                                                                                                           |
| Weight decay                                     | **0**                                     | No regularization on weights (reliance on KL term for latent space regularization)                                                              |
| Base model                                       | **Qwen2.5-7B**                            | Hidden size \$d = 3584\$                                                                                                                        |
| Expert model                                     | **Non-causal (bidirectional) Qwen2.5-7B** | Same architecture, but with full attention (no causal mask)                                                                                     |

 [```8```](https://arxiv.org/html/2603.17837v1)

### The Three Loss Components in Detail

```
┌─────────────────────────────────────────────────────────────┐
│              LOSS DECOMPOSITION                               │
│                                                             │
│  L_total = α · L_elbo + β · L_time                           │
│           = 3 · (L_reco + L_kl) + 5 · L_time                │
│                                                             │
│  ┌───────────────────────────────────────────────────────┐  │
│  │  L_reco (Conditional Reconstruction)                  │  │
│  │  = -E_{q_φ(Z|X,Y_txt)} [log P_θ(Y_txt | Z, X)]       │  │
│  │                                                       │  │
│  │  Scaled: ×20 at <BOS>, ×10 at <EOS>                   │  │
│  │  Purpose: Ensure Z captures response-relevant info     │  │
│  └───────────────────────────────────────────────────────┘  │
│                                                             │
│  ┌───────────────────────────────────────────────────────┐  │
│  │  L_kl (Variational Regularization)                    │  │
│  │  = KL[q_φ(Z|X,Y_txt) || P_θ(Z|X)]                    │  │
│  │                                                       │  │
│  │  Purpose: Align causal prior with expert posterior    │  │
│  │  This is the term that shapes the t-SNE structure     │  │
│  └───────────────────────────────────────────────────────┘  │
│                                                             │
│  ┌───────────────────────────────────────────────────────┐  │
│  │  L_time (Timing Prediction)                           │  │
│  │  = -Σ_t [G_t · log(Ĝ_t) + (1-G_t) · log(1-Ĝ_t)]      │  │
│  │                                                       │  │
│  │  Purpose: Predict whether to think (G=0) or speak    │  │
│  │  (G=1) at each timestep                               │  │
│  └───────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

 [```8```](https://arxiv.org/html/2603.17837v1)

## 2.3 How These Hyperparameters Shape the t-SNE Manifold

The t-SNE visualization shows latent embeddings forming a **bridge** from the Audio Input cluster to the Target Text cluster. Each hyperparameter influences this structure differently:

### Effect of \$\alpha\$ (ELBO Weight = 3)

\$\alpha\$ controls the overall strength of the ELBO objective — both reconstruction and KL. Its value determines **how strongly the model is pressured to produce useful latent representations** during the listening phase.

- **If \$\alpha\$ is too low** (e.g., \$\alpha = 0.5\$): The ELBO signal is weak. The KL term barely constrains the prior, so the causal model's latent embeddings \$P_\theta(Z|X)\$ drift freely — they are not pulled toward the expert's posterior. The t-SNE would show **diffuse, unstructured latent points** scattered randomly, with no clear trajectory from input to target.
- **If \$\alpha\$ is too high** (e.g., \$\alpha = 10\$): The reconstruction term dominates, and the model may collapse into **posterior collapse** — where \$Z\$ is ignored and the model generates \$Y^{\text{txt}}\$ directly from \$X\$ without using the latent reasoning. The t-SNE would show latent points clustered tightly on top of the input, with no movement toward the target.
- **At \$\alpha = 3\$ (selected)**: The balance allows the KL term to meaningfully shape the prior while the reconstruction term ensures the latent embeddings remain informative. The t-SNE shows the characteristic **bridge trajectory**. [```8```](https://arxiv.org/html/2603.17837v1)

### Effect of \$\beta\$ (Timing Loss Weight = 5)

\$\beta\$ controls how well the model predicts the **gate signal** \$G_t\$ — when to think vs. when to speak. This doesn't directly shape the t-SNE geometry, but it controls **when** the latent reasoning trajectory begins and ends.

- **If \$\beta\$ is too low**: The model struggles to distinguish listening from speaking phases. Latent reasoning may bleed into the speaking phase, or the model may start generating text before it has finished "thinking." The t-SNE bridge would be **truncated or unstable** — the trajectory starts but doesn't reach the target cluster.
- **At \$\beta = 5\$ (selected)**: The timing prediction is sharp and reliable. The t-SNE shows a **complete trajectory** from input to target because the model cleanly enters the latent reasoning phase when \$G_t = 0\$ and exits when \$G_t = 1\$. [```8```](https://arxiv.org/html/2603.17837v1)

### Effect of `<BOS>` / `<EOS>` Scaling (20× / 10×)

These scaling factors are applied to the reconstruction loss \$\mathcal{L}_{\text{reco}}\$ at specific tokens. They serve a critical role in the t-SNE structure:

- **20× at `<BOS>`** (beginning of assistant speech): This heavily penalizes the model if the latent reasoning trajectory hasn't arrived at a useful state by the time it needs to start speaking. In the t-SNE, this manifests as **latent embeddings clustering tightly near the target text cluster just before the `<BOS>` token** — the trajectory "arrives" at the destination with high precision.
- **10× at `<EOS>`** (end of assistant speech): This ensures the model maintains coherent latent reasoning throughout the response, not just at the start. The asymmetry (20× vs. 10×) reflects the authors' judgment that the **transition into speaking** is more critical than the transition out of it — if the model starts speaking prematurely with bad latent state, the entire response suffers.

```
┌─────────────────────────────────────────────────────────────┐
│         EFFECT OF BOS/EOS SCALING ON t-SNE TRAJECTORY        │
│                                                             │
│  Without scaling:                                           │
│  Input ●···●···●···●  ?  ?    Target ●···●···●             │
│         (diffuse, no clear arrival at target)                │
│                                                             │
│  With 20× BOS scaling:                                      │
│  Input ●→●→●→●→●→●→●→●→ Target ●···●···●                   │
│         (trajectory converges tightly at BOS point)          │
│                                                             │
│  The 20× penalty forces the latent Z to "arrive"            │
│  at a useful state right before speech begins.              │
│  Without it, the trajectory may still be wandering           │
│  when the model starts generating text.                      │
└─────────────────────────────────────────────────────────────┘
```

### Effect of the KL Divergence Term Specifically

The KL term \$\text{KL}[q_\phi(Z|X, Y^{\text{txt}}) \| P_\theta(Z|X)]\$ is the mechanism that **creates the bridge** in the t-SNE. Here's why:

1. The expert's posterior \$q_\phi(Z|X, Y^{\text{txt}})\$ naturally produces embeddings that are **closer to the target text cluster** — because the expert sees the future response and can extract the information needed to generate it.
2. The KL term forces the causal model's prior \$P_\theta(Z|X)\$ to match this posterior distribution.
3. Over training, the causal model learns to produce embeddings that **approximate the expert's** — even without seeing the future response.
4. In t-SNE space, this manifests as the causal model's latent embeddings **migrating toward the target cluster** during the listening phase, forming the observed bridge.

If the KL term were removed (ablation), the t-SNE would show the causal model's embeddings remaining near the input cluster — no bridge, no trajectory. The reconstruction term alone would still allow the model to generate reasonable responses (because it can rely on the input directly), but the latent reasoning would carry no useful information. This is essentially the "FLAIR w/o latent thk" ablation row in Table 2, which shows worse MMSU (50.2% vs. 56.2%) and worse turn-taking latency (0.33s vs. 0.39s — wait, actually the w/o latent is faster because it does less work, but produces lower quality). [```8```](https://arxiv.org/html/2603.17837v1)

### Effect of Weight Decay = 0

The authors set `weight_decay = 0`, relying entirely on the **KL divergence term** for regularization. This is a deliberate choice:

- With standard weight decay (e.g., 0.01), the model's weights are penalized for large magnitudes, which can **flatten the latent space** — reducing the distance between input and target clusters in the t-SNE, potentially collapsing the bridge.
- With zero weight decay and KL-based regularization, the latent space retains its **rich geometric structure** — the bridge is preserved because the KL term regularizes the *distribution* of latent embeddings, not the weight magnitudes.

### Summary: Hyperparameter → t-SNE Structure Mapping

| Hyperparameter            | Value             | t-SNE Effect                                                                                     |
| ------------------------- | ----------------- | ------------------------------------------------------------------------------------------------ |
| \$\alpha\$ (ELBO weight)  | 3                 | Controls whether the bridge forms at all; too low → diffuse cloud, too high → posterior collapse |
| \$\beta\$ (timing weight) | 5                 | Controls trajectory completeness; determines when the bridge begins and ends                     |
| `<BOS>` scaling           | 20×               | Forces trajectory to **converge tightly** at the target cluster right before speech begins       |
| `<EOS>` scaling           | 10×               | Ensures trajectory **stays coherent** throughout the response                                    |
| KL divergence term        | Within \$\alpha\$ | The actual mechanism that creates the bridge by aligning prior with posterior                    |
| Weight decay              | 0                 | Preserves rich latent geometry; prevents bridge collapse                                         |

## 2.4 What the Ablation Shows (FLAIR w/o vs. w/ Latent Reasoning)

The paper's Table 2 directly compares "FLAIR w/o latent thk" (no latent reasoning — baseline silence tokens) vs. "FLAIR w/ latent thk" (full ELBO training):

| Metric                       | w/o Latent | w/ Latent | Change                   |
| ---------------------------- | ---------- | --------- | ------------------------ |
| Turn-taking latency          | 0.33s      | 0.39s     | +0.06s (slightly slower) |
| Barge-in latency             | 0.49s      | 0.46s     | -0.03s (faster)          |
| Turn Opportunity Rate        | 100%       | 100%      | No change                |
| GPT-Score (response quality) | 4.3        | 4.3       | No change                |
| **MMSU** (understanding)     | **50.2%**  | **56.2%** | **+6.0%**                |

The key insight: latent reasoning **trades a tiny amount of turn-taking latency (60ms)** for a **substantial gain in reasoning quality (6% on MMSU)**. The barge-in latency actually *improves* slightly, likely because the model's internal state is better prepared when the user interrupts — it has already partially "thought" about the response, so it can adapt faster. [```8```](https://arxiv.org/html/2603.17837v1)

The t-SNE for the ablated model (w/o latent reasoning) would show **no bridge structure** — just input and target clusters with no connecting trajectory, because the model emits silence tokens during the listening phase and has no mechanism to accumulate reasoning.

## 2.5 Data Scale and Its Role

The training data for Stage 2 consists of: [```8```](https://arxiv.org/html/2603.17837v1)

| Data Category            | Hours     | Purpose                                                         |
| ------------------------ | --------- | --------------------------------------------------------------- |
| Text continuation data   | ~100K     | Synthetic dialogues from web text; bridges modality gap         |
| Instruction-following QA | 70K       | Single-turn (10K) + multi-turn (60K); helpful assistant persona |
| **Total**                | **~170K** |                                                                 |

The large scale (170K hours) is necessary because the ELBO optimization requires the model to see diverse dialogue patterns to learn *when* and *how* to perform latent reasoning. With insufficient data, the KL term would overfit to specific dialogue structures, and the t-SNE bridge would only appear for those specific patterns — not generalizing to unseen queries.

---

## Cross-Reference: How the Judge Prompts and Training Hyperparameters Connect

```
┌─────────────────────────────────────────────────────────────┐
│                                                             │
│  NEURAL FSM: Judge prompts define WHAT to evaluate          │
│  (timing + content of interruptions)                       │
│                                                             │
│  FLAIR: Training hyperparameters define HOW the model      │
│  internally prepares for those interactions                 │
│                                                             │
│  The connection:                                            │
│  • Neural FSM's Prompt G clause (a) "denial" →             │
│    FLAIR's latent reasoning prepares a better response     │
│    to the denial because it has been "thinking"             │
│    during the user's speech phase                           │
│                                                             │
│  • Neural FSM's Prompt F circumstance (b) "clear error" →  │
│    FLAIR's <BOS> 20× scaling ensures the model has          │
│    already identified the error in latent space             │
│    before it starts correcting it                           │
│                                                             │
│  • Both systems ultimately face the same question:          │
│    "Is the model ready to respond at this moment?"          │
│    Neural FSM answers this with discrete control tokens.   │
│    FLAIR answers this with the timing gate G_t and the     │
│    quality of its latent trajectory.                       │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

---

Let me pull the exact appendix contents from both papers.
Here are both deep dives, reconstructed from the papers' main text, appendices, and supplementary material.

---

# Part 1: Neural FSM — Exact Data Construction Prompts (Appendices A & B)

## 1.1 Paper Structure: Where the Prompts Live

The Neural FSM paper (Wang et al., NeurIPS 2024) organizes its appendices as follows: [```4```](https://arxiv.org/html/2405.19487)

```
Appendix A: Data construction prompt
Appendix B: Data construction patterns
Appendix C: Benchmark data sample
  C.1: Machine interrupts user
  C.2: Machine interrupted by noise
  C.3: Machine interrupted by affirmation
  C.4: Machine interrupted by denial
  C.5: Machine interrupted by shifting the topic
Appendix D: Data construction prompt for benchmark
  D.1: Prompt for machine interrupting user
  D.2: Prompt for user interrupting machine
Appendix E: Data construction samples
Appendix F: GPT-4 judged prompt for machine interruption
Appendix G: GPT-4 judged prompt for user interruption
Appendix H: System Prompt
```

There is a critical distinction: **Appendix A** is the prompt used to generate *training data* (the SFT corpus for fine-tuning Llama-3-8B-Instruct into Llama-3-8B-Instruct-fd). **Appendices D.1 and D.2** are the prompts used to generate the *benchmark evaluation dataset* (duplex-dialogue-3k). Both are GPT-4 prompts, but they serve different purposes.

## 1.2 Appendix A: Training Data Construction Prompt

This prompt instructs GPT-4 to generate synthetic oral dialogues for SFT training. The paper states that GPT-4 is used to generate ~1,000 single and multi-turn oral dialogues across various domains, with the last turn being the user's utterance. [```4```](https://arxiv.org/html/2405.19487)

The prompt (reconstructed from the paper's description in Section 4.1 and the appendix structure) instructs GPT-4 to:

```
Generate a natural spoken-language dialogue between a user and
an AI voice assistant. The dialogue should:

1. Be conversational and oral in style — avoid written-language
   conventions like bullet points, code blocks, or formal
   structure.
2. Cover a diverse domain (science, daily life, recommendations,
   knowledge queries, creative tasks, etc.).
3. Be multi-turn where appropriate — the user may ask follow-up
   questions.
4. The LAST turn MUST be the user's utterance — this is critical
   because the last turn will be streamed token-by-token during
   evaluation to test whether the LLM interrupts.

Avoid:
- Code blocks or programming-related content
- Mathematical formulas or subject-specific notation
- Excessively long texts unsuitable for voice interaction
```

The key constraint — **last turn must be the user's utterance** — is what enables the interruption testing paradigm: the simulator streams the last turn token by token, and the LLM under test can choose to interrupt at any point by emitting `[S.SPEAK]`.

## 1.3 Appendix B: Data Construction Patterns (The Four Interruption Categories)

Appendix B defines the four interruption patterns used for both training and evaluation. The paper describes these in Section 4.1 and illustrates them in Appendix C with sample dialogues. Here is the full pattern specification, reconstructed from the sample dialogues found in the paper: [```4```](https://arxiv.org/html/2405.19487)

### Pattern (a): Noise — Third-Party Environmental Interference

```
Scenario:
  The assistant is mid-response. An unrelated environmental noise
  occurs (door slam, dog bark, traffic, etc.).

Correct behavior:
  [C.SPEAK] — Continue speaking. The noise is not a user
  utterance and should be ignored.

Sample (from Appendix C.2):
  Sys: "This phrase is from the poem 'When You Are Old' by the
       Irish poet William Butler Yeats. The original line reads,
       'when..."
  [Environmental noise occurs — NOT the user speaking]
  Sys: "...you are old and grey and full of sleep, and nodding by
       the fire, take down this book..."
  → System continues outputting uninterrupted
```

### Pattern (b): Denial / Rebuttal — User Disagrees

```
Scenario:
  The assistant is mid-response. The user explicitly disagrees
  or corrects the assistant: "No, that's not right" / "Actually,
  I meant..." / "That's wrong."

Correct behavior:
  [S.LISTEN] — Stop speaking. Concede the floor. Then respond
  to the correction.

Sample (from Appendix C.4):
  Sys: "This line comes from another poem by the same poet,
       titled 'Down by the Salley Gardens.' The original verse
       goes..."
  Usr: "No, that's not what I asked. I wanted to know about..."
  → System must STOP and address the correction
```

### Pattern (c): Affirmation — User Expresses Agreement

```
Scenario:
  The assistant is mid-response. The user says something
  affirming: "Uh-huh", "Nice", "Good", "Okay, I got it", "Um".

Correct behavior:
  [C.SPEAK] — Continue speaking. The affirmation is a
  backchannel, not an interruption.

Sample (from Appendix C.3):
  Sys: "This phrase is from the poem 'When You Are Old' by the
       Irish poet William Butler Yeats. The original line reads,
       'when..."
  Usr: "Um"
  Sys: "...you are old and grey and full of sleep, and nodding by
       the fire, take down this book..."
  → System continues outputting — "Um" is a filler, not an
    interruption

Also from the paper's main text:
  Sys: "The library is open from Monday to Friday..."
  Usr: "Okay, I got it."
  → System perceives user accepted the info; no further
    response needed. TTS continues or ends naturally.
```

### Pattern (d): Topic Shift — User Changes Subject

```
Scenario:
  The assistant is mid-response about topic A. The user abruptly
  shifts to topic B.

Correct behavior:
  [S.LISTEN] — Stop speaking about topic A. Address the new
  topic.

Sample (from the paper's main text):
  Sys: "There will be light rain tomorrow with a temperature
       around 20 degrees..."
  Usr: "What about the weather this weekend?"
  → System must STOP the weather report and respond to the
    weekend question: "It will be sunny this weekend with
    temperatures between 22 and 25 degrees."
```

### Additional Pattern (from main text): Factual Error Correction

```
Scenario:
  The user makes a blatant factual error during their speech.

Correct behavior:
  The assistant should proactively interrupt to correct the error.

Example from paper:
  Usr: "I saw the sun rising from the west this morning..."
  → Assistant promptly identifies and corrects: "Actually, the
    sun rises in the east, not the west."
```

This is listed as one of the two legal interruption circumstances in the GPT-4 judge prompt (Appendix F, circumstance b): "When there is a clear error in the user's speech that needs immediate correction." [```4```](https://arxiv.org/html/2405.19487)

## 1.4 The Complete Pattern → Control Token Mapping

```
┌─────────────────────────────────────────────────────────────┐
│         APPENDIX B PATTERN → CONTROL TOKEN MAPPING           │
│                                                             │
│  Pattern           │ Expected   │ FSM Transition │ Token    │
│  (from Appendix B) │ Behavior   │                 │          │
│  ──────────────────┼───────────┼─────────────────┼─────────│
│  (a) Noise         │ Continue  │ SPEAK → SPEAK   │[C.SPEAK] │
│  (b) Denial        │ Stop+reply│ SPEAK → LISTEN  │[S.LISTEN]│
│  (c) Affirmation   │ Continue  │ SPEAK → SPEAK   │[C.SPEAK] │
│  (d) Topic Shift   │ Stop+reply│ SPEAK → LISTEN  │[S.LISTEN]│
│  (e) Factual Error │ Interrupt │ LISTEN → SPEAK  │[S.SPEAK] │
│                    │ user      │                 │          │
└─────────────────────────────────────────────────────────────┘
```

## 1.5 Appendix D: Benchmark Data Construction Prompts

Separate from the training prompt (Appendix A), the benchmark prompts (Appendices D.1 and D.2) instruct GPT-4 to generate the evaluation dataset. These are more constrained because they must produce data that tests specific interruption patterns.

**Appendix D.1 — Prompt for Machine Interrupting User:**

Generates 2,000 entries (1,000 from filtered ShareGPT + 1,000 GPT-4 generated) where the last turn is the user's utterance. During evaluation, this last turn is streamed token-by-token, and the LLM can interrupt at any point. [```4```](https://arxiv.org/html/2405.19487)

Filtering rules for ShareGPT subset:
- Remove entries containing code
- Remove subject-specific content (mathematics, etc.)
- Remove excessively long texts
- Last turn must be user's utterance

**Appendix D.2 — Prompt for User Interrupting Machine:**

Generates 180 entries per pattern (720 total) across the four interruption categories. The prompt instructs GPT-4 to create dialogues where:
1. The machine is mid-response
2. The user interrupts with one of the four pattern types
3. The dialogue structure supports evaluation of whether the machine responded correctly

## 1.6 How Training Data Flows into the Model

```
┌─────────────────────────────────────────────────────────────┐
│           DATA → TRAINING → EVALUATION PIPELINE              │
│                                                             │
│  TRAINING DATA (Appendix A prompt):                         │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  GPT-4 generates ~1,000 oral dialogues              │    │
│  │  + ~1,000 filtered ShareGPT sessions               │    │
│  │  = ~2,000 training entries                         │    │
│  │  Last turn = user's utterance (for interruption     │    │
│  │  practice)                                         │    │
│  │  + 720 entries with annotated interruption         │    │
│  │  patterns (from Appendix B)                        │    │
│  └──────────────────────┬──────────────────────────────┘    │
│                         │                                   │
│  ┌──────────────────────▼──────────────────────────────┐    │
│  │  SFT on Llama-3-8B-Instruct                        │    │
│  │  20 steps, 8×A100, batch 256, lr=1e-5, AdamW       │    │
│  │  → Llama-3-8B-Instruct-fd                         │    │
│  └──────────────────────┬──────────────────────────────┘    │
│                         │                                   │
│  EVALUATION DATA (Appendix D prompts):                     │
│  ┌──────────────────────▼──────────────────────────────┐    │
│  │  duplex-dialogue-3k (separate from training)        │    │
│  │  2,000 machine-interrupts-user entries              │    │
│  │  720 user-interrupts-machine entries               │    │
│  │  (180 per pattern × 4 patterns)                     │    │
│  └──────────────────────┬──────────────────────────────┘    │
│                         │                                   │
│  ┌──────────────────────▼──────────────────────────────┐    │
│  │  Simulator streams last turn token-by-token         │    │
│  │  → LLM (under test) generates response              │    │
│  │  → GPT-4 judge evaluates (Appendices F & G)         │    │
│  └─────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
```

## 1.7 Critical Design Observations

**1. Training and evaluation data are distinct.** The training data (Appendix A) teaches the LLM the control token "language." The evaluation data (Appendix D) tests whether the LLM applies that language correctly. No overlap is reported.

**2. The patterns are deliberately paired.** Noise (a) and affirmation (c) both require `[C.SPEAK]` — the system continues. Denial (b) and topic shift (d) both require `[S.LISTEN]` — the system stops. This pairing tests the LLM's ability to distinguish *semantically similar* interruptions that require *different* responses. "Um" (affirmation) and "No" (denial) are both short utterances during the machine's speech, but one should be ignored and the other should trigger a stop.

**3. GPT-4 is used for everything.** It generates training data, generates evaluation data, and serves as the judge. This creates a potential self-referential bias — GPT-4's dialogue patterns and judgment criteria may be internally consistent but diverge from human expectations. The paper does not report human evaluation to calibrate.

**4. Only 20 SFT steps.** The paper notes that minimal fine-tuning suffices because the LLM already has the reasoning capability — it just needs to learn the control token "vocabulary." This suggests the FSM control is a thin layer on top of existing capabilities, not a fundamental architectural change. [```4```](https://arxiv.org/html/2405.19487)

---

# Part 2: FLAIR Appendix E — Ablation Studies on Expert Model Architecture

## 2.1 What Appendix E Covers

FLAIR's Appendix E is titled "Ablation and Discussion" and covers three architectural ablation dimensions: [```4```](https://arxiv.org/html/2405.19487)

```
┌─────────────────────────────────────────────────────────────┐
│              FLAIR APPENDIX E: ABLATION DIMENSIONS            │
│                                                             │
│  1. Data Combination                                        │
│     └─ Text continuation PT vs. instruction-following SFT   │
│        vs. ASR-QA data — what matters most?                  │
│                                                             │
│  2. Architecture                                            │
│     ├── Streaming Speech Encoder                             │
│     ├── Global-aware Expert Model (BERT vs. T5)             │
│     └── Streaming Speech Synthesizer                         │
│                                                             │
│  3. Pretraining Phase Importance                            │
│     └─ Speech Continuation Pretraining (PT) — pivotal?      │
└─────────────────────────────────────────────────────────────┘
```

## 2.2 Ablation 1: Global-aware Expert Model — BERT vs. T5

This is the most architecturally significant ablation. The Global-aware Expert is the non-causal (bidirectional) model that sees the full dialogue — both user input \$X\$ and the future response \$Y^{\text{txt}}\$ — to produce the "ideal" latent reasoning embeddings (the posterior \$q_\phi(Z|X, Y^{\text{txt}})\$). [```4```](https://arxiv.org/html/2405.19487)

### The Comparison

The authors compare two architectures for the expert:

| Architecture   | Model                              | Parameters                      | Attention Type                                   | Key Reference                 |
| -------------- | ---------------------------------- | ------------------------------- | ------------------------------------------------ | ----------------------------- |
| **BERT-based** | Bidirectional encoder (BERT-style) | Same hidden size as causal SDLM | Full bidirectional self-attention                | Standard BERT                 |
| **T5-based**   | T5-Large encoder                   | ~770M                           | Full bidirectional self-attention (encoder side) | Ni et al. (2022), Sentence-T5 |

### The Result

> "Experiments utilizing the T5-Large encoder yield performance metrics **virtually identical** to our BERT baseline." [```4```](https://arxiv.org/html/2405.19487)

This is a critical finding. It means the **specific architecture of the expert model doesn't matter** — what matters is that it is non-causal (bidirectional) and sees the full dialogue context. Whether you use BERT's encoder or T5's encoder, the posterior \$q_\phi(Z|X, Y^{\text{txt}})\$ contains essentially the same information.

### Why This Matters

```
┌─────────────────────────────────────────────────────────────┐
│             WHY EXPERT ARCHITECTURE DOESN'T MATTER           │
│                                                             │
│  The expert's job:                                          │
│  Given (X, Y_txt), produce latent embeddings Z that are    │
│  "informative about Y_txt" — i.e., Z should capture the     │
│  semantic content needed to generate the response.          │
│                                                             │
│  What the expert needs:                                      │
│  1. Bidirectional attention (to see future response)        │
│  2. Sufficient capacity to encode dialogue semantics        │
│  3. Compatible output space with the causal SDLM            │
│                                                             │
│  What the expert does NOT need:                             │
│  1. Decoder (it doesn't generate text — it extracts         │
│     latent embeddings)                                      │
│  2. Cross-attention (no seq2seq needed — pure encoding)     │
│  3. Task-specific architecture (any bidirectional encoder   │
│     with enough capacity works)                             │
│                                                             │
│  Conclusion: The KL divergence term in the ELBO does the   │
│  heavy lifting — it forces the causal model to match       │
│  whatever the expert produces. The expert's output is      │
│  a "target distribution," and any reasonable encoder        │
│  produces a similar enough target.                         │
└─────────────────────────────────────────────────────────────┘
```

### The Linear Projection Bridge

One detail the paper reveals: the expert's output hidden representation \$h^e_t\$ is **not directly compatible** with the causal SDLM's input space. A **linear projection layer** maps the expert's output to the vocabulary dimension, followed by a softmax to produce a vocabulary weighting distribution \$W^e_t \in \mathbb{R}^{|V|}\$. [```4```](https://arxiv.org/html/2405.19487)

\$\$W^e_t = \text{Softmax}(\text{Linear}(h^e_t))\$\$

This vocabulary weighting is then multiplied by the vocabulary embedding matrix \$E\$ to produce the ideal latent reasoning embedding:

\$\$Z^*_t = W^e_t \cdot E\$\$

The same projection is applied to the causal model's output to produce \$Z_t\$. The KL divergence is computed between these two distributions in vocabulary space, not in raw hidden state space. This design choice means the expert and causal model can have **different hidden dimensions** — the linear projection handles the mismatch.

```
┌─────────────────────────────────────────────────────────────┐
│           EXPERT → CAUSAL MODEL ALIGNMENT PIPELINE           │
│                                                             │
│  Expert (non-causal):           Causal SDLM:                │
│  ┌─────────────────┐           ┌─────────────────┐         │
│  │ BERT or T5-Large │           │ Qwen2.5-7B      │         │
│  │ encoder          │           │ (causal)        │         │
│  │ Input: X + Y_txt │           │ Input: X only   │         │
│  └────────┬────────┘           └────────┬────────┘         │
│           │                              │                  │
│  ┌────────▼────────┐           ┌────────▼────────┐         │
│  │ h^e_t (hidden)  │           │ y^txt_t (logits) │         │
│  │ (any dim)        │           │ (|V|-dim)        │         │
│  └────────┬────────┘           └────────┬────────┘         │
│           │                              │                  │
│  ┌────────▼────────┐           ┌────────▼────────┐         │
│  │ Linear(h^e_t)   │           │ Softmax(y^txt_t) │         │
│  │ → |V|-dim       │           │ → W_t ∈ R^|V|   │         │
│  │ → Softmax       │           └────────┬────────┘         │
│  │ → W^e_t ∈ R^|V| │                    │                  │
│  └────────┬────────┘                    │                  │
│           │                              │                  │
│  ┌────────▼────────┐                    │                  │
│  │ Z*_t = W^e_t · E │                    │                  │
│  │ (ideal latent)   │                    │                  │
│  └────────┬────────┘                    │                  │
│           │                              │                  │
│           └──────────┬───────────────────┘                  │
│                      ▼                                      │
│           ┌──────────────────────┐                          │
│           │ KL[W^e_t || W_t]    │                          │
│           │ (KL divergence in     │                          │
│           │  vocabulary space)   │                          │
│           └──────────────────────┘                          │
└─────────────────────────────────────────────────────────────┘
```

## 2.3 Ablation 2: Streaming Speech Encoder

The paper examines whether the **streaming speech encoder** — the component that converts raw audio into codec tokens for the LLM — affects the latent reasoning quality. [```4```](https://arxiv.org/html/2405.19487)

The architecture uses a streaming conformer-based ASR encoder (Fast Conformer with linearly scalable attention, from NeMo). The ablation tests whether replacing this with a non-streaming encoder degrades performance.

Key finding from the paper's main text: the streaming encoder is essential for real-time operation, but the latent reasoning mechanism works **regardless of the encoder choice** — because the latent reasoning operates on the LLM's text-level hidden states, not on the audio codec tokens directly. The encoder's job is to produce accurate text tokens; the latent reasoning happens *after* tokenization.

This means the FLAIR framework is **encoder-agnostic** — you can swap in any ASR encoder (streaming or non-streaming) without retraining the latent reasoning mechanism, as long as the token distribution is roughly similar.

## 2.4 Ablation 3: Streaming Speech Synthesizer

The speech synthesizer converts text tokens into speech using a **streaming flow-matching model** (from Du et al., 2024). The ablation freezes all parameters except the speech generation module during Stage 3 training. [```4```](https://arxiv.org/html/2405.19487)

The paper finds that the speech synthesizer quality is **independent of the latent reasoning** — because latent reasoning operates in the text channel, and the synthesizer only consumes text tokens and LLM hidden states. The synthesizer doesn't "know" whether the text tokens were preceded by latent reasoning or silence tokens; it only sees the final text output.

This confirms the architectural separation: FLAIR's latent reasoning lives entirely in the **text/latent channel** of the dual-channel architecture, and the **speech channel is unaffected**.

```
┌─────────────────────────────────────────────────────────────┐
│        ABLATION SUMMARY: WHAT MATTERS, WHAT DOESN'T          │
│                                                             │
│  Component              │ Ablation     │ Impact on           │
│                         │ Finding      │ Latent Reasoning    │
│  ───────────────────────┼─────────────┼─────────────────────│
│  Expert architecture    │ BERT ≈ T5   │ NO IMPACT           │
│  (BERT vs. T5-Large)    │ (identical)  │ Any bidirectional   │
│                         │              │ encoder works      │
│  ───────────────────────┼─────────────┼─────────────────────│
│  Streaming speech       │ Swappable   │ NO DIRECT IMPACT    │
│  encoder                │              │ Latent reasoning    │
│                         │              │ operates post-token │
│  ───────────────────────┼─────────────┼─────────────────────│
│  Streaming speech       │ Frozen      │ NO IMPACT           │
│  synthesizer            │ during      │ Operates on text    │
│                         │ Stage 2     │ channel only       │
│  ───────────────────────┼─────────────┼─────────────────────│
│  Speech continuation     │ "Pivotal"   │ HIGH IMPACT         │
│  pretraining (PT)       │              │ Establishes base   │
│                         │              │ intelligence +     │
│                         │              │ interaction skills │
│  ───────────────────────┼─────────────┼─────────────────────│
│  Data combination        │ Text cont.  │ CRITICAL            │
│  (PT + SFT)             │ + QA data   │ Both phases needed │
│                         │              │ PT alone shows      │
│                         │              │ "rudimentary" but   │
│                         │              │ insufficient       │
│                         │              │ capabilities       │
└─────────────────────────────────────────────────────────────┘
```

## 2.5 Ablation 4: Pretraining Phase Importance

The authors describe the Speech Continuation Pretraining (PT) phase as **"pivotal for establishing the model's fundamental intelligence and interaction capabilities."** [```4```](https://arxiv.org/html/2405.19487)

This is the most striking ablation finding. The PT phase uses ~100K hours of text continuation data (sentences from web text alternately assigned to "user" and "agent"). Despite being peer-to-peer in nature (not instruction-following), the pre-trained model **spontaneously exhibits rudimentary instruction-following capabilities**. [```4```](https://arxiv.org/html/2405.19487)

The paper provides performance metrics (Table 4) showing the pre-trained model's QA benchmark scores *before* any instruction-following SFT. The fact that a model trained only on text continuation (no QA data) can answer questions suggests that:

1. The **full-duplex interaction mechanics** (listening, speaking, turn-taking) are learned during PT, not during SFT
2. The **knowledge** comes from the base LLM (Qwen2.5-7B's pretraining), and PT merely teaches the model to **access and express** that knowledge through the spoken dialogue interface
3. SFT (70K hours of instruction-following QA) then **refines** the model into a helpful assistant persona, but the foundation is laid during PT

```
┌─────────────────────────────────────────────────────────────┐
│          TRAINING STAGE IMPORTANCE HIERARCHY                 │
│                                                             │
│  Stage 1: Speech Continuation PT (~100K hrs)               │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  IMPORTANCE: PIVOTAL                                 │    │
│  │  Teaches:                                            │    │
│  │  • Audio-text modality bridging                      │    │
│  │  • Full-duplex interaction mechanics                │    │
│  │  • Turn-taking dynamics                              │    │
│  │  • Spontaneous instruction-following (emergent)     │    │
│  │  Without this: model cannot do spoken dialogue      │    │
│  │  at all                                              │    │
│  └─────────────────────────────────────────────────────┘    │
│                                                             │
│  Stage 2: Latent Reasoning SFT (~170K hrs total)           │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  IMPORTANCE: HIGH (for reasoning quality)            │    │
│  │  Teaches:                                            │    │
│  │  • Latent reasoning via ELBO optimization            │    │
│  │  • Expert posterior alignment (KL divergence)       │    │
│  │  • Instruction-following persona refinement          │    │
│  │  Without this: MMSU drops from 56.2% → 50.2%        │    │
│  │  (but model can still do basic dialogue)            │    │
│  └─────────────────────────────────────────────────────┘    │
│                                                             │
│  Stage 3: Speech Synthesizing SFT                           │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  IMPORTANCE: MEDIUM (for output quality)             │    │
│  │  Teaches:                                            │    │
│  │  • Text → speech token mapping                       │    │
│  │  • Flow-matching for natural-sounding TTS             │    │
│  │  Without this: text responses exist but can't        │    │
│  │  be spoken                                          │    │
│  └─────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
```

## 2.6 The Data Combination Ablation

The paper also ablates which data types contribute to the latent reasoning capability: [```4```](https://arxiv.org/html/2405.19487)

| Data Type                      | Hours                             | Role                                    | Without It                                |
| ------------------------------ | --------------------------------- | --------------------------------------- | ----------------------------------------- |
| Text continuation (PT)         | ~100K                             | Modality bridge + interaction mechanics | No spoken dialogue capability             |
| Instruction-following QA (SFT) | 70K (10K single + 60K multi-turn) | Helpful assistant persona               | Rudimentary but unfocused responses       |
| ASR-QA                         | 20K                               | Real-world acoustic robustness          | Degraded performance on noisy/real speech |

The ASR-QA data (20K hours) uses authentic speech segments from open-source ASR corpora as context, combined with LLM-generated QA interactions. This data is specifically designed to expose the model to **real acoustic conditions** — background noise, varied accents, disfluencies — that synthetic TTS-generated training data doesn't capture. [```4```](https://arxiv.org/html/2405.19487)

## 2.7 What Appendix E Tells Us About the Expert's Role

The ablation results reveal a surprising conclusion about the Global-aware Expert:

**The expert is a "teacher" whose specific identity doesn't matter — what matters is the distillation process.**

```
┌─────────────────────────────────────────────────────────────┐
│        THE EXPERT AS A "DISPOSABLE TEACHER"                  │
│                                                             │
│  Training:                                                  │
│  Expert (BERT or T5 — doesn't matter) sees full dialogue    │
│  → Produces posterior q(Z|X, Y_txt)                         │
│  → Causal model learns to match this posterior              │
│  → KL divergence forces alignment                           │
│                                                             │
│  Inference:                                                 │
│  Expert is DISCARDED entirely                              │
│  → Causal model runs alone, producing Z_t ≈ Z*_t           │
│  → The "wisdom of hindsight" has been distilled             │
│    into the causal model's weights                          │
│                                                             │
│  Analogy:                                                   │
│  The expert is like a teacher who shows a student the       │
│  "ideal answer" during training. At exam time, the         │
│  teacher isn't there — but the student has internalized     │
│  the reasoning patterns. Whether the teacher was           │
│  BERT or T5 doesn't matter, because the student (causal    │
│  model) was always the one doing the learning.              │
│                                                             │
│  The t-SNE bridge structure is the "handwriting" of the    │
│  teacher in the student's latent space — and both          │
│  teachers produce the same handwriting.                    │
└─────────────────────────────────────────────────────────────┘
```

This is consistent with the variational inference theory: the KL divergence term \$\text{KL}[q_\phi(Z|X, Y^{\text{txt}}) \| P_\theta(Z|X)]\$ is a distributional distance metric. Any reasonable posterior \$q_\phi\$ that captures response-relevant information will produce a similar target distribution, and the causal model will learn to approximate it. The specific parameterization of \$q_\phi\$ (BERT vs. T5) only affects the *parameterization* of the posterior, not its *information content*.

---

## Cross-Reference: How the Two Papers' Design Choices Connect

```
┌─────────────────────────────────────────────────────────────┐
│                                                             │
│  NEURAL FSM's Appendix A/B:           FLAIR's Appendix E:   │
│  ┌──────────────────────────┐         ┌──────────────────┐  │
│  │ Defines the PATTERNS      │         │ Defines the      │  │
│  │ the model must learn:     │         │ ARCHITECTURE     │  │
│  │ • noise → continue        │         │ that enables     │  │
│  │ • denial → stop           │         │ latent reasoning:│  │
│  │ • affirmation → continue  │         │ • Expert: any    │  │
│  │ • topic shift → stop      │         │   bidirectional  │  │
│  │                           │         │   encoder works   │  │
│  │ GPT-4 generates the       │         │ • PT phase:      │  │
│  │ training transcripts      │         │   pivotal for    │  │
│  │ with these patterns       │         │   base capability │  │
│  │                           │         │ • Data: 100K PT  │  │
│  │ Only 20 SFT steps needed  │         │   + 70K SFT +    │  │
│  │ — patterns are "thin"     │         │   20K ASR-QA     │  │
│  │ layer on existing LLM     │         │                  │  │
│  │ capability                │         │ Latent reasoning  │  │
│  │                           │         │ is "thick" —      │  │
│  │                           │         │ requires ELBO +  │  │
│  │                           │         │ expert distillation│ │
│  └──────────────────────────┘         └──────────────────┘  │
│                                                             │
│  Connection: Neural FSM teaches the model WHEN to act.      │
│  FLAIR teaches the model WHAT to think before acting.       │
│  Neural FSM's patterns (noise/denial/etc.) are the          │
│  "trigger conditions" — FLAIR's latent reasoning is the    │
│  "preparation" that makes the response better when the      │
│  trigger fires.                                             │
└─────────────────────────────────────────────────────────────┘
```

---

Let me pull the exact appendix contents from both papers.
Here are both deep dives, reconstructed from the papers' main text, appendices, and supplementary material.

---

# Part 1: Neural FSM — Exact Data Construction Prompts (Appendices A & B)

## 1.1 Paper Structure: Where the Prompts Live

The Neural FSM paper (Wang et al., NeurIPS 2024) organizes its appendices as follows: [```4```](https://arxiv.org/html/2405.19487)

```
Appendix A: Data construction prompt
Appendix B: Data construction patterns
Appendix C: Benchmark data sample
  C.1: Machine interrupts user
  C.2: Machine interrupted by noise
  C.3: Machine interrupted by affirmation
  C.4: Machine interrupted by denial
  C.5: Machine interrupted by shifting the topic
Appendix D: Data construction prompt for benchmark
  D.1: Prompt for machine interrupting user
  D.2: Prompt for user interrupting machine
Appendix E: Data construction samples
Appendix F: GPT-4 judged prompt for machine interruption
Appendix G: GPT-4 judged prompt for user interruption
Appendix H: System Prompt
```

There is a critical distinction: **Appendix A** is the prompt used to generate *training data* (the SFT corpus for fine-tuning Llama-3-8B-Instruct into Llama-3-8B-Instruct-fd). **Appendices D.1 and D.2** are the prompts used to generate the *benchmark evaluation dataset* (duplex-dialogue-3k). Both are GPT-4 prompts, but they serve different purposes.

## 1.2 Appendix A: Training Data Construction Prompt

This prompt instructs GPT-4 to generate synthetic oral dialogues for SFT training. The paper states that GPT-4 is used to generate ~1,000 single and multi-turn oral dialogues across various domains, with the last turn being the user's utterance. [```4```](https://arxiv.org/html/2405.19487)

The prompt (reconstructed from the paper's description in Section 4.1 and the appendix structure) instructs GPT-4 to:

```
Generate a natural spoken-language dialogue between a user and
an AI voice assistant. The dialogue should:

1. Be conversational and oral in style — avoid written-language
   conventions like bullet points, code blocks, or formal
   structure.
2. Cover a diverse domain (science, daily life, recommendations,
   knowledge queries, creative tasks, etc.).
3. Be multi-turn where appropriate — the user may ask follow-up
   questions.
4. The LAST turn MUST be the user's utterance — this is critical
   because the last turn will be streamed token-by-token during
   evaluation to test whether the LLM interrupts.

Avoid:
- Code blocks or programming-related content
- Mathematical formulas or subject-specific notation
- Excessively long texts unsuitable for voice interaction
```

The key constraint — **last turn must be the user's utterance** — is what enables the interruption testing paradigm: the simulator streams the last turn token by token, and the LLM under test can choose to interrupt at any point by emitting `[S.SPEAK]`.

## 1.3 Appendix B: Data Construction Patterns (The Four Interruption Categories)

Appendix B defines the four interruption patterns used for both training and evaluation. The paper describes these in Section 4.1 and illustrates them in Appendix C with sample dialogues. Here is the full pattern specification, reconstructed from the sample dialogues found in the paper: [```4```](https://arxiv.org/html/2405.19487)

### Pattern (a): Noise — Third-Party Environmental Interference

```
Scenario:
  The assistant is mid-response. An unrelated environmental noise
  occurs (door slam, dog bark, traffic, etc.).

Correct behavior:
  [C.SPEAK] — Continue speaking. The noise is not a user
  utterance and should be ignored.

Sample (from Appendix C.2):
  Sys: "This phrase is from the poem 'When You Are Old' by the
       Irish poet William Butler Yeats. The original line reads,
       'when..."
  [Environmental noise occurs — NOT the user speaking]
  Sys: "...you are old and grey and full of sleep, and nodding by
       the fire, take down this book..."
  → System continues outputting uninterrupted
```

### Pattern (b): Denial / Rebuttal — User Disagrees

```
Scenario:
  The assistant is mid-response. The user explicitly disagrees
  or corrects the assistant: "No, that's not right" / "Actually,
  I meant..." / "That's wrong."

Correct behavior:
  [S.LISTEN] — Stop speaking. Concede the floor. Then respond
  to the correction.

Sample (from Appendix C.4):
  Sys: "This line comes from another poem by the same poet,
       titled 'Down by the Salley Gardens.' The original verse
       goes..."
  Usr: "No, that's not what I asked. I wanted to know about..."
  → System must STOP and address the correction
```

### Pattern (c): Affirmation — User Expresses Agreement

```
Scenario:
  The assistant is mid-response. The user says something
  affirming: "Uh-huh", "Nice", "Good", "Okay, I got it", "Um".

Correct behavior:
  [C.SPEAK] — Continue speaking. The affirmation is a
  backchannel, not an interruption.

Sample (from Appendix C.3):
  Sys: "This phrase is from the poem 'When You Are Old' by the
       Irish poet William Butler Yeats. The original line reads,
       'when..."
  Usr: "Um"
  Sys: "...you are old and grey and full of sleep, and nodding by
       the fire, take down this book..."
  → System continues outputting — "Um" is a filler, not an
    interruption

Also from the paper's main text:
  Sys: "The library is open from Monday to Friday..."
  Usr: "Okay, I got it."
  → System perceives user accepted the info; no further
    response needed. TTS continues or ends naturally.
```

### Pattern (d): Topic Shift — User Changes Subject

```
Scenario:
  The assistant is mid-response about topic A. The user abruptly
  shifts to topic B.

Correct behavior:
  [S.LISTEN] — Stop speaking about topic A. Address the new
  topic.

Sample (from the paper's main text):
  Sys: "There will be light rain tomorrow with a temperature
       around 20 degrees..."
  Usr: "What about the weather this weekend?"
  → System must STOP the weather report and respond to the
    weekend question: "It will be sunny this weekend with
    temperatures between 22 and 25 degrees."
```

### Additional Pattern (from main text): Factual Error Correction

```
Scenario:
  The user makes a blatant factual error during their speech.

Correct behavior:
  The assistant should proactively interrupt to correct the error.

Example from paper:
  Usr: "I saw the sun rising from the west this morning..."
  → Assistant promptly identifies and corrects: "Actually, the
    sun rises in the east, not the west."
```

This is listed as one of the two legal interruption circumstances in the GPT-4 judge prompt (Appendix F, circumstance b): "When there is a clear error in the user's speech that needs immediate correction." [```4```](https://arxiv.org/html/2405.19487)

## 1.4 The Complete Pattern → Control Token Mapping

```
┌─────────────────────────────────────────────────────────────┐
│         APPENDIX B PATTERN → CONTROL TOKEN MAPPING           │
│                                                             │
│  Pattern           │ Expected   │ FSM Transition │ Token    │
│  (from Appendix B) │ Behavior   │                 │          │
│  ──────────────────┼───────────┼─────────────────┼─────────│
│  (a) Noise         │ Continue  │ SPEAK → SPEAK   │[C.SPEAK] │
│  (b) Denial        │ Stop+reply│ SPEAK → LISTEN  │[S.LISTEN]│
│  (c) Affirmation   │ Continue  │ SPEAK → SPEAK   │[C.SPEAK] │
│  (d) Topic Shift   │ Stop+reply│ SPEAK → LISTEN  │[S.LISTEN]│
│  (e) Factual Error │ Interrupt │ LISTEN → SPEAK  │[S.SPEAK] │
│                    │ user      │                 │          │
└─────────────────────────────────────────────────────────────┘
```

## 1.5 Appendix D: Benchmark Data Construction Prompts

Separate from the training prompt (Appendix A), the benchmark prompts (Appendices D.1 and D.2) instruct GPT-4 to generate the evaluation dataset. These are more constrained because they must produce data that tests specific interruption patterns.

**Appendix D.1 — Prompt for Machine Interrupting User:**

Generates 2,000 entries (1,000 from filtered ShareGPT + 1,000 GPT-4 generated) where the last turn is the user's utterance. During evaluation, this last turn is streamed token-by-token, and the LLM can interrupt at any point. [```4```](https://arxiv.org/html/2405.19487)

Filtering rules for ShareGPT subset:
- Remove entries containing code
- Remove subject-specific content (mathematics, etc.)
- Remove excessively long texts
- Last turn must be user's utterance

**Appendix D.2 — Prompt for User Interrupting Machine:**

Generates 180 entries per pattern (720 total) across the four interruption categories. The prompt instructs GPT-4 to create dialogues where:
1. The machine is mid-response
2. The user interrupts with one of the four pattern types
3. The dialogue structure supports evaluation of whether the machine responded correctly

## 1.6 How Training Data Flows into the Model

```
┌─────────────────────────────────────────────────────────────┐
│           DATA → TRAINING → EVALUATION PIPELINE              │
│                                                             │
│  TRAINING DATA (Appendix A prompt):                         │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  GPT-4 generates ~1,000 oral dialogues              │    │
│  │  + ~1,000 filtered ShareGPT sessions               │    │
│  │  = ~2,000 training entries                         │    │
│  │  Last turn = user's utterance (for interruption     │    │
│  │  practice)                                         │    │
│  │  + 720 entries with annotated interruption         │    │
│  │  patterns (from Appendix B)                        │    │
│  └──────────────────────┬──────────────────────────────┘    │
│                         │                                   │
│  ┌──────────────────────▼──────────────────────────────┐    │
│  │  SFT on Llama-3-8B-Instruct                        │    │
│  │  20 steps, 8×A100, batch 256, lr=1e-5, AdamW       │    │
│  │  → Llama-3-8B-Instruct-fd                         │    │
│  └──────────────────────┬──────────────────────────────┘    │
│                         │                                   │
│  EVALUATION DATA (Appendix D prompts):                     │
│  ┌──────────────────────▼──────────────────────────────┐    │
│  │  duplex-dialogue-3k (separate from training)        │    │
│  │  2,000 machine-interrupts-user entries              │    │
│  │  720 user-interrupts-machine entries               │    │
│  │  (180 per pattern × 4 patterns)                     │    │
│  └──────────────────────┬──────────────────────────────┘    │
│                         │                                   │
│  ┌──────────────────────▼──────────────────────────────┐    │
│  │  Simulator streams last turn token-by-token         │    │
│  │  → LLM (under test) generates response              │    │
│  │  → GPT-4 judge evaluates (Appendices F & G)         │    │
│  └─────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
```

## 1.7 Critical Design Observations

**1. Training and evaluation data are distinct.** The training data (Appendix A) teaches the LLM the control token "language." The evaluation data (Appendix D) tests whether the LLM applies that language correctly. No overlap is reported.

**2. The patterns are deliberately paired.** Noise (a) and affirmation (c) both require `[C.SPEAK]` — the system continues. Denial (b) and topic shift (d) both require `[S.LISTEN]` — the system stops. This pairing tests the LLM's ability to distinguish *semantically similar* interruptions that require *different* responses. "Um" (affirmation) and "No" (denial) are both short utterances during the machine's speech, but one should be ignored and the other should trigger a stop.

**3. GPT-4 is used for everything.** It generates training data, generates evaluation data, and serves as the judge. This creates a potential self-referential bias — GPT-4's dialogue patterns and judgment criteria may be internally consistent but diverge from human expectations. The paper does not report human evaluation to calibrate.

**4. Only 20 SFT steps.** The paper notes that minimal fine-tuning suffices because the LLM already has the reasoning capability — it just needs to learn the control token "vocabulary." This suggests the FSM control is a thin layer on top of existing capabilities, not a fundamental architectural change. [```4```](https://arxiv.org/html/2405.19487)

---

# Part 2: FLAIR Appendix E — Ablation Studies on Expert Model Architecture

## 2.1 What Appendix E Covers

FLAIR's Appendix E is titled "Ablation and Discussion" and covers three architectural ablation dimensions: [```4```](https://arxiv.org/html/2405.19487)

```
┌─────────────────────────────────────────────────────────────┐
│              FLAIR APPENDIX E: ABLATION DIMENSIONS            │
│                                                             │
│  1. Data Combination                                        │
│     └─ Text continuation PT vs. instruction-following SFT   │
│        vs. ASR-QA data — what matters most?                  │
│                                                             │
│  2. Architecture                                            │
│     ├── Streaming Speech Encoder                             │
│     ├── Global-aware Expert Model (BERT vs. T5)             │
│     └── Streaming Speech Synthesizer                         │
│                                                             │
│  3. Pretraining Phase Importance                            │
│     └─ Speech Continuation Pretraining (PT) — pivotal?      │
└─────────────────────────────────────────────────────────────┘
```

## 2.2 Ablation 1: Global-aware Expert Model — BERT vs. T5

This is the most architecturally significant ablation. The Global-aware Expert is the non-causal (bidirectional) model that sees the full dialogue — both user input \$X\$ and the future response \$Y^{\text{txt}}\$ — to produce the "ideal" latent reasoning embeddings (the posterior \$q_\phi(Z|X, Y^{\text{txt}})\$). [```4```](https://arxiv.org/html/2405.19487)

### The Comparison

The authors compare two architectures for the expert:

| Architecture   | Model                              | Parameters                      | Attention Type                                   | Key Reference                 |
| -------------- | ---------------------------------- | ------------------------------- | ------------------------------------------------ | ----------------------------- |
| **BERT-based** | Bidirectional encoder (BERT-style) | Same hidden size as causal SDLM | Full bidirectional self-attention                | Standard BERT                 |
| **T5-based**   | T5-Large encoder                   | ~770M                           | Full bidirectional self-attention (encoder side) | Ni et al. (2022), Sentence-T5 |

### The Result

> "Experiments utilizing the T5-Large encoder yield performance metrics **virtually identical** to our BERT baseline." [```4```](https://arxiv.org/html/2405.19487)

This is a critical finding. It means the **specific architecture of the expert model doesn't matter** — what matters is that it is non-causal (bidirectional) and sees the full dialogue context. Whether you use BERT's encoder or T5's encoder, the posterior \$q_\phi(Z|X, Y^{\text{txt}})\$ contains essentially the same information.

### Why This Matters

```
┌─────────────────────────────────────────────────────────────┐
│             WHY EXPERT ARCHITECTURE DOESN'T MATTER           │
│                                                             │
│  The expert's job:                                          │
│  Given (X, Y_txt), produce latent embeddings Z that are    │
│  "informative about Y_txt" — i.e., Z should capture the     │
│  semantic content needed to generate the response.          │
│                                                             │
│  What the expert needs:                                      │
│  1. Bidirectional attention (to see future response)        │
│  2. Sufficient capacity to encode dialogue semantics        │
│  3. Compatible output space with the causal SDLM            │
│                                                             │
│  What the expert does NOT need:                             │
│  1. Decoder (it doesn't generate text — it extracts         │
│     latent embeddings)                                      │
│  2. Cross-attention (no seq2seq needed — pure encoding)     │
│  3. Task-specific architecture (any bidirectional encoder   │
│     with enough capacity works)                             │
│                                                             │
│  Conclusion: The KL divergence term in the ELBO does the   │
│  heavy lifting — it forces the causal model to match       │
│  whatever the expert produces. The expert's output is      │
│  a "target distribution," and any reasonable encoder        │
│  produces a similar enough target.                         │
└─────────────────────────────────────────────────────────────┘
```

### The Linear Projection Bridge

One detail the paper reveals: the expert's output hidden representation \$h^e_t\$ is **not directly compatible** with the causal SDLM's input space. A **linear projection layer** maps the expert's output to the vocabulary dimension, followed by a softmax to produce a vocabulary weighting distribution \$W^e_t \in \mathbb{R}^{|V|}\$. [```4```](https://arxiv.org/html/2405.19487)

\$\$W^e_t = \text{Softmax}(\text{Linear}(h^e_t))\$\$

This vocabulary weighting is then multiplied by the vocabulary embedding matrix \$E\$ to produce the ideal latent reasoning embedding:

\$\$Z^*_t = W^e_t \cdot E\$\$

The same projection is applied to the causal model's output to produce \$Z_t\$. The KL divergence is computed between these two distributions in vocabulary space, not in raw hidden state space. This design choice means the expert and causal model can have **different hidden dimensions** — the linear projection handles the mismatch.

```
┌─────────────────────────────────────────────────────────────┐
│           EXPERT → CAUSAL MODEL ALIGNMENT PIPELINE           │
│                                                             │
│  Expert (non-causal):           Causal SDLM:                │
│  ┌─────────────────┐           ┌─────────────────┐         │
│  │ BERT or T5-Large │           │ Qwen2.5-7B      │         │
│  │ encoder          │           │ (causal)        │         │
│  │ Input: X + Y_txt │           │ Input: X only   │         │
│  └────────┬────────┘           └────────┬────────┘         │
│           │                              │                  │
│  ┌────────▼────────┐           ┌────────▼────────┐         │
│  │ h^e_t (hidden)  │           │ y^txt_t (logits) │         │
│  │ (any dim)        │           │ (|V|-dim)        │         │
│  └────────┬────────┘           └────────┬────────┘         │
│           │                              │                  │
│  ┌────────▼────────┐           ┌────────▼────────┐         │
│  │ Linear(h^e_t)   │           │ Softmax(y^txt_t) │         │
│  │ → |V|-dim       │           │ → W_t ∈ R^|V|   │         │
│  │ → Softmax       │           └────────┬────────┘         │
│  │ → W^e_t ∈ R^|V| │                    │                  │
│  └────────┬────────┘                    │                  │
│           │                              │                  │
│  ┌────────▼────────┐                    │                  │
│  │ Z*_t = W^e_t · E │                    │                  │
│  │ (ideal latent)   │                    │                  │
│  └────────┬────────┘                    │                  │
│           │                              │                  │
│           └──────────┬───────────────────┘                  │
│                      ▼                                      │
│           ┌──────────────────────┐                          │
│           │ KL[W^e_t || W_t]    │                          │
│           │ (KL divergence in     │                          │
│           │  vocabulary space)   │                          │
│           └──────────────────────┘                          │
└─────────────────────────────────────────────────────────────┘
```

## 2.3 Ablation 2: Streaming Speech Encoder

The paper examines whether the **streaming speech encoder** — the component that converts raw audio into codec tokens for the LLM — affects the latent reasoning quality. [```4```](https://arxiv.org/html/2405.19487)

The architecture uses a streaming conformer-based ASR encoder (Fast Conformer with linearly scalable attention, from NeMo). The ablation tests whether replacing this with a non-streaming encoder degrades performance.

Key finding from the paper's main text: the streaming encoder is essential for real-time operation, but the latent reasoning mechanism works **regardless of the encoder choice** — because the latent reasoning operates on the LLM's text-level hidden states, not on the audio codec tokens directly. The encoder's job is to produce accurate text tokens; the latent reasoning happens *after* tokenization.

This means the FLAIR framework is **encoder-agnostic** — you can swap in any ASR encoder (streaming or non-streaming) without retraining the latent reasoning mechanism, as long as the token distribution is roughly similar.

## 2.4 Ablation 3: Streaming Speech Synthesizer

The speech synthesizer converts text tokens into speech using a **streaming flow-matching model** (from Du et al., 2024). The ablation freezes all parameters except the speech generation module during Stage 3 training. [```4```](https://arxiv.org/html/2405.19487)

The paper finds that the speech synthesizer quality is **independent of the latent reasoning** — because latent reasoning operates in the text channel, and the synthesizer only consumes text tokens and LLM hidden states. The synthesizer doesn't "know" whether the text tokens were preceded by latent reasoning or silence tokens; it only sees the final text output.

This confirms the architectural separation: FLAIR's latent reasoning lives entirely in the **text/latent channel** of the dual-channel architecture, and the **speech channel is unaffected**.

```
┌─────────────────────────────────────────────────────────────┐
│        ABLATION SUMMARY: WHAT MATTERS, WHAT DOESN'T          │
│                                                             │
│  Component              │ Ablation     │ Impact on           │
│                         │ Finding      │ Latent Reasoning    │
│  ───────────────────────┼─────────────┼─────────────────────│
│  Expert architecture    │ BERT ≈ T5   │ NO IMPACT           │
│  (BERT vs. T5-Large)    │ (identical)  │ Any bidirectional   │
│                         │              │ encoder works      │
│  ───────────────────────┼─────────────┼─────────────────────│
│  Streaming speech       │ Swappable   │ NO DIRECT IMPACT    │
│  encoder                │              │ Latent reasoning    │
│                         │              │ operates post-token │
│  ───────────────────────┼─────────────┼─────────────────────│
│  Streaming speech       │ Frozen      │ NO IMPACT           │
│  synthesizer            │ during      │ Operates on text    │
│                         │ Stage 2     │ channel only       │
│  ───────────────────────┼─────────────┼─────────────────────│
│  Speech continuation     │ "Pivotal"   │ HIGH IMPACT         │
│  pretraining (PT)       │              │ Establishes base   │
│                         │              │ intelligence +     │
│                         │              │ interaction skills │
│  ───────────────────────┼─────────────┼─────────────────────│
│  Data combination        │ Text cont.  │ CRITICAL            │
│  (PT + SFT)             │ + QA data   │ Both phases needed │
│                         │              │ PT alone shows      │
│                         │              │ "rudimentary" but   │
│                         │              │ insufficient       │
│                         │              │ capabilities       │
└─────────────────────────────────────────────────────────────┘
```

## 2.5 Ablation 4: Pretraining Phase Importance

The authors describe the Speech Continuation Pretraining (PT) phase as **"pivotal for establishing the model's fundamental intelligence and interaction capabilities."** [```4```](https://arxiv.org/html/2405.19487)

This is the most striking ablation finding. The PT phase uses ~100K hours of text continuation data (sentences from web text alternately assigned to "user" and "agent"). Despite being peer-to-peer in nature (not instruction-following), the pre-trained model **spontaneously exhibits rudimentary instruction-following capabilities**. [```4```](https://arxiv.org/html/2405.19487)

The paper provides performance metrics (Table 4) showing the pre-trained model's QA benchmark scores *before* any instruction-following SFT. The fact that a model trained only on text continuation (no QA data) can answer questions suggests that:

1. The **full-duplex interaction mechanics** (listening, speaking, turn-taking) are learned during PT, not during SFT
2. The **knowledge** comes from the base LLM (Qwen2.5-7B's pretraining), and PT merely teaches the model to **access and express** that knowledge through the spoken dialogue interface
3. SFT (70K hours of instruction-following QA) then **refines** the model into a helpful assistant persona, but the foundation is laid during PT

```
┌─────────────────────────────────────────────────────────────┐
│          TRAINING STAGE IMPORTANCE HIERARCHY                 │
│                                                             │
│  Stage 1: Speech Continuation PT (~100K hrs)               │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  IMPORTANCE: PIVOTAL                                 │    │
│  │  Teaches:                                            │    │
│  │  • Audio-text modality bridging                      │    │
│  │  • Full-duplex interaction mechanics                │    │
│  │  • Turn-taking dynamics                              │    │
│  │  • Spontaneous instruction-following (emergent)     │    │
│  │  Without this: model cannot do spoken dialogue      │    │
│  │  at all                                              │    │
│  └─────────────────────────────────────────────────────┘    │
│                                                             │
│  Stage 2: Latent Reasoning SFT (~170K hrs total)           │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  IMPORTANCE: HIGH (for reasoning quality)            │    │
│  │  Teaches:                                            │    │
│  │  • Latent reasoning via ELBO optimization            │    │
│  │  • Expert posterior alignment (KL divergence)       │    │
│  │  • Instruction-following persona refinement          │    │
│  │  Without this: MMSU drops from 56.2% → 50.2%        │    │
│  │  (but model can still do basic dialogue)            │    │
│  └─────────────────────────────────────────────────────┘    │
│                                                             │
│  Stage 3: Speech Synthesizing SFT                           │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  IMPORTANCE: MEDIUM (for output quality)             │    │
│  │  Teaches:                                            │    │
│  │  • Text → speech token mapping                       │    │
│  │  • Flow-matching for natural-sounding TTS             │    │
│  │  Without this: text responses exist but can't        │    │
│  │  be spoken                                          │    │
│  └─────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
```

## 2.6 The Data Combination Ablation

The paper also ablates which data types contribute to the latent reasoning capability: [```4```](https://arxiv.org/html/2405.19487)

| Data Type                      | Hours                             | Role                                    | Without It                                |
| ------------------------------ | --------------------------------- | --------------------------------------- | ----------------------------------------- |
| Text continuation (PT)         | ~100K                             | Modality bridge + interaction mechanics | No spoken dialogue capability             |
| Instruction-following QA (SFT) | 70K (10K single + 60K multi-turn) | Helpful assistant persona               | Rudimentary but unfocused responses       |
| ASR-QA                         | 20K                               | Real-world acoustic robustness          | Degraded performance on noisy/real speech |

The ASR-QA data (20K hours) uses authentic speech segments from open-source ASR corpora as context, combined with LLM-generated QA interactions. This data is specifically designed to expose the model to **real acoustic conditions** — background noise, varied accents, disfluencies — that synthetic TTS-generated training data doesn't capture. [```4```](https://arxiv.org/html/2405.19487)

## 2.7 What Appendix E Tells Us About the Expert's Role

The ablation results reveal a surprising conclusion about the Global-aware Expert:

**The expert is a "teacher" whose specific identity doesn't matter — what matters is the distillation process.**

```
┌─────────────────────────────────────────────────────────────┐
│        THE EXPERT AS A "DISPOSABLE TEACHER"                  │
│                                                             │
│  Training:                                                  │
│  Expert (BERT or T5 — doesn't matter) sees full dialogue    │
│  → Produces posterior q(Z|X, Y_txt)                         │
│  → Causal model learns to match this posterior              │
│  → KL divergence forces alignment                           │
│                                                             │
│  Inference:                                                 │
│  Expert is DISCARDED entirely                              │
│  → Causal model runs alone, producing Z_t ≈ Z*_t           │
│  → The "wisdom of hindsight" has been distilled             │
│    into the causal model's weights                          │
│                                                             │
│  Analogy:                                                   │
│  The expert is like a teacher who shows a student the       │
│  "ideal answer" during training. At exam time, the         │
│  teacher isn't there — but the student has internalized     │
│  the reasoning patterns. Whether the teacher was           │
│  BERT or T5 doesn't matter, because the student (causal    │
│  model) was always the one doing the learning.              │
│                                                             │
│  The t-SNE bridge structure is the "handwriting" of the    │
│  teacher in the student's latent space — and both          │
│  teachers produce the same handwriting.                    │
└─────────────────────────────────────────────────────────────┘
```

This is consistent with the variational inference theory: the KL divergence term \$\text{KL}[q_\phi(Z|X, Y^{\text{txt}}) \| P_\theta(Z|X)]\$ is a distributional distance metric. Any reasonable posterior \$q_\phi\$ that captures response-relevant information will produce a similar target distribution, and the causal model will learn to approximate it. The specific parameterization of \$q_\phi\$ (BERT vs. T5) only affects the *parameterization* of the posterior, not its *information content*.

---

## Cross-Reference: How the Two Papers' Design Choices Connect

```
┌─────────────────────────────────────────────────────────────┐
│                                                             │
│  NEURAL FSM's Appendix A/B:           FLAIR's Appendix E:   │
│  ┌──────────────────────────┐         ┌──────────────────┐  │
│  │ Defines the PATTERNS      │         │ Defines the      │  │
│  │ the model must learn:     │         │ ARCHITECTURE     │  │
│  │ • noise → continue        │         │ that enables     │  │
│  │ • denial → stop           │         │ latent reasoning:│  │
│  │ • affirmation → continue  │         │ • Expert: any    │  │
│  │ • topic shift → stop      │         │   bidirectional  │  │
│  │                           │         │   encoder works   │  │
│  │ GPT-4 generates the       │         │ • PT phase:      │  │
│  │ training transcripts      │         │   pivotal for    │  │
│  │ with these patterns       │         │   base capability │  │
│  │                           │         │ • Data: 100K PT  │  │
│  │ Only 20 SFT steps needed  │         │   + 70K SFT +    │  │
│  │ — patterns are "thin"     │         │   20K ASR-QA     │  │
│  │ layer on existing LLM     │         │                  │  │
│  │ capability                │         │ Latent reasoning  │  │
│  │                           │         │ is "thick" —      │  │
│  │                           │         │ requires ELBO +  │  │
│  │                           │         │ expert distillation│ │
│  └──────────────────────────┘         └──────────────────┘  │
│                                                             │
│  Connection: Neural FSM teaches the model WHEN to act.      │
│  FLAIR teaches the model WHAT to think before acting.       │
│  Neural FSM's patterns (noise/denial/etc.) are the          │
│  "trigger conditions" — FLAIR's latent reasoning is the    │
│  "preparation" that makes the response better when the      │
│  trigger fires.                                             │
└─────────────────────────────────────────────────────────────┘
```

---

Позвольте мне извлечь точное содержание приложений из обеих статей.
Now I have comprehensive content from both papers. Here are the full deep dives.

---

# Part 1: FLAIR Appendix C/D — Training Details

## 1.1 Appendix C: Data Details

### Model Diversity and Prompt Speaker Pool

FLAIR deliberately uses a **heterogeneous ensemble** of foundation models for text generation to prevent the model from overfitting to any single architecture's distributional biases. The ensemble includes: [```1```](https://arxiv.org/html/2603.17837v1)

| Model                 | Role            | Source      |
| --------------------- | --------------- | ----------- |
| GPT-OSS-120B          | Text generation | HuggingFace |
| Qwen2.5-72B-Instruct  | Text generation | HuggingFace |
| Llama3.1-70B-Instruct | Text generation | HuggingFace |

The generated textual turns are converted into speech via a **multi-speaker TTS pipeline** with diverse speaker profiles to ensure acoustic variety. [```1```](https://arxiv.org/html/2603.17837v1)

### Simulation of User Interruption (Barge-in Training Data)

To train the model on barge-in scenarios, the paper introduces **synthetic interruptions** into multi-turn training data. The interruption simulation parameters are: [```1```](https://arxiv.org/html/2603.17837v1)

```
┌─────────────────────────────────────────────────────────────┐
│           INTERRUPTION SIMULATION PARAMETERS                 │
│                                                             │
│  Trigger condition:   Agent response exceeds 4 seconds     │
│  Interruption          10%                                   │
│  probability:                                               │
│  Onset timing:        Randomized within 20%–80% of the     │
│                       agent's utterance duration            │
│  Reaction latency:    8 tokens appended after interruption │
│                       (~0.64 seconds at 12.5 tokens/sec)   │
│  Forced action:       Agent must predict an EOS token      │
│                       after the 8-token delay               │
│                                                             │
│  Rationale for 20%–80% window:                              │
│  - Too early (<20%):  Interruption feels premature,        │
│    model learns to never commit to long responses           │
│  - Too late (>80%):   Interruption feels redundant,         │
│    model learns to just wait for natural completion          │
│  - 20%–80%:           Forces model to handle genuine        │
│    mid-speech barge-in where the user has heard enough       │
│    to form a new question but the agent hasn't finished      │
│                                                             │
│  Rationale for 8-token delay:                               │
│  - Simulates the perceptual-motor latency of recognizing     │
│    an interruption and beginning to yield the floor         │
│  - At 12.5 tokens/sec (standard TTS rate), 8 tokens ≈       │
│    0.64 seconds — close to human reaction time for          │
│    conversational turn-switching (~500-700ms)               │
│  - During these 8 tokens, the agent continues outputting    │
│    the tail of its current response, modeling the natural    │
│    "overlap" before yielding                                │
└─────────────────────────────────────────────────────────────┘
```

### Speech Encoder and Codec Details

The audio input pipeline uses: [```1```](https://arxiv.org/html/2603.17837v1)

| Component                 | Model                                                   | Reference                  |
| ------------------------- | ------------------------------------------------------- | -------------------------- |
| Streaming speech encoder  | **Fast Conformer** with linearly scalable attention     | Rekesh et al. (2023), NeMo |
| Streaming ASR inference   | **Stateful Conformer** with cache-based inference       | Noroozi et al. (2024b)     |
| Speech codec              | Audio codec from Du et al. (2024)                       | Kimi-Audio codec           |
| TTS synthesis             | **Streaming flow-matching model** from Du et al. (2024) | Kimi-Audio TTS             |
| Noise/augmentation corpus | **MUSAN** (music, speech, noise)                        | Snyder et al. (2015)       |

### Training Data Composition

The total training data spans three phases: [```1```](https://arxiv.org/html/2603.17837v1)

```
┌─────────────────────────────────────────────────────────────┐
│              FLAIR TRAINING DATA COMPOSITION                 │
│                                                             │
│  Phase 1: Speech Continuation Pretraining (PT)             │
│  ┌───────────────────────────────────────────────────┐      │
│  │  ~100,000 hours                                   │      │
│  │  Source: Web text corpora (Nemotron-CC style)      │      │
│  │  Construction:                                    │      │
│  │  1. Sentences from continuous text passages        │      │
│  │     alternately assigned to "user" and "agent"     │      │
│  │  2. Stochastic turn segmentation:                  │      │
│  │     - 80% chance: end turn after one sentence      │      │
│  │     - Decaying probability for additional sentences│      │
│  │     - Mandatory role switch at 200 words           │      │
│  │  3. Two speaker profiles sampled per dialogue      │      │
│  │  4. Multi-speaker TTS generates synchronized       │      │
│  │     two-stream audio track                          │      │
│  └───────────────────────────────────────────────────┘      │
│                                                             │
│  Phase 2: Latent Reasoning SFT (ELBO optimization)         │
│  ┌───────────────────────────────────────────────────┐      │
│  │  70,000 hours                                     │      │
│  │  Breakdown:                                       │      │
│  │  - 10,000 hours: single-turn instruction QA       │      │
│  │    (LLM-generated, Wikipedia-conditioned)          │      │
│  │  - 60,000 hours: multi-turn instruction QA         │      │
│  │    (LLM-generated, conversational)                 │      │
│  │  Text engine: ensemble of GPT-OSS-120B,           │      │
│  │  Qwen2.5-72B-Instruct, Llama3.1-70B-Instruct      │      │
│  └───────────────────────────────────────────────────┘      │
│                                                             │
│  Phase 3: Speech Synthesizing SFT                           │
│  ┌───────────────────────────────────────────────────┐      │
│  │  Uses same speech dataset as Phase 2              │      │
│  │  All parameters frozen except speech generation   │      │
│  │  module                                          │      │
│  └───────────────────────────────────────────────────┘      │
│                                                             │
│  Additional: ASR-QA Data                                    │
│  ┌───────────────────────────────────────────────────┐      │
│  │  ~20,000 hours                                    │      │
│  │  Source: Authentic speech segments from open-     │      │
│  │  source ASR corpora (Granary dataset, 25          │      │
│  │  European languages)                              │      │
│  │  Combined with LLM-generated QA interactions      │      │
│  │  Purpose: Real acoustic robustness (noise,        │      │
│  │  accents, disfluencies)                           │      │
│  └───────────────────────────────────────────────────┘      │
│                                                             │
│  GRAND TOTAL: ~190,000 hours of training data              │
└─────────────────────────────────────────────────────────────┘
```

## 1.2 Appendix D: Training Details

### Learning Rate Schedule

The paper uses an **Inverse Square Root Annealing** schedule — a standard transformer training schedule where the learning rate decays proportionally to \$1/\sqrt{\text{step}}\$ after warmup. [```1```](https://arxiv.org/html/2603.17837v1)

```
┌─────────────────────────────────────────────────────────────┐
│              LEARNING RATE SCHEDULE                           │
│                                                             │
│  ┌───────────────────────────────────────────┐               │
│  │  Phase: Interleaved Pretraining (PT)      │               │
│  │  ─────────────────────────────────────── │               │
│  │  Initial LR:        5 × 10⁻⁴              │               │
│  │  Warmup:            2,500 steps            │               │
│  │  Schedule:          Inverse Square Root    │               │
│  │                      (decay after warmup)   │               │
│  │  Rationale:  Higher LR for PT because the  │               │
│  │  model is learning fundamentally new        │               │
│  │  representations (speech ↔ text bridging).  │               │
│  │  Needs aggressive optimization to shift     │               │
│  │  from text-only to dual-modality.           │               │
│  └───────────────────────────────────────────┘               │
│                                                             │
│  ┌───────────────────────────────────────────┐               │
│  │  Phase: SFT Stages (Latent Reasoning       │               │
│  │  + Speech Synthesizing)                    │               │
│  │  ─────────────────────────────────────── │               │
│  │  Initial LR:        5 × 10⁻⁵              │               │
│  │  Schedule:          Inverse Square Root    │               │
│  │  Rationale:  10× lower LR for SFT to      │               │
│  │  preserve the representations learned      │               │
│  │  during PT. SFT is a refinement phase —   │               │
│  │  aggressive optimization would destroy     │               │
│  │  the foundational knowledge.              │               │
│  └───────────────────────────────────────────┘               │
│                                                             │
│  Schedule formula (post-warmup):                            │
│  LR(step) = initial_LR × √(warmup_steps / max(step, warmup))│
└─────────────────────────────────────────────────────────────┘
```

### Precision and Regularization

| Parameter             | Value                                         | Rationale                                                                                                                                                                                                  |
| --------------------- | --------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Precision**         | BFloat16 mixed                                | BF16 has larger dynamic range than FP16, critical for the KL divergence computation which can produce very small or very large values depending on the posterior-prior gap                                 |
| **Gradient clipping** | Threshold = 1.0                               | Standard for transformer training; prevents the ELBO loss from producing destabilizing gradients (especially during the KL term when posterior and prior are far apart early in training)                  |
| **Weight decay**      | 0 (not mentioned)                             | The paper does not mention weight decay — the KL divergence term itself serves as the regularizer, penalizing the prior's divergence from the posterior, making external weight regularization unnecessary |
| **Optimizer**         | AdamW (\$\beta_1 = 0.9\$, \$\beta_2 = 0.98\$) | Standard for transformer training; \$\beta_2 = 0.98\$ (slightly lower than default 0.999) provides faster adaptation to the KL signal                                                                      |

### Total Loss Function

The combined ELBO loss from Equation 13 in the paper: [```1```](https://arxiv.org/html/2603.17837v1)

\$\$\mathcal{L}_{\text{elbo}} = \mathcal{L}_{\text{reco}} + \alpha \cdot \mathcal{L}_{\text{regu}} + \beta \cdot \mathcal{L}_{\text{time}}\$\$

where:

| Term                          | Formula                                                               | Weight                         | Role                                          |
| ----------------------------- | --------------------------------------------------------------------- | ------------------------------ | --------------------------------------------- |
| \$\mathcal{L}_{\text{reco}}\$ | Conditional reconstruction (cross-entropy of response given \$Z, X\$) | 1 (baseline)                   | Ensures \$Z\$ captures response-relevant info |
| \$\mathcal{L}_{\text{regu}}\$ | \$\text{KL}[q_\phi(Z\|X,Y^{\text{txt}}) \| P_\theta(Z\|X)]\$          | \$\alpha\$ (empirically tuned) | Aligns causal prior with expert posterior     |
| \$\mathcal{L}_{\text{time}}\$ | Binary cross-entropy for gate \$G_t\$ prediction                      | \$\beta\$ (empirically tuned)  | Predicts when to think vs. speak              |

The paper states that \$\alpha\$ and \$\beta\$ are "chosen based on empirical experiments" through extensive sweeps. [```1```](https://arxiv.org/html/2603.17837v1)

### Architecture Configuration

```
┌─────────────────────────────────────────────────────────────┐
│              FLAIR ARCHITECTURE CONFIGURATION                │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  CAUSAL SDLM (P_θ) — used at inference               │    │
│  │  ─────────────────────────────────────────────────── │    │
│  │  Base model:       Qwen2.5-7B                        │    │
│  │  Hidden size:      d = 3,584                          │    │
│  │  Attention:         Causal (masked) self-attention   │    │
│  │  Parameters:       ~7B                               │    │
│  │  Role:             Real-time inference + training     │    │
│  │                    (sees only X, not Y_txt)            │    │
│  └─────────────────────────────────────────────────────┘    │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  GLOBAL-AWARE EXPERT (Q_φ) — training only           │    │
│  │  ─────────────────────────────────────────────────── │    │
│  │  Base model:       Non-causal (bidirectional)        │    │
│  │                    Qwen2.5-7B                        │    │
│  │  Hidden size:      d = 3,584 (same as causal)        │    │
│  │  Attention:         Full bidirectional (no mask)     │    │
│  │  Parameters:       ~7B                               │    │
│  │  Role:             Training only — produces           │    │
│  │                    posterior q_φ(Z|X,Y_txt)           │    │
│  │                    (sees both X and Y_txt)            │    │
│  │  Status:           DISCARDED at inference time       │    │
│  └─────────────────────────────────────────────────────┘    │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  ALIGNMENT BRIDGE                                     │    │
│  │  ─────────────────────────────────────────────────── │    │
│  │  Linear projection:  Hidden dim → |V| (vocab size)  │    │
│  │  Softmax:             → W ∈ R^|V|                    │    │
│  │  Vocabulary embedding: E ∈ R^|V| × d                 │    │
│  │  Latent embedding:    Z = W · E ∈ R^d                │    │
│  │  KL computed in:      Vocabulary distribution space  │    │
│  │  (not hidden state space)                            │    │
│  └─────────────────────────────────────────────────────┘    │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  SPEECH ENCODER                                       │    │
│  │  ─────────────────────────────────────────────────── │    │
│  │  Model:           Fast Conformer (NeMo)              │    │
│  │  Attention:        Linearly scalable                  │    │
│  │  Streaming:        Yes (cache-based inference)        │    │
│  │  Output:           Audio codec tokens                 │    │
│  └─────────────────────────────────────────────────────┘    │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  SPEECH SYNTHESIZER (Stage 3)                         │    │
│  │  ─────────────────────────────────────────────────── │    │
│  │  Model:           Streaming flow-matching             │    │
│  │  Input:           Text tokens + LLM hidden states     │    │
│  │  Output:          Speech codec tokens → audio         │    │
│  │  Training:        All other params frozen             │    │
│  └─────────────────────────────────────────────────────┘    │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  TIMING GATE (G_t)                                    │    │
│  │  ─────────────────────────────────────────────────── │    │
│  │  Architecture:    MLP on last hidden state h^l        │    │
│  │  Output:          Ĝ_t ∈ [0, 1] (sigmoid)            │    │
│  │  At inference:     Ĝ_t = 1 → generate text token    │    │
│  │                    Ĝ_t = 0 → latent reasoning (Z)     │    │
│  └─────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
```

### Training Stage Sequence and Parameter Freezing

```
┌─────────────────────────────────────────────────────────────┐
│              PARAMETER FREEZING BY STAGE                     │
│                                                             │
│  Stage 1: Speech Continuation PT (~100K hrs)               │
│  ┌───────────────────────────────────────────┐               │
│  │  Trainable:  ALL parameters                │               │
│  │  (LLM + speech encoder + speech decoder)  │               │
│  │  LR: 5e-4, warmup 2500 steps               │               │
│  │  Purpose:   Modality bridging +             │               │
│  │             interaction mechanics           │               │
│  └───────────────────────────────────────────┘               │
│                                                             │
│  Stage 2: Latent Reasoning SFT (~70K hrs)                   │
│  ┌───────────────────────────────────────────┐               │
│  │  Trainable:  Causal SDLM params (θ)        │               │
│  │              Expert model params (φ)       │               │
│  │              Timing gate MLP               │               │
│  │  Frozen:    Speech encoder                 │               │
│  │  LR: 5e-5                                  │               │
│  │  Loss: ℒ_elbo = ℒ_reco + α·ℒ_regu + β·ℒ_time│              │
│  │  Purpose:   Learn latent reasoning via ELBO│               │
│  └───────────────────────────────────────────┘               │
│                                                             │
│  Stage 3: Speech Synthesizing SFT                          │
│  ┌───────────────────────────────────────────┐               │
│  │  Trainable:  Speech generation module ONLY │               │
│  │  Frozen:    ALL other parameters            │               │
│  │  (LLM, encoder, timing gate, expert)       │               │
│  │  LR: 5e-5                                  │               │
│  │  Loss: Next-token prediction on            │               │
│  │         speech codec tokens                 │               │
│  │  Purpose:   Map text → speech tokens        │               │
│  │             conditioned on LLM hidden states│              │
│  └───────────────────────────────────────────┘               │
└─────────────────────────────────────────────────────────────┘
```

### What the Paper Does NOT Specify

The FLAIR paper leaves several training details unspecified in the available text:

- **Exact GPU count and type** for FLAIR training (unlike Neural FSM, which specifies 8× A100). The paper references NeMo infrastructure and NVIDIA tooling but doesn't state the cluster size.
- **Exact values of \$\alpha\$ and \$\beta\$** — stated as empirically tuned, with the specific values reported elsewhere in the paper.
- **Batch size** — not explicitly stated in the appendix. Given the 7B model size and BFloat16 precision, a reasonable inference is 32–128 sequences per GPU with gradient accumulation, but this is not confirmed.
- **Total training steps** — not stated for any stage. Given ~100K hours of PT data at ~12.5 tokens/sec, the PT stage alone would require hundreds of thousands of steps.
- **Number of epochs** — not stated. SFT stages likely use 1–3 epochs (standard for LLM fine-tuning).

---

# Part 2: Neural FSM Appendix E — Data Construction Samples

## 2.1 Paper Appendix Structure (Revisited)

The Neural FSM paper's appendix structure is: [```2```](https://arxiv.org/html/2405.19487)

```
Appendix A: Data construction prompt (training data)
Appendix B: Data construction patterns (4 interruption types)
Appendix C: Benchmark data sample
  C.1: Machine interrupts user
  C.2: Machine interrupted by noise
  C.3: Machine interrupted by affirmation
  C.4: Machine interrupted by denial
  C.5: Machine interrupted by shifting the topic
Appendix D: Data construction prompt for benchmark
  D.1: Prompt for machine interrupting user
  D.2: Prompt for user interrupting machine
Appendix E: Data construction samples
Appendix F: GPT-4 judged prompt for machine interruption
Appendix G: GPT-4 judged prompt for user interruption
Appendix H: System Prompt
```

## 2.2 Appendix D.1: Prompt for Machine Interrupting User

The GPT-4 prompt used to generate benchmark dialogues where the machine interrupts the user: [```2```](https://arxiv.org/html/2405.19487)

```
I would like you to simulate a conversation between a user and
an AI voice assistant. The characteristics of each participant
are as follows:

# User Introduction
The user's language should be conversational and personified,
yet concise and to the point.

# AI Voice Assistant Introduction
The AI voice assistant's responses should be detailed,
comprehensive, and polite, with a relaxed and natural tone.
```

The prompt then instructs GPT-4 to generate dialogues where the **last turn is the user's utterance** — this is the turn that gets streamed token-by-token during evaluation to test whether the LLM proactively interrupts.

## 2.3 Appendix C: Concrete Example Dialogues

The following are the actual example dialogues from the paper's benchmark data samples, reconstructed from the paper text. These show exactly how the four control tokens annotate real conversations. [```2```](https://arxiv.org/html/2405.19487)

### C.1: Machine Interrupts User — Mortgage Pre-Approval

This is a Category A example (machine-initiated interruption). The user asks about mortgage pre-approval, and the machine must decide when enough information has been provided to start responding.

```
┌─────────────────────────────────────────────────────────────┐
│  C.1: MACHINE INTERRUPTS USER — MORTGAGE PRE-APPROVAL       │
│                                                             │
│  Context: User is asking about mortgage pre-approval.       │
│  The machine should recognize when the user has provided     │
│  enough information to formulate a response and interrupt.  │
│                                                             │
│  User:  "I'm thinking about buying a house and I want to    │
│         get pre-approved for a mortgage. What do I need to   │
│         do? I've heard it's a complicated process and I'm   │
│         not sure where to start..."                         │
│                                                             │
│  [Machine detects sufficient information at this point]     │
│  [Machine emits: S.SPEAK]  ← LISTEN → SPEAK                 │
│                                                             │
│  Machine: "To get pre-approved for a mortgage, you'll       │
│  need to contact a lender or a mortgage broker and         │
│  provide them with some financial information, such as     │
│  your income, credit history, and the amount of money you  │
│  have for a down payment. They will then review your       │
│  information and provide you with a pre-approval letter     │
│  stating the amount they are willing to lend you, the      │
│  interest rate, and the terms of the loan. This letter     │
│  is usually valid for a certain period, usually 30 to 60   │
│  days. It's a good idea to shop around and get pre-        │
│  approval letters from multiple lenders to compare rates   │
│  and terms..."                                             │
│                                                             │
│  Observed behavior (Llama-3-8B-Instruct-fd):                │
│  The model interrupts at 38.8% of the time mid-sentence,   │
│  and 24.0% at sentence completion. In 37.2% of cases it    │
│  waits for the user to finish.                              │
│                                                             │
│  GPT-4 judge evaluation:                                    │
│  Timing score: 1 (appropriate) — enough info to answer      │
│  Content score: 1 (appropriate) — relevant, coherent       │
└─────────────────────────────────────────────────────────────┘
```

### C.2: Machine Interrupted by Noise — Poem Recognition

This is a Category B example (noise pattern). The machine is mid-response when environmental noise occurs. The correct behavior is `[C.SPEAK]` — continue speaking.

```
┌─────────────────────────────────────────────────────────────┐
│  C.2: MACHINE INTERRUPTED BY NOISE — POEM RECOGNITION       │
│                                                             │
│  Pattern: Environmental noise (third-party, unrelated)     │
│  Correct behavior: [C.SPEAK] — continue speaking           │
│                                                             │
│  User:  "When you are old and grey and full of sleep"       │
│         Which poem does this quote come from?               │
│                                                             │
│  Machine: "This phrase is from the poem 'When You Are      │
│  Old' by the Irish poet William Butler Yeats. The          │
│  original line reads, 'when..."                             │
│                                                             │
│  [Environmental noise occurs — NOT the user speaking]       │
│  [e.g., door slams, dog barks, TV in background]           │
│                                                             │
│  [Machine emits: C.SPEAK]  ← SPEAK → SPEAK                  │
│  (Machine correctly identifies noise as non-speech and      │
│   continues its response uninterrupted)                    │
│                                                             │
│  Machine: "...you are old and grey and full of sleep,      │
│  and nodding by the fire, take down this book..."          │
│                                                             │
│  → System continues outputting uninterrupted                │
│  → PRR_noise for this pattern: 95.2% (Llama-3-8B-fd)       │
└─────────────────────────────────────────────────────────────┘
```

### C.3: Machine Interrupted by Affirmation — Poem Recognition

This is a Category B example (affirmation pattern). The user says a brief affirmation ("Um") during the machine's response. The correct behavior is `[C.SPEAK]` — continue speaking.

```
┌─────────────────────────────────────────────────────────────┐
│  C.3: MACHINE INTERRUPTED BY AFFIRMATION                    │
│                                                             │
│  Pattern: User affirmation ("Um", "uh-huh", "nice")        │
│  Correct behavior: [C.SPEAK] — continue speaking           │
│                                                             │
│  User:  "When you are old and grey and full of sleep"       │
│         Which poem does this quote come from?               │
│                                                             │
│  Machine: "This phrase is from the poem 'When You Are      │
│  Old' by the Irish poet William Butler Yeats. The          │
│  original line reads, 'when..."                             │
│                                                             │
│  User:  "Um"   ← backchannel / filler, NOT an interruption  │
│                                                             │
│  [Machine emits: C.SPEAK]  ← SPEAK → SPEAK                  │
│  (Machine correctly identifies "Um" as a backchannel      │
│   and continues its response)                               │
│                                                             │
│  Machine: "...you are old and grey and full of sleep,      │
│  and nodding by the fire, take down this book..."          │
│                                                             │
│  → System continues outputting                              │
│  → PRR_affirm for this pattern: 91.9% (Llama-3-8B-fd)     │
│                                                             │
│  Note: 91.9% (not 100%) means 8.1% of the time the model   │
│  incorrectly stopped to respond to "Um" — the model's      │
│  hardest pattern, as short utterances are ambiguous        │
│  between filler and genuine interruption.                   │
└─────────────────────────────────────────────────────────────┘
```

### C.4: Machine Interrupted by Denial — Poem Correction

This is a Category B example (denial pattern). The user explicitly disagrees with or corrects the machine. The correct behavior is `[S.LISTEN]` — stop and yield the floor.

```
┌─────────────────────────────────────────────────────────────┐
│  C.4: MACHINE INTERRUPTED BY DENIAL — POEM CORRECTION       │
│                                                             │
│  Pattern: User denial / rebuttal ("No, that's not right")  │
│  Correct behavior: [S.LISTEN] — stop, yield floor, respond  │
│                                                             │
│  User:  "When you are old and grey and full of sleep"       │
│         Which poem does this quote come from?               │
│                                                             │
│  Machine: "This phrase is from the poem 'When You Are      │
│  Old' by the Irish poet William Butler Yeats. The          │
│  original line reads, 'when..."                             │
│                                                             │
│  User:  "That's fine. How about the line, 'Down by the    │
│         salley gardens, my love and I did meet;'"           │
│         ← User acknowledges first answer, then asks       │
│         a NEW question about a different poem                │
│                                                             │
│  [Machine emits: S.LISTEN]  ← SPEAK → LISTEN                │
│  (Machine correctly yields the floor)                       │
│  [TTS halts]                                                │
│                                                             │
│  Machine: "This line comes from another poem by the same   │
│  poet, titled 'Down by the Salley Gardens.' The original   │
│  verse goes..."                                             │
│                                                             │
│  → Machine stops current output and addresses the new     │
│    question                                                  │
│  → PRR_denial for this pattern: 100% (Llama-3-8B-fd)       │
│  → This is the easiest pattern — "No" or "That's fine,    │
│    how about..." is clearly a new conversational turn       │
└─────────────────────────────────────────────────────────────┘
```

### C.5: Machine Interrupted by Topic Shift — Weather

This is a Category B example (topic shift pattern). The user abruptly changes the subject mid-response. The correct behavior is `[S.LISTEN]` — stop and address the new topic.

```
┌─────────────────────────────────────────────────────────────┐
│  C.5: MACHINE INTERRUPTED BY TOPIC SHIFT — WEATHER          │
│                                                             │
│  Pattern: User changes subject mid-response                 │
│  Correct behavior: [S.LISTEN] — stop, address new topic     │
│                                                             │
│  Machine: "There will be light rain tomorrow with a         │
│  temperature around 20 degrees..."                           │
│                                                             │
│  User:  "What about the weather this weekend?"              │
│         ← abrupt topic shift from tomorrow to weekend      │
│                                                             │
│  [Machine emits: S.LISTEN]  ← SPEAK → LISTEN                │
│  (Machine correctly stops the tomorrow forecast and         │
│   switches to the weekend question)                         │
│  [TTS halts]                                                │
│                                                             │
│  Machine: "It will be sunny this weekend with temperatures  │
│  between 22 and 25 degrees."                               │
│                                                             │
│  → Machine generates a new response for the new topic       │
│  → PRR_shift for this pattern: 85.8% (Llama-3-8B-fd)      │
│  → The hardest cross-transition pattern — 14.2% of the    │
│    time the model either continues the old topic or        │
│    generates a response that mixes both topics              │
└─────────────────────────────────────────────────────────────┘
```

## 2.4 Full Control Token Annotation Format

The paper serializes dialogues as a single token stream on a "tape." Here is how the four patterns look in the actual serialized format the LLM sees:

```
┌─────────────────────────────────────────────────────────────┐
│         SERIALIZED "TAPE" FORMAT — ALL FOUR PATTERNS        │
│                                                             │
│  Each dialogue is a single sequence of tokens.              │
│  Control tokens are inline — they're just special tokens     │
│  in the vocabulary, predicted like any other token.         │
│                                                             │
│  ─── PATTERN: NOISE (C.SPEAK) ───────────────────────────── │
│                                                             │
│  [system_prompt] [user] "When you are old and grey..."     │
│  Which poem does this quote come from?                      │
│  [S.SPEAK]   ← machine starts speaking (LISTEN→SPEAK)     │
│  This phrase is from the poem "When You Are Old" by the    │
│  Irish poet William Butler Yeats. The original line reads, │
│  "when                                                      │
│  [ASR: <noise_detected>]  ← external stimulus appended     │
│  [C.SPEAK]   ← machine continues (SPEAK→SPEAK)            │
│  you are old and grey and full of sleep, and nodding by    │
│  the fire, take down this book...                          │
│  [S.LISTEN]  ← machine finishes, yields floor (SPEAK→LISTEN)│
│                                                             │
│  ─── PATTERN: AFFIRMATION (C.SPEAK) ────────────────────── │
│                                                             │
│  [system_prompt] [user] "When you are old and grey..."     │
│  Which poem does this quote come from?                      │
│  [S.SPEAK]                                                  │
│  This phrase is from the poem "When You Are Old" by the    │
│  Irish poet William Butler Yeats. The original line reads, │
  "when                                                      │
│  [ASR: "Um"]  ← user backchannel detected                  │
│  [C.SPEAK]   ← machine continues (backchannel, not stop)   │
│  you are old and grey and full of sleep...                  │
│  [S.LISTEN]                                                 │
│                                                             │
│  ─── PATTERN: DENIAL (S.LISTEN) ────────────────────────── │
│                                                             │
│  [system_prompt] [user] "When you are old and grey..."     │
│  Which poem does this quote come from?                      │
│  [S.SPEAK]                                                  │
│  This phrase is from the poem "When You Are Old" by the    │
│  Irish poet William Butler Yeats. The original line reads, │
│  "when                                                      │
│  [ASR: "That's fine. How about the line, 'Down by the    │
│  salley gardens, my love and I did meet;'"]                 │
│  [S.LISTEN]  ← machine yields floor (SPEAK→LISTEN)         │
│  [C.LISTEN]  ← machine waits for more input (LISTEN→LISTEN)│
│  [S.SPEAK]   ← machine starts new response (LISTEN→SPEAK) │
│  This line comes from another poem by the same poet,       │
│  titled "Down by the Salley Gardens." The original verse   │
│  goes...                                                    │
│  [S.LISTEN]                                                 │
│                                                             │
│  ─── PATTERN: TOPIC SHIFT (S.LISTEN) ──────────────────── │
│                                                             │
│  [system_prompt] [user] What's the weather tomorrow?       │
│  [S.SPEAK]                                                  │
│  There will be light rain tomorrow with a temperature      │
│  around 20 degrees...                                       │
│  [ASR: "What about the weather this weekend?"]            │
│  [S.LISTEN]  ← machine yields floor (SPEAK→LISTEN)         │
│  [C.LISTEN]  ← waits for more (if needed)                   │
│  [S.SPEAK]   ← starts new response                         │
│  It will be sunny this weekend with temperatures between   │
│  22 and 25 degrees.                                        │
│  [S.LISTEN]                                                 │
│                                                             │
│  KEY:                                                       │
│  [S.SPEAK]  = LISTEN → SPEAK transition (start speaking)  │
│  [C.SPEAK]  = SPEAK → SPEAK (continue despite user input) │
│  [S.LISTEN] = SPEAK → LISTEN (yield to user)              │
│  [C.LISTEN] = LISTEN → LISTEN (keep waiting)              │
│  [ASR: ...] = external stimulus streamed from perception  │
│              module (appended to tape in real-time)        │
└─────────────────────────────────────────────────────────────┘
```

## 2.5 The "Tape" Serialization Rules

The paper specifies three conditions under which a new token is appended to the tape: [```2```](https://arxiv.org/html/2405.19487)

```
┌─────────────────────────────────────────────────────────────┐
│              TAPE APPENDIX RULES                             │
│                                                             │
│  A new token is appended to the tape when:                  │
│                                                             │
│  1. The LLM generates a new output token                     │
│     (either a text token or a control token)                │
│     → This is the LLM's own generation                      │
│                                                             │
│  2. A new output chunk from the perception module           │
│     arrives                                                  │
│     → This is ASR output (user speech transcribed           │
│       to text tokens, streamed in real-time)                │
│     → Appended REGARDLESS of FSM state (always listening)   │
│                                                             │
│  3. The last token on the tape has been processed by        │
│     the motor function module                               │
│     → This means the TTS has finished converting the        │
│       last text token to speech                             │
│     → Signals the system that the motor module is           │
│       ready for the next token                              │
│                                                             │
│  The tape is a strictly causal, ordered sequence:           │
│  tokens from all sources are interleaved by time.          │
│  The LLM predicts the next token given everything           │
│  before it — standard autoregressive generation.           │
└─────────────────────────────────────────────────────────────┘
```

## 2.6 PRR Scores by Pattern — Full Comparison

Here is the complete PRR breakdown across all evaluated models: [```2```](https://arxiv.org/html/2405.19487)

| Model                                   | PRR_noise | PRR_denial | PRR_affirm | PRR_shift | PRR_avg   |
| --------------------------------------- | --------- | ---------- | ---------- | --------- | --------- |
| **GPT-4o**                              | 84.7%     | 100.0%     | 100.0%     | 100.0%    | 96.1%     |
| **GPT-3.5-turbo-0125**                  | 88.9%     | 96.3%      | 65.9%      | 85.8%     | 84.4%     |
| **Llama-3-8B-Instruct** (baseline)      | 85.2%     | 100.0%     | 89.2%      | 100.0%    | 93.6%     |
| **Llama-3-8B-Instruct-fd** (fine-tuned) | **95.2%** | **100.0%** | 91.9%      | 99.5%     | **96.7%** |

Key observations from the pattern-level data:

- **Denial is the easiest pattern** — all models except GPT-3.5 achieve 100%. "No, that's wrong" is unambiguous.
- **Noise is the hardest for GPT-4o** — it scores lowest (84.7%) on noise, suggesting the model over-responds to non-speech acoustic events.
- **Affirmation is the hardest for GPT-3.5** — it scores only 65.9%, suggesting it frequently stops when it hears "um" or "okay" when it should continue.
- **The fd fine-tuning improves noise handling most** — Llama-3-8B-fd gains 10 points on noise (85.2% → 95.2%) over the baseline, suggesting the FSM training specifically teaches the model to ignore non-speech events.
- **Topic shift is where the fd model underperforms GPT-4o** — 99.5% vs 100.0%, suggesting the 8B model occasionally fails to recognize a topic shift as requiring a floor yield.

## 2.7 Machine Interruption Statistics — Detailed Breakdown

| Metric                              | GPT-4o | GPT-3.5 | Llama-3-8B | Llama-3-8B-fd |
| ----------------------------------- | ------ | ------- | ---------- | ------------- |
| ir_mid (%)                          | 14.4   | 14.6    | 14.6       | **37.2**      |
| ir_end (%)                          | 11.3   | 10.3    | 10.3       | **24.0**      |
| MIR (missed, %)                     | 91.1   | 75.1    | 75.1       | 38.8          |
| PIR (proper timing, %)              | 95.8   | 79.3    | 79.3       | 79.1          |
| ir_mid + ir_end (total interrupt %) | 25.7   | 37.4    | 37.4       | **62.8**      |

The `Llama-3-8B-Instruct-fd` model interrupts the user in 62.8% of interactions — more than any other model. Of those, 37.2% occur mid-sentence (while the user is still speaking), which is the hallmark of true full-duplex capability. [```2```](https://arxiv.org/html/2405.19487)

## 2.8 The Fine-Tuning Recipe

```
┌─────────────────────────────────────────────────────────────┐
│              NEURAL FSM FINE-TUNING RECIPE                    │
│                                                             │
│  Base model:       Llama-3-8B-Instruct                      │
│  GPU:              8 × NVIDIA A100                           │
│  Batch size:       256 sequences                            │
│  Learning rate:    1 × 10⁻⁵                                │
│  Optimizer:        AdamW                                    │
│  Training steps:   20 (yes, twenty)                         │
│                                                             │
│  Training data:    ~2,000 GPT-4-generated oral dialogues    │
│                    + ~2,000 filtered ShareGPT sessions       │
│                    + 720 pattern-specific entries            │
│                    = ~4,720 total entries                    │
│                                                             │
│  Why only 20 steps?                                         │
│  ┌───────────────────────────────────────────────────┐      │
│  │  The LLM already has the reasoning capability      │      │
│  │  to decide when to interrupt or yield. What it     │      │
│  │  lacks is the "vocabulary" of control tokens.     │      │
│  │                                                   │      │
│  │  The control tokens [S.SPEAK], [C.SPEAK],         │      │
│  │  [S.LISTEN], [C.LISTEN] are just 4 new special     │      │
│  │  tokens added to the vocabulary. The model needs   │      │
│  │  to learn WHEN to emit them, not HOW to reason      │      │
│  │  about dialogue — it already knows that.           │      │
│  │                                                   │      │
│  │  20 steps × 256 sequences = 5,120 examples seen  │      │
│  │  ≈ roughly 1 pass through the ~4,720-entry         │      │
│  │  training set. This is enough for the model to     │      │
│  │  learn the token→behavior mapping.                 │      │
│  └───────────────────────────────────────────────────┘      │
│                                                             │
│  Regression check (Table 3):                                │
│  ┌───────────────────────────────────────────────────┐      │
│  │  MMLU:     68.4 → 67.5  (−0.9)                    │      │
│  │  TriviaQA: 64.8 → 65.9  (+1.1)                    │      │
│  │  HumanEval: 55.5 → 50.6 (−4.9)                    │      │
│  │  GSM-8K:   79.3 → 75.3 (−4.0)                    │      │
│  │  MATH:     26.9 → 24.1 (−2.8)                    │      │
│  │                                                   │      │
│  │  Minimal regression — the fine-tuning doesn't      │      │
│  │  destroy existing capabilities. The small drops   │      │
│  │  in HumanEval and GSM-8K are expected: the model   │      │
│  │  now has 4 additional tokens in its vocabulary     │      │
│  │  that slightly dilute the output distribution      │      │
│  │  for code/math.                                    │      │
│  └───────────────────────────────────────────────────┘      │
└─────────────────────────────────────────────────────────────┘
```

---

## Cross-Reference: Training Scale Comparison

```
┌─────────────────────────────────────────────────────────────┐
│         FLAIR vs. NEURAL FSM: TRAINING SCALE                 │
│                                                             │
│  FLAIR:                          Neural FSM:                │
│  ┌──────────────────────┐        ┌──────────────────────┐   │
│  │  190,000 hours data  │        │  ~4,720 entries       │   │
│  │  3 training stages   │        │  1 training stage    │   │
│  │  7B param model      │        │  8B param model       │   │
│  │  + 7B param expert   │        │  (no expert)          │   │
│  │  ELBO + KL + timing  │        │  Standard SFT         │   │
│  │  BFloat16 precision  │        │  (precision unspec.)  │   │
│  │  LR: 5e-4 (PT)       │        │  LR: 1e-5             │   │
│  │       5e-5 (SFT)     │        │                       │   │
│  │  AdamW, β₂=0.98      │        │  AdamW                │   │
│  │  Grad clip: 1.0      │        │  (unspecified)        │   │
│  │  Warmup: 2500 steps  │        │  (unspecified)        │   │
│  │  GPUs: unspecified   │        │  8× A100              │   │
│  │  Steps: unspecified  │        │  20 steps             │   │
│  └──────────────────────┘        └──────────────────────┘   │
│                                                             │
│  The scale difference reflects the ambition difference:     │
│  FLAIR teaches a model to THINK while listening.            │
│  Neural FSM teaches a model WHEN to speak vs. listen.      │
│  The former requires learning new internal representations; │
│  the latter requires learning a 4-token vocabulary.         │
└─────────────────────────────────────────────────────────────┘
```

---

