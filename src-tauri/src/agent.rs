use anyhow::{bail, Context, Result};
use serde_json::json;
use std::path::Path;
use std::process::Command;

use crate::config::AgentConfig;

pub struct AgentRequest<'a> {
    pub command: &'a str,
    pub selected_text: Option<&'a str>,
    pub target_app: &'a str,
}

pub fn run_agent(config: &AgentConfig, request: AgentRequest<'_>) -> Result<String> {
    if !config.enabled {
        bail!("Agent mode is not enabled. Add an OpenAI API key in Settings.");
    }

    if config.provider != "openai" {
        bail!("Unsupported agent provider: {}", config.provider);
    }

    let api_key = config
        .api_key
        .as_deref()
        .filter(|key| !key.trim().is_empty())
        .map(str::trim)
        .map(str::to_string)
        .or_else(|| std::env::var("OPENAI_API_KEY").ok())
        .context("OpenAI API key is missing. Add it in Settings.")?;

    let selected_text = request.selected_text.unwrap_or("").trim();
    let context = if selected_text.is_empty() {
        format!(
            "Target app: {}\nUser command/content:\n{}",
            request.target_app, request.command
        )
    } else {
        format!(
            "Target app: {}\nUser command:\n{}\n\nSelected text:\n{}",
            request.target_app, request.command, selected_text
        )
    };

    let instructions = "You are voice-mcp-host's writing agent. Follow the user's spoken command. \
If selected text is provided, transform or answer using that selected text. \
If no selected text is provided, use the spoken content itself. \
Return only the final text to insert into the user's active app. \
Do not include explanations, markdown fences, preambles, or labels unless the user explicitly asks.";

    let url = format!("{}/responses", config.base_url.trim_end_matches('/'));
    let body = json!({
        "model": config.model,
        "instructions": instructions,
        "input": context,
    });

    let response = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(90))
        .build()
        .context("failed to build OpenAI HTTP client")?
        .post(url)
        .bearer_auth(api_key)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(body.to_string())
        .send()
        .context("OpenAI request failed")?;

    let status = response.status();
    let response_text = response.text().unwrap_or_default();
    if !status.is_success() {
        bail!("OpenAI returned HTTP {status}: {response_text}");
    }

    parse_output_text(&response_text)
}

pub fn speak_response(config: &AgentConfig, text: &str) -> Result<()> {
    if !config.enabled || !config.speak_responses || text.trim().is_empty() {
        return Ok(());
    }

    let api_key = config
        .api_key
        .as_deref()
        .filter(|key| !key.trim().is_empty())
        .map(str::trim)
        .map(str::to_string)
        .or_else(|| std::env::var("OPENAI_API_KEY").ok())
        .context("OpenAI API key is missing for TTS")?;

    let url = format!("{}/audio/speech", config.base_url.trim_end_matches('/'));
    let body = json!({
        "model": config.tts_model,
        "voice": config.tts_voice,
        "input": text,
        "instructions": "Natural, clear assistant voice. Keep a calm, helpful tone.",
        "response_format": "wav",
    });

    let response = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(90))
        .build()
        .context("failed to build OpenAI TTS HTTP client")?
        .post(url)
        .bearer_auth(api_key)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(body.to_string())
        .send()
        .context("OpenAI TTS request failed")?;

    let status = response.status();
    let bytes = response.bytes().context("failed to read OpenAI TTS response")?;
    if !status.is_success() {
        bail!("OpenAI TTS returned HTTP {status}: {}", String::from_utf8_lossy(&bytes));
    }

    let audio_path = std::env::temp_dir().join(format!("voice-mcp-host-tts-{}.wav", std::process::id()));
    std::fs::write(&audio_path, &bytes).context("failed to write TTS audio file")?;
    let play_result = play_audio_file(&audio_path);
    let _ = std::fs::remove_file(&audio_path);
    play_result
}

fn parse_output_text(body: &str) -> Result<String> {
    let value: serde_json::Value = serde_json::from_str(body)
        .context("OpenAI response was not valid JSON")?;

    if let Some(text) = value.get("output_text").and_then(|v| v.as_str()) {
        return Ok(text.trim().to_string());
    }

    let mut chunks = Vec::new();
    if let Some(output) = value.get("output").and_then(|v| v.as_array()) {
        for item in output {
            if let Some(content) = item.get("content").and_then(|v| v.as_array()) {
                for part in content {
                    if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                        chunks.push(text.trim());
                    }
                }
            }
        }
    }

    let text = chunks
        .into_iter()
        .filter(|chunk| !chunk.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    if text.is_empty() {
        bail!("OpenAI response did not include output text");
    }
    Ok(text)
}

fn play_audio_file(path: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        let script = format!(
            "Add-Type -AssemblyName System; $p = New-Object System.Media.SoundPlayer '{}'; $p.PlaySync()",
            escape_powershell_path(path)
        );
        let output = hidden_command("powershell")
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &script])
            .output()
            .context("failed to start Windows audio playback")?;
        if output.status.success() {
            return Ok(());
        }
        bail!(
            "Windows audio playback failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    #[cfg(target_os = "macos")]
    {
        let output = Command::new("afplay")
            .arg(path)
            .output()
            .context("failed to start macOS audio playback")?;
        if output.status.success() {
            return Ok(());
        }
        bail!(
            "macOS audio playback failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
}

#[cfg(windows)]
fn escape_powershell_path(path: &Path) -> String {
    path.to_string_lossy().replace('\'', "''")
}

#[cfg(windows)]
fn hidden_command<S: AsRef<std::ffi::OsStr>>(program: S) -> Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut command = Command::new(program);
    command.creation_flags(CREATE_NO_WINDOW);
    command
}
