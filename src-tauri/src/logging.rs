use serde_json::Value;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use time::{OffsetDateTime, format_description};

fn log_dir() -> PathBuf {
    #[cfg(windows)]
    {
        let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".into());
        PathBuf::from(appdata).join("voice-mcp-host").join("logs")
    }
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(home).join("Library").join("Logs").join("voice-mcp-host")
    }
}

fn log_file_path() -> PathBuf {
    let now = OffsetDateTime::now_utc();
    let fmt = format_description::parse("[year]-[month]-[day]").unwrap();
    let date = now.format(&fmt).unwrap_or_else(|_| "unknown".into());
    log_dir().join(format!("events-{date}.jsonl"))
}

pub fn write_event(event: &str, payload: Option<Value>) {
    let now = OffsetDateTime::now_utc();
    let fmt = format_description::parse(
        "[year]-[month]-[day]T[hour]:[minute]:[second]Z",
    )
    .unwrap();
    let ts = now.format(&fmt).unwrap_or_default();

    let line = match payload {
        Some(p) => serde_json::json!({ "ts": ts, "event": event, "data": p }),
        None => serde_json::json!({ "ts": ts, "event": event }),
    };

    let path = log_file_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(file, "{}", line);
    }
}

pub fn log_dir_path() -> PathBuf {
    log_dir()
}
