//! Prosopon — Skye's voice + avatar presence client.
//!
//! The Rust shell owns the state machine, the frameless window, and the
//! tray icon. The webview renders the orb and listens for `state` events.

mod state_machine;

use state_machine::{ClientState, StateMachine, Transition};
use std::sync::Mutex;
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, State,
};

pub struct AppState {
    machine: Mutex<StateMachine>,
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

#[tauri::command]
fn connect(app: AppHandle, state: State<AppState>) -> ClientState {
    let mut machine = state.machine.lock().unwrap();
    if let Some(new_state) = machine.apply(Transition::Connect) {
        emit_state(&app, new_state);
    }
    machine.current()
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
            connect,
            disconnect
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
