//! Prosopon — Skye's voice + avatar presence client.
//!
//! The Rust shell owns the state machine, the frameless window, the tray
//! icon, and the WebRTC transport (via `prosopon-client-core`). The webview
//! renders the orb and listens for `state` events.

mod state_machine;

use prosopon_client_core::aec::{AecProcessor, ReferenceBuffer};
use prosopon_client_core::mic_bus::MicBus;
use prosopon_client_core::stt::{SttDetector, SttEvent};
use prosopon_client_core::wake_word::WakeWordDetector;
use prosopon_client_core::webrtc_client::WebRtcClient;
use state_machine::{ClientState, StateMachine, Transition};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, State,
};

/// Maximum number of log lines retained in the in-memory ring buffer.
const MAX_LOGS: usize = 500;

/// A single structured log entry, surfaced to the webview's logs panel.
#[derive(Clone, serde::Serialize)]
struct LogEvent {
    level: String,
    source: String,
    message: String,
}

pub struct AppState {
    machine: Mutex<StateMachine>,
    /// The connected WebRTC client, if any. `Arc` so commands can clone it
    /// without holding the mutex across an await point.
    webrtc: Mutex<Option<Arc<WebRtcClient>>>,
    /// The running wake-word detector, if any. Held so it stays alive (and
    /// so it can be stopped on disconnect).
    wake_word: Mutex<Option<WakeWordDetector>>,
    /// The shared mic bus, if any. Held so it stays alive (and so it can be
    /// stopped on disconnect). The standalone wake-word path owns one; the
    /// conversation loop owns its own local bus.
    mic_bus: Mutex<Option<MicBus>>,
    /// Ring buffer of recent log lines, so the webview can backfill on load.
    logs: Mutex<VecDeque<LogEvent>>,
}

fn emit_state(app: &AppHandle, state: ClientState) {
    let _ = app.emit("state", state);
}

/// Emit a structured log line: to the terminal (stderr) and to the webview
/// (a `log` event), and into the in-memory ring buffer for backfill.
fn emit_log(app: &AppHandle, level: &str, source: &str, message: impl Into<String>) {
    let message = message.into();
    eprintln!("[prosopon] [{source}] {message}");
    let event = LogEvent {
        level: level.to_string(),
        source: source.to_string(),
        message,
    };
    if let Some(state) = app.try_state::<AppState>() {
        let mut logs = state.logs.lock().unwrap();
        logs.push_back(event.clone());
        while logs.len() > MAX_LOGS {
            logs.pop_front();
        }
    }
    let _ = app.emit("log", event);
}

/// Load the client config from `client/config.yaml`, trying a few candidate
/// paths (dev vs bundled). Falls back to defaults if none are found.
fn load_client_config() -> prosopon_client_core::config::ClientConfig {
    let candidates = [
        "client/config.yaml",
        "config.yaml",
        "../client/config.yaml",
    ];
    for p in candidates {
        match prosopon_client_core::config::ClientConfig::load(p) {
            Ok(cfg) => {
                eprintln!("[prosopon] loaded config from {p}");
                return cfg;
            }
            Err(e) => {
                eprintln!("[prosopon] config {p}: {e}");
            }
        }
    }
    eprintln!("[prosopon] no config.yaml found, using defaults");
    prosopon_client_core::config::ClientConfig::default()
}

#[tauri::command]
fn get_state(state: State<AppState>) -> ClientState {
    state.machine.lock().unwrap().current()
}

#[tauri::command]
fn set_muted(app: AppHandle, state: State<AppState>, muted: bool) -> ClientState {
    let mut machine = state.machine.lock().unwrap();
    if let Some(new_state) = machine.apply(Transition::SetMute(muted)) {
        emit_state(&app, new_state);
    }
    machine.current()
}

/// Connect the WebRTC transport to the server's signaling endpoint, then flip
/// the state machine to Idle. Loads config from `client/config.yaml`.
#[tauri::command]
async fn connect_webrtc(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let config = load_client_config();
    eprintln!("[prosopon] connect_webrtc: url = {}", config.signaling.url);
    eprintln!(
        "[prosopon] connect_webrtc: auth_token {}",
        if config.signaling.auth_token.is_empty() {
            "<empty>".to_string()
        } else {
            format!("<set, {} chars>", config.signaling.auth_token.len())
        }
    );
    eprintln!(
        "[prosopon] connect_webrtc: stun = {:?}",
        config.webrtc.stun_servers
    );

    let client = WebRtcClient::connect(&config).await.map_err(|e| {
        eprintln!("[prosopon] connect_webrtc FAILED: {e}");
        e.to_string()
    })?;

    eprintln!("[prosopon] connect_webrtc: data channel open");
    *state.webrtc.lock().unwrap() = Some(Arc::new(client));
    let mut machine = state.machine.lock().unwrap();
    if let Some(new_state) = machine.apply(Transition::Connect) {
        emit_state(&app, new_state);
    }
    Ok(())
}

