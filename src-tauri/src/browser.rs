use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;

use crate::config::BrowserConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserToolResult {
    pub tool: String,
    pub summary: String,
    pub content: String,
}

pub fn context(config: &BrowserConfig) -> String {
    if !config.enabled {
        return "Browser connector is disabled.".into();
    }
    "Browser connector is available in read-only mode. Tools: browser.open_url, browser.search_web, browser.extract_page_text. It can read public pages and search results, but it cannot click, type, log in, submit forms, or change websites.".into()
}

pub fn validate_tool_args(name: &str, args: &serde_json::Value) -> Result<()> {
    match name {
        "browser.open_url" | "browser.extract_page_text" => {
            let url = required_string(args, "url")?;
            validate_url(url)
        }
        "browser.search_web" => {
            required_string(args, "query")?;
            Ok(())
        }
        other => bail!("unknown browser tool: {other}"),
    }
}

pub fn requires_confirmation(_name: &str) -> bool {
    false
}

pub fn execute(config: &BrowserConfig, name: &str, args: &serde_json::Value) -> Result<BrowserToolResult> {
    if !config.enabled {
        bail!("Browser connector is disabled");
    }
    validate_tool_args(name, args)?;

    let script = browser_script_path().context("browser sidecar script was not found")?;
    let payload = serde_json::json!({
        "tool": name,
        "args": args,
    });

    let output = Command::new("node")
        .arg(&script)
        .arg(payload.to_string())
        .output()
        .with_context(|| {
            format!(
                "failed to start browser sidecar at {}. Install Node.js and run npm install.",
                script.display()
            )
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        bail!(
            "browser sidecar failed: {}{}",
            stderr.trim(),
            if stdout.trim().is_empty() {
                String::new()
            } else {
                format!("; stdout: {}", stdout.trim())
            }
        );
    }

    let result: BrowserToolResult = serde_json::from_slice(&output.stdout)
        .context("browser sidecar returned invalid JSON")?;
    Ok(result)
}

fn browser_script_path() -> Option<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("src-tauri").join("sidecars").join("browser_readonly.mjs"));
        candidates.push(cwd.join("sidecars").join("browser_readonly.mjs"));
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("sidecars").join("browser_readonly.mjs"));
            candidates.push(dir.join("resources").join("sidecars").join("browser_readonly.mjs"));
            candidates.push(dir.join("..").join("Resources").join("sidecars").join("browser_readonly.mjs"));
        }
    }

    candidates.into_iter().find(|path| path.exists())
}

fn validate_url(url: &str) -> Result<()> {
    let lower = url.trim().to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        Ok(())
    } else {
        bail!("browser URL must start with http:// or https://")
    }
}

fn required_string<'a>(args: &'a serde_json::Value, key: &str) -> Result<&'a str> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .with_context(|| format!("missing required string argument: {key}"))
}
