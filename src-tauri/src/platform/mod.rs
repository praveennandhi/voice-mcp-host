use serde::{Deserialize, Serialize};
#[cfg(windows)]
use std::collections::hash_map::DefaultHasher;
#[cfg(windows)]
use std::hash::{Hash, Hasher};

#[cfg(windows)]
pub mod windows;
#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(not(any(windows, target_os = "macos")))]
compile_error!("unsupported platform — only Windows and macOS are supported in 0.1");

#[cfg(windows)]
pub use windows::WindowsPlatform as Platform;
#[cfg(target_os = "macos")]
pub use macos::MacosPlatform as Platform;

pub fn platform() -> Platform {
    Platform::new()
}

// ── Shared types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TargetWindow {
    pub hwnd: Option<u64>,
    pub title: String,
    pub title_hash: String,
    pub process_name: String,
    pub pid: Option<u32>,
}

impl TargetWindow {
    pub fn context_json(&self) -> serde_json::Value {
        serde_json::json!({
            "hwnd": self.hwnd,
            "title": self.title,
            "title_hash": self.title_hash,
            "process_name": self.process_name,
            "pid": self.pid,
        })
    }
}

#[cfg(windows)]
pub fn hash_title(title: &str) -> String {
    let mut hasher = DefaultHasher::new();
    title.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

#[derive(Debug, Clone, Serialize)]
pub struct PermissionsStatus {
    pub microphone: PermissionState,
    pub accessibility: PermissionState,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum PermissionState {
    #[cfg(target_os = "macos")]
    Granted,
    #[cfg(target_os = "macos")]
    Denied,
    #[cfg(target_os = "macos")]
    Unknown,
    #[cfg(windows)]
    NotRequired,
}

// ── Platform traits ───────────────────────────────────────────────────────────

pub trait ClipboardOps {
    fn get_text(&self) -> Result<String, String>;
    fn set_text(&self, text: &str) -> Result<(), String>;
    /// Ctrl+V on Windows, Cmd+V on macOS
    fn send_paste_shortcut(&self) -> Result<(), String>;
}

pub trait WindowTargetOps {
    fn capture_foreground(&self) -> TargetWindow;
    fn focus_target(&self, target: &TargetWindow) -> Result<(), String>;
}

pub trait PermissionsOps {
    fn check_permissions(&self) -> PermissionsStatus;
    /// Opens the system accessibility permission dialog; returns current state after prompt.
    fn request_accessibility_permission(&self) -> bool;
}
