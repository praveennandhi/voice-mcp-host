use serde::Serialize;
use std::{path::PathBuf, thread, time::Duration};
use crate::platform::{ClipboardOps, platform};

#[derive(Debug, Clone, Serialize)]
pub struct PasteReport {
    pub paste_status: String,
    pub clipboard_restore_status: String,
    pub error_message: String,
    pub recovery_action: String,
}

pub fn paste_text(text: &str, paste_delay_ms: u64, restore_delay_ms: u64, fallback_path: Option<&PathBuf>) -> PasteReport {
    let p = platform();

    // Save original clipboard contents
    #[cfg(windows)]
    let original = p.get_text().ok();

    // Write transcript to clipboard
    if let Err(e) = p.set_text(text) {
        // Leave transcript on clipboard as recovery; caller handles fallback
        let _ = p.set_text(text);
        if let Some(path) = fallback_path {
            let _ = save_fallback(text, path);
        }
        return PasteReport {
            paste_status: "failed".into(),
            clipboard_restore_status: "skipped".into(),
            error_message: format!("set_clipboard failed: {e}"),
            recovery_action: "transcript_on_clipboard".into(),
        };
    }

    sleep_ms(paste_delay_ms);

    // Send paste shortcut
    if let Err(e) = p.send_paste_shortcut() {
        if let Some(path) = fallback_path {
            let _ = save_fallback(text, path);
        }
        return PasteReport {
            paste_status: "failed".into(),
            clipboard_restore_status: "skipped".into(),
            error_message: format!("send_paste_shortcut failed: {e}"),
            recovery_action: "transcript_left_on_clipboard".into(),
        };
    }

    let _ = restore_delay_ms;

    #[cfg(target_os = "macos")]
    return PasteReport {
        paste_status: "success".into(),
        clipboard_restore_status: "skipped_macos_transcript_left_on_clipboard".into(),
        error_message: String::new(),
        recovery_action: "manual_cmd_v_available".into(),
    };

    #[cfg(windows)]
    sleep_ms(restore_delay_ms);

    // Restore original clipboard
    #[cfg(windows)]
    let restore_status = if let Some(ref orig) = original {
        match p.set_text(orig) {
            Ok(_) => "success".into(),
            Err(e) => {
                if let Some(path) = fallback_path {
                    let _ = save_fallback(text, path);
                }
                format!("restore_failed: {e}")
            }
        }
    } else {
        "no_original".into()
    };

    #[cfg(windows)]
    PasteReport {
        paste_status: "success".into(),
        clipboard_restore_status: restore_status,
        error_message: String::new(),
        recovery_action: String::new(),
    }
}

fn sleep_ms(ms: u64) {
    if ms > 0 {
        thread::sleep(Duration::from_millis(ms));
    }
}

fn save_fallback(text: &str, path: &PathBuf) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, text)
}
