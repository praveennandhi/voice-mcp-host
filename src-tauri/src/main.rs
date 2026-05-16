#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app_state;
mod agent_memory;
mod agent_prompts;
mod agent_provider;
mod agent_runtime;
mod agent_tools;
mod agent_types;
mod asr;
mod audio;
mod commands;
mod config;
mod hotkeys;
mod insertion;
mod logging;
mod model_store;
mod platform;
mod recorder;
mod tray;
mod workspace;

use app_state::AppState;
use tauri::{Manager, WindowEvent};

fn main() {
    let cfg = config::load_or_default();
    let app_state = AppState::new(cfg);

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::get_overlay_state,
            commands::get_agent_session,
            commands::clear_agent_session,
            commands::send_agent_chat,
            commands::start_overlay_drag,
            commands::save_config,
            commands::get_status,
            commands::download_model,
            commands::download_engine,
            commands::list_audio_devices,
            commands::list_models,
            commands::check_permissions,
            commands::request_accessibility_permission,
            commands::open_log_dir,
            commands::get_version,
            commands::quit_app,
        ])
        .setup(|app| {
            logging::write_event("app_started", Some(serde_json::json!({
                "version": env!("CARGO_PKG_VERSION"),
                "platform": std::env::consts::OS,
            })));

            // Position overlay at bottom-center of primary monitor
            if let Some(overlay) = app.get_webview_window("overlay") {
                if let Ok(Some(monitor)) = overlay.primary_monitor() {
                    let screen = monitor.size();
                    let win_w = 390u32;
                    let win_h = 118u32;
                    let x = (screen.width.saturating_sub(win_w)) / 2;
                    let y = screen.height.saturating_sub(win_h).saturating_sub(120);
                    let _ = overlay.set_position(tauri::PhysicalPosition { x, y });
                }
            }

            // Closing settings minimizes it; the background hotkey host keeps running.
            // This avoids an unrecoverable hidden window if the OS tray menu is unavailable.
            if let Some(settings) = app.get_webview_window("settings") {
                let settings_for_close = settings.clone();
                settings.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = settings_for_close.minimize();
                    }
                });
            }

            tray::setup(app)?;

            // Try to load the model if it's already on disk.
            commands::load_transcriber(app.handle());

            // Register the global hotkey.
            hotkeys::register(app.handle())?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running voice-mcp-host");
}
