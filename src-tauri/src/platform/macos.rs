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
    fn get_text(&self) -> Result<String, String> {
        get_pasteboard_text()
    }

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

    fn focus_target(&self, _target: &TargetWindow) -> Result<(), String> {
        Ok(())
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
        // Passing true triggers the system prompt if not yet determined.
        unsafe { accessibility_sys::AXIsProcessTrustedWithOptions(std::ptr::null()) }
    }
}

// ── macOS clipboard implementation ────────────────────────────────────────────

fn get_pasteboard_text() -> Result<String, String> {
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
    use objc2_foundation::NSString;

    unsafe {
        let pb = NSPasteboard::generalPasteboard();
        let ns_type = NSPasteboardTypeString;
        match pb.stringForType(ns_type) {
            Some(s) => Ok(s.to_string()),
            None => Ok(String::new()),
        }
    }
}

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
    use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, CGKeyCode};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

    // ANSI key code for 'V'
    const KEY_V: CGKeyCode = 9;

    let src = CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
        .map_err(|_| "CGEventSource::new failed")?;

    let down = CGEvent::new_keyboard_event(src.clone(), KEY_V, true)
        .map_err(|_| "CGEvent key-down failed")?;
    down.set_flags(CGEventFlags::CGEventFlagCommand);
    down.post(CGEventTapLocation::HID);

    let up = CGEvent::new_keyboard_event(src, KEY_V, false)
        .map_err(|_| "CGEvent key-up failed")?;
    up.set_flags(CGEventFlags::CGEventFlagCommand);
    up.post(CGEventTapLocation::HID);

    Ok(())
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