/// Send the user's utterance over the data channel.
#[tauri::command]
async fn send_text(state: State<'_, AppState>, text: String) -> Result<(), String> {
    let client = {
        let guard = state.webrtc.lock().unwrap();
        guard.as_ref().cloned()
    };
    let client = client.ok_or("not connected")?;
    client.send_text(&text).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn disconnect(app: AppHandle) -> ClientState {
    eprintln!("[prosopon] disconnect requested");
    graceful_shutdown(&app).await;
    app.state::<AppState>().machine.lock().unwrap().current()
}

/// Gracefully tear down the WebRTC connection and the wake-word detector, then
/// flip the state machine to `Disconnected`. Shared by the `disconnect` command
/// and the Ctrl-C / tray-quit shutdown paths.
///
/// The key detail: we call `WebRtcClient::close()` (which closes the peer
/// connection properly) rather than just dropping the client. Dropping the
/// `Arc<dyn PeerConnection>` tears the connection down abruptly, so the server
/// never observes a clean close and keeps a stale session alive until its ICE
/// keepalive times out. A graceful close lets the server's data channel see
/// `OnClose` and tear down its session immediately.
async fn graceful_shutdown(app: &AppHandle) {
    // Close the WebRTC peer connection gracefully.
    let client = {
        let state = app.state::<AppState>();
        let c = state.webrtc.lock().unwrap().take();
        c
    };
    if let Some(client) = client {
        eprintln!("[prosopon] shutdown: closing webrtc connection");
        match tokio::time::timeout(std::time::Duration::from_secs(2), client.close()).await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => eprintln!("[prosopon] shutdown: webrtc close error: {e}"),
            Err(_) => eprintln!("[prosopon] shutdown: webrtc close timed out"),
        }
    }

    // Stop the wake-word detector (kills the sidecar + mic capture).
    let detector = {
        let state = app.state::<AppState>();
        let d = state.wake_word.lock().unwrap().take();
        d
    };
    if let Some(mut detector) = detector {
        detector.stop();
    }

    // Stop the shared mic bus (stops capture + disconnects subscribers).
    let bus = {
        let state = app.state::<AppState>();
        let b = state.mic_bus.lock().unwrap().take();
        b
    };
    if let Some(mut bus) = bus {
        bus.stop();
    }

    // Flip the state machine to Disconnected.
    let state = app.state::<AppState>();
    let mut machine = state.machine.lock().unwrap();
    if let Some(new_state) = machine.apply(Transition::Disconnect) {
        emit_state(app, new_state);
    }
}

/// Capture `seconds` of mic audio and report the RMS level + sample count.
/// Proves the mic opens and delivers samples — the first milestone before
/// wiring the wake word and STT.
#[tauri::command]
fn record_mic(seconds: f32) -> Result<String, String> {
    let mic = prosopon_client_core::mic::Mic::start().map_err(|e| e.to_string())?;
    let target = (prosopon_client_core::mic::SAMPLE_RATE as f32 * seconds) as usize;
    let mut samples: Vec<f32> = Vec::with_capacity(target);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs_f32(seconds);

    while std::time::Instant::now() < deadline {
        match mic.next_chunk() {
            Ok(chunk) => samples.extend_from_slice(&chunk),
            Err(_) => break,
        }
    }

    let n = samples.len();
    let rms = if n == 0 {
        0.0
    } else {
        (samples.iter().map(|s| s * s).sum::<f32>() / n as f32).sqrt()
    };
    let dur = n as f32 / prosopon_client_core::mic::SAMPLE_RATE as f32;

    Ok(format!(
        "captured {n} samples ({dur:.2}s) at 16 kHz mono, RMS = {rms:.4}"
    ))
}

