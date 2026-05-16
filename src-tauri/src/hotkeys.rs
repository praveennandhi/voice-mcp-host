use std::str::FromStr;
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use crate::app_state::{AppState, RecorderState};
use crate::logging;
use crate::recorder;

pub fn register(app: &AppHandle) -> tauri::Result<()> {
    let cfg = app.state::<AppState>().config.lock().unwrap().clone();
    let hotkey = cfg.dictation.primary_hotkey.clone();
    let shortcut = parse_shortcut(&hotkey);

    // Unregister first in case a previous instance left the key claimed.
    let _ = app.global_shortcut().unregister(shortcut);

    app.global_shortcut()
        .on_shortcut(shortcut, move |app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                handle_hotkey_press(app.clone());
            }
        })
        .map_err(|e| tauri::Error::Anyhow(anyhow::anyhow!("hotkey registration failed: {e}")))?;

    logging::write_event("hotkey_registered", Some(serde_json::json!({ "key": hotkey })));
    Ok(())
}

fn handle_hotkey_press(app: AppHandle) {
    logging::write_event("hotkey_pressed", None);
    let state = app.state::<AppState>();
    let current = state.recorder_state.lock().unwrap().clone();

    match current {
        RecorderState::Idle | RecorderState::Ready | RecorderState::Error(_) => {
            recorder::start_recording(app);
        }
        RecorderState::Recording => {
            recorder::stop_and_transcribe(app);
        }
        RecorderState::Transcribing | RecorderState::Pasting => {
            // Ignore hotkey while busy
        }
    }
}

fn parse_shortcut(hotkey: &str) -> Shortcut {
    Shortcut::from_str(&hotkey.trim().to_uppercase())
        .unwrap_or_else(|_| {
            if cfg!(target_os = "macos") {
                Shortcut::from_str("F5").unwrap()
            } else {
                Shortcut::from_str("F2").unwrap()
            }
        })
}
