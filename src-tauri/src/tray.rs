use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle, Manager};

const SHOW_SETTINGS_ID: &str = "show_settings";
const OPEN_LOGS_ID: &str = "open_logs";
const QUIT_ID: &str = "quit";

pub fn setup(app: &mut App) -> tauri::Result<()> {
    let show_settings = MenuItem::with_id(app, SHOW_SETTINGS_ID, "Show Settings", true, None::<&str>)?;
    let open_logs = MenuItem::with_id(app, OPEN_LOGS_ID, "Open Logs", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, QUIT_ID, "Quit voice-mcp-host", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&show_settings, &open_logs, &separator, &quit])?;

    let mut builder = TrayIconBuilder::with_id("voice-mcp-host")
        .tooltip("voice-mcp-host")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            SHOW_SETTINGS_ID => show_settings_window(app),
            OPEN_LOGS_ID => open_logs_dir(app),
            QUIT_ID => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| match event {
            TrayIconEvent::Click {
                button: MouseButton::Left,
                ..
            }
            | TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } => show_settings_window(tray.app_handle()),
            _ => {}
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }

    builder.build(app)?;
    Ok(())
}

pub fn show_settings_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn open_logs_dir(app: &AppHandle) {
    if let Err(error) = crate::commands::open_log_dir(app.clone()) {
        crate::logging::write_event(
            "open_log_dir_failed",
            Some(serde_json::json!({ "error": error })),
        );
    }
}