/// Start the always-on wake-word listener. Spawns the openWakeWord sidecar,
/// streams the mic to it, and flips the state machine to `Listening` on each
/// detection. Idempotent: a second call while already running is a no-op.
#[tauri::command]
fn start_wake_word(app: AppHandle) -> Result<(), String> {
    start_wake_word_inner(&app)
}

/// The shared wake-word startup path, used by both the `start_wake_word`
/// command and the auto-start hook in `setup`.
fn start_wake_word_inner(app: &AppHandle) -> Result<(), String> {
    // No-op if already running.
    {
        let state = app.state::<AppState>();
        if state.wake_word.lock().unwrap().is_some() {
            emit_log(app, "info", "wake_word", "already running");
            return Ok(());
        }
    }

    let config = load_client_config();
    emit_log(
        app,
        "info",
        "wake_word",
        format!(
            "model = {}, threshold = {}, sidecar = {}",
            config.wake_word.model, config.wake_word.threshold, config.wake_word.sidecar_path
        ),
    );

    // Open the shared mic bus and subscribe the wake-word sidecar to it.
    let bus = MicBus::start().map_err(|e| {
        emit_log(app, "error", "wake_word", format!("mic bus failed: {e}"));
        e.to_string()
    })?;
    let sub = bus.subscribe();

    let app_for_log = (*app).clone();
    let (detector, wake_rx) = WakeWordDetector::start(&config.wake_word, sub, move |line| {
        emit_log(&app_for_log, "info", "sidecar", line);
    })
    .map_err(|e| {
        emit_log(app, "error", "wake_word", format!("FAILED: {e}"));
        e.to_string()
    })?;

    {
        let state = app.state::<AppState>();
        *state.wake_word.lock().unwrap() = Some(detector);
        *state.mic_bus.lock().unwrap() = Some(bus);
    }

    // Background thread: on each wake event, flip Idle -> Listening.
    let app_handle = (*app).clone();
    std::thread::spawn(move || {
        while wake_rx.recv().is_ok() {
            let state = app_handle.state::<AppState>();
            let mut machine = state.machine.lock().unwrap();
            if let Some(new_state) = machine.apply(Transition::WakeWord) {
                emit_state(&app_handle, new_state);
            }
        }
    });

    emit_log(app, "info", "wake_word", "listening");
    Ok(())
}

/// Start the full voice loop: wake word → STT → send → receive → play →
/// repeat. This is the bridge that ties the two sides together.
///
/// The loop owns the wake-word detector lifecycle (started fresh each turn)
/// and keeps the STT detector warm for the whole session (gated, so it only
/// hears real audio while the machine is in `Listening`). It runs on a
/// background thread and loops until the process exits.
#[tauri::command]
fn start_conversation(app: AppHandle) -> Result<(), String> {
    // Stop any standalone wake-word detector first — the loop owns it now.
    let detector = {
        let state = app.state::<AppState>();
        let d = state.wake_word.lock().unwrap().take();
        d
    };
    if let Some(mut d) = detector {
        d.stop();
    }
    // Also stop the standalone mic bus (the loop owns its own bus now).
    let bus = {
        let state = app.state::<AppState>();
        let b = state.mic_bus.lock().unwrap().take();
        b
    };
    if let Some(mut b) = bus {
        b.stop();
    }
    run_conversation_loop(app);
    Ok(())
}

