//! Prosopon — Skye's voice + avatar presence client.
//!
//! The Rust shell owns the state machine, the frameless window, the tray
//! icon, and the WebRTC transport (via `prosopon-client-core`). The webview
//! renders the orb and listens for `state` events.

mod state_machine;

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

    let app_for_log = (*app).clone();
    let (detector, wake_rx) = WakeWordDetector::start(&config.wake_word, move |line| {
        emit_log(&app_for_log, "info", "sidecar", line);
    })
    .map_err(|e| {
        emit_log(app, "error", "wake_word", format!("FAILED: {e}"));
        e.to_string()
    })?;

    {
        let state = app.state::<AppState>();
        *state.wake_word.lock().unwrap() = Some(detector);
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

            // Auto-start the wake word so the client is armed from launch
            // (no manual `start_wake_word` invoke needed).
            let config = load_client_config();
            if config.wake_word.auto_start {
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
            get_logs
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
