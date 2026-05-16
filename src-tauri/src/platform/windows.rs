//! Windows platform implementation.
//! Ported from C:\HermesApps\speaktype-tauri\src-tauri\src\clipboard.rs
//! and window_target.rs, wrapped in the platform trait system.

use super::{ClipboardOps, PermissionState, PermissionsOps, PermissionsStatus, TargetWindow, WindowTargetOps, hash_title};

const KEYEVENTF_KEYUP: u32 = 0x0002;
const VK_CONTROL: u8 = 0x11;
const VK_V: u8 = 0x56;

pub struct WindowsPlatform;

impl WindowsPlatform {
    pub fn new() -> Self {
        Self
    }
}

// ── ClipboardOps ──────────────────────────────────────────────────────────────

impl ClipboardOps for WindowsPlatform {
    fn get_text(&self) -> Result<String, String> {
        get_clipboard_text()
    }

    fn set_text(&self, text: &str) -> Result<(), String> {
        set_clipboard_text(text)
    }

    fn send_paste_shortcut(&self) -> Result<(), String> {
        send_ctrl_v()
    }
}

// ── WindowTargetOps ───────────────────────────────────────────────────────────

impl WindowTargetOps for WindowsPlatform {
    fn capture_foreground(&self) -> TargetWindow {
        capture_foreground_win()
    }

    fn focus_target(&self, target: &TargetWindow) -> Result<(), String> {
        focus_target_win(target)
    }
}

// ── PermissionsOps ────────────────────────────────────────────────────────────

impl PermissionsOps for WindowsPlatform {
    fn check_permissions(&self) -> PermissionsStatus {
        PermissionsStatus {
            microphone: PermissionState::NotRequired,
            accessibility: PermissionState::NotRequired,
        }
    }

    fn request_accessibility_permission(&self) -> bool {
        true
    }
}

// ── Windows clipboard implementation ─────────────────────────────────────────

fn get_clipboard_text() -> Result<String, String> {
    use std::{ffi::OsString, os::windows::ffi::OsStringExt, ptr};
    use windows_sys::Win32::{
        System::DataExchange::{CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard},
        System::Memory::{GlobalLock, GlobalUnlock},
    };
    const CF_UNICODETEXT: u32 = 13;

    unsafe {
        if IsClipboardFormatAvailable(CF_UNICODETEXT) == 0 {
            return Ok(String::new());
        }
        if OpenClipboard(ptr::null_mut()) == 0 {
            return Err("OpenClipboard failed".into());
        }
        let handle = GetClipboardData(CF_UNICODETEXT);
        if handle.is_null() {
            CloseClipboard();
            return Err("GetClipboardData failed".into());
        }
        let ptr = GlobalLock(handle) as *const u16;
        if ptr.is_null() {
            CloseClipboard();
            return Err("GlobalLock failed".into());
        }
        let mut len = 0;
        while *ptr.add(len) != 0 {
            len += 1;
        }
        let text = OsString::from_wide(std::slice::from_raw_parts(ptr, len))
            .to_string_lossy()
            .into_owned();
        GlobalUnlock(handle);
        CloseClipboard();
        Ok(text)
    }
}

fn set_clipboard_text(text: &str) -> Result<(), String> {
    use std::{ffi::OsStr, os::windows::ffi::OsStrExt, ptr};
    use windows_sys::Win32::{
        System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData},
        System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE},
    };
    const CF_UNICODETEXT: u32 = 13;

    let wide: Vec<u16> = OsStr::new(text).encode_wide().chain(Some(0)).collect();
    unsafe {
        if OpenClipboard(ptr::null_mut()) == 0 {
            return Err("OpenClipboard failed".into());
        }
        if EmptyClipboard() == 0 {
            CloseClipboard();
            return Err("EmptyClipboard failed".into());
        }
        let bytes = wide.len() * std::mem::size_of::<u16>();
        let handle = GlobalAlloc(GMEM_MOVEABLE, bytes);
        if handle.is_null() {
            CloseClipboard();
            return Err("GlobalAlloc failed".into());
        }
        let locked = GlobalLock(handle) as *mut u16;
        if locked.is_null() {
            CloseClipboard();
            return Err("GlobalLock failed".into());
        }
        ptr::copy_nonoverlapping(wide.as_ptr(), locked, wide.len());
        GlobalUnlock(handle);
        if SetClipboardData(CF_UNICODETEXT, handle).is_null() {
            CloseClipboard();
            return Err("SetClipboardData failed".into());
        }
        CloseClipboard();
        Ok(())
    }
}

fn send_ctrl_v() -> Result<(), String> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::keybd_event;
    unsafe {
        keybd_event(VK_CONTROL, 0, 0, 0);
        keybd_event(VK_V, 0, 0, 0);
        keybd_event(VK_V, 0, KEYEVENTF_KEYUP, 0);
        keybd_event(VK_CONTROL, 0, KEYEVENTF_KEYUP, 0);
    }
    Ok(())
}

// ── Windows window target implementation ──────────────────────────────────────

fn capture_foreground_win() -> TargetWindow {
    use std::{ffi::OsString, os::windows::ffi::OsStringExt, ptr};
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::{
            ProcessStatus::K32GetModuleBaseNameW,
            Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ},
        },
        UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId},
    };

    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return TargetWindow::default();
        }

        let len = GetWindowTextLengthW(hwnd);
        let mut title_buf = vec![0u16; (len as usize).saturating_add(1)];
        let copied = GetWindowTextW(hwnd, title_buf.as_mut_ptr(), title_buf.len() as i32);
        let title = if copied > 0 {
            OsString::from_wide(&title_buf[..copied as usize])
                .to_string_lossy()
                .into_owned()
        } else {
            String::new()
        };

        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        let process_name = if pid > 0 {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ, 0, pid);
            if !handle.is_null() {
                let mut name_buf = vec![0u16; 260];
                let name_len = K32GetModuleBaseNameW(handle, ptr::null_mut(), name_buf.as_mut_ptr(), name_buf.len() as u32);
                CloseHandle(handle);
                if name_len > 0 {
                    OsString::from_wide(&name_buf[..name_len as usize])
                        .to_string_lossy()
                        .into_owned()
                } else {
                    "unknown".into()
                }
            } else {
                "unknown".into()
            }
        } else {
            "unknown".into()
        };

        TargetWindow {
            hwnd: Some(hwnd as usize as u64),
            title: title.clone(),
            title_hash: hash_title(&title),
            process_name,
            pid: if pid > 0 { Some(pid) } else { None },
        }
    }
}

fn focus_target_win(target: &TargetWindow) -> Result<(), String> {
    let Some(hwnd) = target.hwnd else {
        return Ok(());
    };
    unsafe {
        let ok = windows_sys::Win32::UI::WindowsAndMessaging::SetForegroundWindow(
            hwnd as usize as windows_sys::Win32::Foundation::HWND,
        );
        if ok == 0 {
            return Err("SetForegroundWindow failed".into());
        }
    }
    Ok(())
}