/// The conversation loop body. Spawned on a dedicated thread by
/// `start_conversation` (or auto-started from `setup`).
fn run_conversation_loop(app: AppHandle) {
    std::thread::spawn(move || {
        let config = load_client_config();
        let silence_timeout = std::time::Duration::from_secs(config.conversation.silence_timeout_secs);
        let inactivity_timeout = std::time::Duration::from_secs(config.conversation.inactivity_timeout_secs);

        // Phase 0: one shared mic bus for the whole session. The wake-word
        // and STT sidecars subscribe to it instead of opening their own mic.
        // Acoustic echo cancellation (AEC) runs in the capture thread: the
        // reference buffer is fed by playback, and AEC3 cancels the echo of
        // the assistant's own voice from the mic.
        let reference = Arc::new(ReferenceBuffer::new());
        let aec = match AecProcessor::new(Arc::clone(&reference)) {
            Ok(a) => Arc::new(a),
            Err(e) => {
                emit_log(&app, "error", "conversation", format!("AEC init failed: {e}"));
                return;
            }
        };
        let bus = match MicBus::start_with_aec(aec) {
            Ok(b) => b,
            Err(e) => {
                emit_log(&app, "error", "conversation", format!("mic bus failed: {e}"));
                return;
            }
        };
        emit_log(&app, "info", "conversation", "mic bus open (AEC)");

        // Start the STT detector ONCE, warm, with the feed gate closed. It
        // stays alive for the whole session; the gate controls whether it
        // hears real audio (open while Listening) or silence (otherwise).
        // This eliminates the cold-start race where the model-load delay ate
        // the user's first utterance.
        let stt_sub = bus.subscribe();
        let app_for_log = app.clone();
        let (stt, stt_rx) = match SttDetector::start(&config.stt, config.conversation.pre_roll_secs, stt_sub, move |line| {
            emit_log(&app_for_log, "info", "stt", line);
        }) {
            Ok(x) => x,
            Err(e) => {
                emit_log(&app, "error", "conversation", format!("STT failed: {e}"));
                return;
            }
        };
        emit_log(&app, "info", "conversation", "STT warm (gated)");

        loop {
            // --- COLD: wake word ---
            let sub = bus.subscribe();
            let app_for_log = app.clone();
            let (detector, wake_rx) = match WakeWordDetector::start(&config.wake_word, sub, move |line| {
                emit_log(&app_for_log, "info", "wake_word", line);
            }) {
                Ok(x) => x,
                Err(e) => {
                    emit_log(&app, "error", "conversation", format!("wake word failed: {e}"));
                    return;
                }
            };
            emit_log(&app, "info", "conversation", "listening for wake word");

            // Block until a wake event.
            if wake_rx.recv().is_err() {
                return; // detector stopped
            }
            emit_log(&app, "info", "conversation", "wake word detected");
            {
                let state = app.state::<AppState>();
                let mut machine = state.machine.lock().unwrap();
                if let Some(s) = machine.apply(Transition::WakeWord) {
                    emit_state(&app, s);
                }
            }

            // Stop the wake word (drops its bus subscription). The bus keeps
            // capturing; STT is already subscribed and stays warm.
            let mut detector = detector;
            detector.stop();

            // --- WARM: multi-turn conversation ---
            // After the wake word, stay warm: listen for utterances without
            // requiring the wake word again, until the inactivity timeout
            // elapses. The first turn uses the longer silence timeout (grace
            // after the wake word); subsequent turns use the inactivity
            // timeout.
            let mut first_turn = true;
            loop {
                let is_first = first_turn;
                let timeout = if first_turn { silence_timeout } else { inactivity_timeout };
                first_turn = false;

                // Open the STT feed gate. STT is already warm; opening the
                // gate lets real audio flow immediately. On the first (cold)
                // turn, drain the pre-roll buffer first so the first word of
                // the query isn't clipped by wake-word detection latency.
                if is_first {
                    stt.set_listening_with_pre_roll();
                } else {
                    stt.set_listening(true);
                }
                emit_log(&app, "info", "conversation", "listening for utterance");

                // Wait for a completed utterance, resetting the inactivity
                // deadline whenever speech is in progress (a PARTIAL line).
                // This keeps the warm session alive while the user is still
                // speaking, instead of timing out mid-utterance.
                let mut deadline = std::time::Instant::now() + timeout;
                let text = loop {
                    let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                    match stt_rx.recv_timeout(remaining) {
                        Ok(SttEvent::Final(t)) => break Ok(t),
                        Ok(SttEvent::Partial) => {
                            // Speech in progress — reset the deadline.
                            deadline = std::time::Instant::now() + timeout;
                        }
                        Err(_) => {
                            // Deadline reached (or sidecar died) — end the warm
                            // session.
                            emit_log(&app, "info", "conversation", "utterance timeout (silence)");
                            stt.set_listening(false);
                            // Drain any stale events from the aborted utterance
                            // so they don't leak into the next turn.
                            while stt_rx.try_recv().is_ok() {}
                            // End the warm session: Listening -> Idle, cold again.
                            {
                                let state = app.state::<AppState>();
                                let mut machine = state.machine.lock().unwrap();
                                if let Some(s) = machine.apply(Transition::InactivityTimeout) {
                                    emit_state(&app, s);
                                }
                            }
                            break Err(());
                        }
                    }
                };
                let text = match text {
                    Ok(t) => strip_wake_word(&t, &config.wake_word.model),
                    Err(()) => break,
                };
                stt.set_listening(false);
                emit_log(&app, "info", "conversation", format!("utterance: {text}"));

                // --- Send + receive ---
                {
                    let state = app.state::<AppState>();
                    let mut machine = state.machine.lock().unwrap();
                    if let Some(s) = machine.apply(Transition::UtteranceComplete) {
                        emit_state(&app, s);
                    }
                }

                let client = {
                    let state = app.state::<AppState>();
                    let c = state.webrtc.lock().unwrap().as_ref().cloned();
                    c
                };
                let Some(client) = client else {
                    emit_log(&app, "error", "conversation", "not connected");
                    cancel_to_idle(&app);
                    break;
                };

                // send_text + recv_audio + recv_blendshapes are async; run them on the Tauri
                // runtime and hand the result back over a channel.
                let (tx, rx) = std::sync::mpsc::channel();
                let client_for_task = client.clone();
                let text_for_task = text.clone();
                tauri::async_runtime::spawn(async move {
                    let result = async {
                        client_for_task.send_text(&text_for_task).await?;
                        let audio = client_for_task.recv_audio().await?;
                        // Blendshapes are optional: `None` when the avatar is
                        // disabled (no blendshapes channel) or the track fails.
                        let blendshapes = client_for_task.recv_blendshapes().await;
                        Ok::<_, prosopon_client_core::ClientError>((audio, blendshapes))
                    }
                    .await;
                    let _ = tx.send(result);
                });

                let (audio, blendshapes_result) = match rx.recv() {
                    Ok(Ok((a, b))) => (a, b),
                    Ok(Err(e)) => {
                        emit_log(&app, "error", "conversation", format!("receive failed: {e}"));
                        cancel_to_idle(&app);
                        break;
                    }
                    Err(_) => {
                        emit_log(&app, "error", "conversation", "async task dropped");
                        cancel_to_idle(&app);
                        break;
                    }
                };

                let blendshapes = match blendshapes_result {
                    Ok(track) => {
                        emit_log(&app, "info", "conversation", format!("received {} bytes of blendshapes", track.len()));
                        Some(track)
                    }
                    Err(e) => {
                        emit_log(&app, "error", "conversation", format!("blendshapes receive failed: {e}"));
                        None
                    }
                };

                // Forward the blendshape track to the webview (three.js VRM
                // face). The track is NDJSON: one frame per line.
                if let Some(track) = &blendshapes {
                    if let Ok(text) = String::from_utf8(track.clone()) {
                        let _ = app.emit("blendshapes", text);
                    }
                }

                {
                    let bytes = audio;
                        emit_log(
                            &app,
                            "info",
                            "conversation",
                            format!("received {} bytes of audio", bytes.len()),
                        );
                        {
                            let state = app.state::<AppState>();
                            let mut machine = state.machine.lock().unwrap();
                            if let Some(s) = machine.apply(Transition::ResponseStarted) {
                                emit_state(&app, s);
                            }
                        }
                        // --- Play (interruptible for barge-in) ---
                        let playback = match prosopon_client_core::playback::Playback::start_with_reference(&bytes, Arc::clone(&reference)) {
                            Ok(p) => p,
                            Err(e) => {
                                emit_log(&app, "error", "playback", format!("playback failed: {e}"));
                                {
                                    let state = app.state::<AppState>();
                                    let mut machine = state.machine.lock().unwrap();
                                    if let Some(s) = machine.apply(Transition::ResponseComplete) {
                                        emit_state(&app, s);
                                    }
                                }
                                continue;
                            }
                        };

                        // Listen for a barge-in wake word during playback.
                        let barge_sub = bus.subscribe();
                        let app_for_log = app.clone();
                        let (mut ww, ww_rx) = match WakeWordDetector::start(&config.wake_word, barge_sub, move |line| {
                            emit_log(&app_for_log, "info", "wake_word", line);
                        }) {
                            Ok(x) => x,
                            Err(e) => {
                                emit_log(&app, "error", "wake_word", format!("barge-in wake word failed: {e}"));
                                playback.wait();
                                {
                                    let state = app.state::<AppState>();
                                    let mut machine = state.machine.lock().unwrap();
                                    if let Some(s) = machine.apply(Transition::ResponseComplete) {
                                        emit_state(&app, s);
                                    }
                                }
                                continue;
                            }
                        };

                        // Poll: wake word (barge-in) vs playback completion.
                        let mut barged = false;
                        loop {
                            if playback.is_finished() {
                                break;
                            }
                            match ww_rx.recv_timeout(std::time::Duration::from_millis(100)) {
                                Ok(()) => {
                                    emit_log(&app, "info", "conversation", "barge-in detected");
                                    playback.stop();
                                    reference.clear();
                                    barged = true;
                                    break;
                                }
                                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                            }
                        }
                        ww.stop();

                        if barged {
                            {
                                let state = app.state::<AppState>();
                                let mut machine = state.machine.lock().unwrap();
                                if let Some(s) = machine.apply(Transition::BargeIn) {
                                    emit_state(&app, s);
                                }
                            }
                            // BargeIn -> Listening (warm). Loop back to listen.
                        } else {
                            {
                                let state = app.state::<AppState>();
                                let mut machine = state.machine.lock().unwrap();
                                if let Some(s) = machine.apply(Transition::ResponseComplete) {
                                    emit_state(&app, s);
                                }
                            }
                            // ResponseComplete -> Listening (warm). Loop back.
                        }
                    }
            }
        }
    });
}

