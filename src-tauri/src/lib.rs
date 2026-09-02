//! Prosopon — Skye's voice + avatar presence client.
//!
//! The Rust shell owns the state machine, the frameless window, the tray
//! icon, and the WebRTC transport (via `prosopon-client-core`). The webview
//! renders the orb and listens for `state` events.
//!
//! NOTE (2026-09-02): the `connect_webrtc` / `send_text` commands below are
//! written but **not yet compiled** — the Tauri shell cannot build on the
//! headless Linux box (no `webkit2gtk`). They are the integration point
//! between the verified `client-core` transport and the GUI, to be compiled
//! and tested on the Mac.

mod state_machine;

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
}

fn emit_state(app: &AppHandle, state: ClientState) {
    let _ = app.emit("state", state);
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
/// the state machine to Idle. Replaces the old state-machine-only `connect`.
#[tauri::command]
async fn connect_webrtc(
    app: AppHandle,
    state: State<'_, AppState>,
    signaling_url: String,
    auth_token: String,
) -> Result<(), String> {
    let config = prosopon_client_core::config::ClientConfig {
        signaling: prosopon_client_core::config::SignalingConfig {
            url: signaling_url,
            auth_token,
        },
        ..Default::default()
    };
    let client = WebRtcClient::connect(&config)
        .await
        .map_err(|e| e.to_string())?;
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
    let mut machine = state.machine.lock().unwrap();
    if let Some(new_state) = machine.apply(Transition::Disconnect) {
        emit_state(&app, new_state);
    }
    machine.current()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState {
            machine: Mutex::new(StateMachine::new()),
            webrtc: Mutex::new(None),
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
            disconnect
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
