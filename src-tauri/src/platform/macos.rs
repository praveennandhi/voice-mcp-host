//! macOS platform implementation.
//! Uses NSPasteboard for clipboard, CGEvent for synthetic Cmd+V,
//! NSWorkspace for foreground app capture, and accessibility-sys for AX checks.

use super::{ClipboardOps, PermissionState, PermissionsOps, PermissionsStatus, TargetWindow, WindowTargetOps};

pub struct MacosPlatform;

impl MacosPlatform {
    pub fn new() -> Self {
        Self
    }
}

// ── ClipboardOps ──────────────────────────────────────────────────────────────

impl ClipboardOps for MacosPlatform {
    fn set_text(&self, text: &str) -> Result<(), String> {
        set_pasteboard_text(text)
    }

    fn send_paste_shortcut(&self) -> Result<(), String> {
        send_cmd_v()
    }
}

// ── WindowTargetOps ───────────────────────────────────────────────────────────

impl WindowTargetOps for MacosPlatform {
    fn capture_foreground(&self) -> TargetWindow {
        capture_foreground_mac()
    }

    fn focus_target(&self, target: &TargetWindow) -> Result<(), String> {
        focus_target_mac(target)
    }
}

// ── PermissionsOps ────────────────────────────────────────────────────────────

impl PermissionsOps for MacosPlatform {
    fn check_permissions(&self) -> PermissionsStatus {
        PermissionsStatus {
            microphone: check_mic_permission(),
            accessibility: check_ax_permission(),
        }
    }

    fn request_accessibility_permission(&self) -> bool {
        let _ = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            .status();
        unsafe { accessibility_sys::AXIsProcessTrustedWithOptions(std::ptr::null()) }
    }
}

// ── macOS clipboard implementation ────────────────────────────────────────────

fn set_pasteboard_text(text: &str) -> Result<(), String> {
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
    use objc2_foundation::NSString;

    unsafe {
        let pb = NSPasteboard::generalPasteboard();
        pb.clearContents();
        let ns_str = NSString::from_str(text);
        let ok = pb.setString_forType(&ns_str, NSPasteboardTypeString);
        if ok {
            Ok(())
        } else {
            Err("NSPasteboard setString:forType: returned false".into())
        }
    }
}

// ── macOS synthetic Cmd+V ─────────────────────────────────────────────────────

fn send_cmd_v() -> Result<(), String> {
    let output = std::process::Command::new("osascript")
        .args(["-e", r#"tell application "System Events" to keystroke "v" using command down"#])
        .output()
        .map_err(|e| format!("failed to run osascript paste: {e}"))?;

    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "osascript paste failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

// ── macOS foreground app capture ──────────────────────────────────────────────

fn capture_foreground_mac() -> TargetWindow {
    use objc2_app_kit::NSWorkspace;

    unsafe {
        let ws = NSWorkspace::sharedWorkspace();
        match ws.frontmostApplication() {
            Some(app) => {
                let process_name = app
                    .localizedName()
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                TargetWindow {
                    hwnd: None,
                    title: String::new(), // per-window title via AX is a 0.2 refinement
                    title_hash: String::new(),
                    process_name,
                    pid: None,
                }
            }
            None => TargetWindow::default(),
        }
    }
}

fn focus_target_mac(target: &TargetWindow) -> Result<(), String> {
    if target.process_name.trim().is_empty() {
        return Ok(());
    }

    let status = std::process::Command::new("open")
        .args(["-a", target.process_name.as_str()])
        .status()
        .map_err(|e| format!("failed to activate target app: {e}"))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "target app activation failed for '{}': {}",
            target.process_name, status
        ))
    }
}

// ── macOS permission checks ───────────────────────────────────────────────────

fn check_mic_permission() -> PermissionState {
    // cpal will trigger/use the macOS microphone permission at capture time.
    // Keep this conservative until AVFoundation bindings are added cleanly.
    PermissionState::Unknown
}

fn check_ax_permission() -> PermissionState {
    let trusted = unsafe { accessibility_sys::AXIsProcessTrusted() };
    if trusted {
        PermissionState::Granted
    } else {
        PermissionState::Denied
    }
}