/// Strip a leading wake-word phrase (e.g. "hey jarvis") from a transcript,
/// case-insensitively. The pre-roll buffer feeds the wake word into STT, so
/// the first utterance may begin with it; the server tolerates it, but
/// stripping it here keeps the transcript clean.
fn strip_wake_word(text: &str, wake_word_model: &str) -> String {
    let phrase = wake_word_model.replace('_', " ");
    let lower = text.to_lowercase();
    let lower_phrase = phrase.to_lowercase();
    if let Some(idx) = lower.find(&lower_phrase) {
        let prefix = &text[..idx];
        if prefix.trim().is_empty() {
            let rest = &text[idx + lower_phrase.len()..];
            return rest.trim_start().to_string();
        }
    }
    text.to_string()
}

/// Return the state machine to Idle after an aborted listen/think.
fn cancel_to_idle(app: &AppHandle) {
    let state = app.state::<AppState>();
    let mut machine = state.machine.lock().unwrap();
    if let Some(s) = machine.apply(Transition::Cancel) {
        emit_state(app, s);
    }
}

/// Return the buffered log lines, so the webview can backfill on load.
#[tauri::command]
fn get_logs(state: State<AppState>) -> Vec<LogEvent> {
    state.logs.lock().unwrap().iter().cloned().collect()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState {
            machine: Mutex::new(StateMachine::new()),
            webrtc: Mutex::new(None),
            wake_word: Mutex::new(None),
            mic_bus: Mutex::new(None),
            logs: Mutex::new(VecDeque::new()),
        })
        .setup(|app| {
            // Tray icon — minimize-to-tray. Requires an icon asset at
            // src-tauri/icons/ (generate with `tauri icon` once the CLI
            // is installed).
            let show = MenuItem::with_id(app, "show", "Show window", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => {
                        let handle = app.clone();
                        tauri::async_runtime::spawn(async move {
                            graceful_shutdown(&handle).await;
                            handle.exit(0);
                        });
                    }
                    _ => {}
                })
                .build(app)?;

            // Auto-start: prefer the full conversation loop; fall back to the
            // standalone wake word if conversation mode is off.
            let config = load_client_config();
            if config.conversation.auto_start {
                emit_log(app.handle(), "info", "conversation", "auto-starting conversation loop");
                run_conversation_loop(app.handle().clone());
            } else if config.wake_word.auto_start {
                if let Err(e) = start_wake_word_inner(app.handle()) {
                    emit_log(
                        app.handle(),
                        "error",
                        "wake_word",
                        format!("auto-start failed: {e}"),
                    );
                }
            }

            // Install a Ctrl-C (SIGINT) handler so terminating the client from
            // the command line tears the WebRTC connection down gracefully
            // instead of dropping it abruptly. If the client is connected, we
            // close the peer connection (so the server sees a clean close and
            // doesn't need a restart) before exiting.
            let app_handle = app.handle().clone();
            ctrlc::set_handler(move || {
                let handle = app_handle.clone();
                tauri::async_runtime::spawn(async move {
                    graceful_shutdown(&handle).await;
                    handle.exit(0);
                });
            })
            .expect("failed to install Ctrl-C handler");

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            set_muted,
            connect_webrtc,
            send_text,
            disconnect,
            record_mic,
            start_wake_word,
            start_conversation,
            get_logs
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
