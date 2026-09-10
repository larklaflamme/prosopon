//! Prosopon — Skye's voice + avatar presence client.
//!
//! The Rust shell owns the state machine, the frameless window, the tray
//! icon, and the WebRTC transport (via `prosopon-client-core`). The webview
//! renders the orb and listens for `state` events.

mod state_machine;

use prosopon_client_core::wake_word::WakeWordDetector;
use prosopon_client_core::webrtc_client::WebRtcClient;
use state_machine::{ClientState, StateMachine, Transition};
use std::sync::{Arc, Mutex};
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, State,
};

pub struct AppState {
    machine: Mutex<StateMachine>,
    /// The connected WebRTC client, if any. `Arc` so commands can clone it
    /// without holding the mutex across an await point.
    webrtc: Mutex<Option<Arc<WebRtcClient>>>,
    /// The running wake-word detector, if any. Held so it stays alive (and
    /// so it can be stopped on disconnect).
    wake_word: Mutex<Option<WakeWordDetector>>,
}

fn emit_state(app: &AppHandle, state: ClientState) {
    let _ = app.emit("state", state);
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
fn disconnect(app: AppHandle, state: State<AppState>) -> ClientState {
    eprintln!("[prosopon] disconnect requested");
    // Drop the WebRTC client so the data channel closes and a later
    // connect_webrtc starts from a clean slate.
    *state.webrtc.lock().unwrap() = None;
    // Stop the wake-word detector (kills the sidecar + mic capture).
    if let Some(mut detector) = state.wake_word.lock().unwrap().take() {
        detector.stop();
    }
    let mut machine = state.machine.lock().unwrap();
    if let Some(new_state) = machine.apply(Transition::Disconnect) {
        emit_state(&app, new_state);
    }
    machine.current()
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
fn start_wake_word(app: AppHandle, state: State<AppState>) -> Result<(), String> {
    // No-op if already running.
    if state.wake_word.lock().unwrap().is_some() {
        eprintln!("[prosopon] start_wake_word: already running");
        return Ok(());
    }

    let config = load_client_config();
    eprintln!(
        "[prosopon] start_wake_word: model = {}, threshold = {}, sidecar = {}",
        config.wake_word.model, config.wake_word.threshold, config.wake_word.sidecar_path
    );

    let (detector, wake_rx) = WakeWordDetector::start(&config.wake_word).map_err(|e| {
        eprintln!("[prosopon] start_wake_word FAILED: {e}");
        e.to_string()
    })?;

    *state.wake_word.lock().unwrap() = Some(detector);

    // Background thread: on each wake event, flip Idle -> Listening.
    let app_handle = app.clone();
    std::thread::spawn(move || {
        while wake_rx.recv().is_ok() {
            let state = app_handle.state::<AppState>();
            let mut machine = state.machine.lock().unwrap();
            if let Some(new_state) = machine.apply(Transition::WakeWord) {
                emit_state(&app_handle, new_state);
            }
        }
    });

    eprintln!("[prosopon] start_wake_word: listening");
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState {
            machine: Mutex::new(StateMachine::new()),
            webrtc: Mutex::new(None),
            wake_word: Mutex::new(None),
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
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            set_muted,
            connect_webrtc,
            send_text,
            disconnect,
            record_mic,
            start_wake_word
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
