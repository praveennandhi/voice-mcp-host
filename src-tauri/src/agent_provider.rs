use anyhow::{bail, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde::Deserialize;
use serde_json::json;
use std::sync::mpsc;
use std::time::Duration;

use crate::agent_prompts::{agent_context, agent_instructions, note_draft_context, note_draft_instructions};
use crate::agent_types::{AgentOutputMode, AgentRequest, AgentResult, AgentSessionTurn, ToolCall};
use crate::config::AgentConfig;

pub fn run_agent(config: &AgentConfig, request: AgentRequest<'_>) -> Result<AgentResult> {
    ensure_openai_provider(config)?;

    let context = agent_context(request);
    let output = call_openai_responses(config, agent_instructions(), &context, "OpenAI request failed")?;
    match parse_agent_result(&output) {
        Ok(result) => Ok(result),
        Err(first_error) => {
            let repair_input = format!(
                "The previous model output was not valid agent JSON.\n\nPrevious output:\n{}\n\nOriginal request context:\n{}\n\nReturn only one compact valid JSON object with this schema: {{\"mode\":\"speak\"|\"insert\"|\"tool\",\"text\":\"...\",\"tool\":{{\"name\":\"...\",\"args\":{{}}}}}}. For speak or insert, omit tool or set it to null.",
                output, context
            );
            let repaired = call_openai_responses(
                config,
                "Repair the output into valid compact JSON only. No markdown fences. No explanation.",
                &repair_input,
                "OpenAI JSON repair request failed",
            )?;
            parse_agent_result(&repaired).with_context(|| {
                format!("OpenAI agent output was not valid JSON after repair. First error: {first_error}. Repaired output: {repaired}")
            })
        }
    }
}

pub fn draft_workspace_note(config: &AgentConfig, command: &str, path: &str, history: &[AgentSessionTurn]) -> Result<String> {
    ensure_openai_provider(config)?;

    call_openai_responses(
        config,
        note_draft_instructions(),
        &note_draft_context(command, path, history),
        "OpenAI note draft request failed",
    )
}

pub fn speak_response(config: &AgentConfig, text: &str) -> Result<()> {
    if !config.enabled || !config.speak_responses || text.trim().is_empty() {
        return Ok(());
    }

    ensure_openai_provider(config)?;
    let api_key = openai_api_key(config).context("OpenAI API key is missing for TTS")?;
    let url = format!("{}/audio/speech", config.base_url.trim_end_matches('/'));
    let body = json!({
        "model": config.tts_model,
        "voice": config.tts_voice,
        "input": text,
        "instructions": "Natural, clear assistant voice. Keep a calm, helpful tone.",
        "response_format": "pcm",
    });

    let response = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(90))
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

    play_pcm_audio(&bytes)
}

fn ensure_openai_provider(config: &AgentConfig) -> Result<()> {
    if !config.enabled {
        bail!("Agent mode is not enabled. Add an OpenAI API key in Settings.");
    }
    if config.provider != "openai" {
        bail!("Unsupported agent provider: {}", config.provider);
    }
    Ok(())
}

fn call_openai_responses(config: &AgentConfig, instructions: &str, input: &str, error_context: &'static str) -> Result<String> {
    let api_key = openai_api_key(config).context("OpenAI API key is missing. Add it in Settings.")?;
    let url = format!("{}/responses", config.base_url.trim_end_matches('/'));
    let body = json!({
        "model": config.model,
        "instructions": instructions,
        "input": input,
    });

    let response = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(90))
        .build()
        .context("failed to build OpenAI HTTP client")?
        .post(url)
        .bearer_auth(api_key)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(body.to_string())
        .send()
        .context(error_context)?;

    let status = response.status();
    let response_text = response.text().unwrap_or_default();
    if !status.is_success() {
        bail!("OpenAI returned HTTP {status}: {response_text}");
    }

    parse_output_text(&response_text)
}

fn openai_api_key(config: &AgentConfig) -> Option<String> {
    config
        .api_key
        .as_deref()
        .filter(|key| !key.trim().is_empty())
        .map(str::trim)
        .map(str::to_string)
        .or_else(|| std::env::var("OPENAI_API_KEY").ok())
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

#[derive(Deserialize)]
struct AgentJson {
    mode: String,
    text: String,
    tool: Option<ToolCall>,
}

fn parse_agent_result(output: &str) -> Result<AgentResult> {
    let parsed = parse_agent_json(output)
        .with_context(|| format!("OpenAI agent output was not valid JSON: {output}"))?;
    let text = parsed.text.trim().to_string();
    if text.is_empty() {
        bail!("OpenAI agent returned empty text");
    }

    let mode = match parsed.mode.trim().to_ascii_lowercase().as_str() {
        "speak" => AgentOutputMode::Speak,
        "insert" => AgentOutputMode::Insert,
        "tool" => AgentOutputMode::Tool,
        other => bail!("OpenAI agent returned unsupported mode: {other}"),
    };

    if mode == AgentOutputMode::Tool && parsed.tool.is_none() {
        bail!("OpenAI agent returned tool mode without a tool call");
    }

    Ok(AgentResult {
        mode,
        text,
        tool_call: parsed.tool,
    })
}

fn parse_agent_json(output: &str) -> Result<AgentJson> {
    let trimmed = output.trim();
    if let Ok(parsed) = serde_json::from_str::<AgentJson>(trimmed) {
        return Ok(parsed);
    }

    let candidate = extract_json_object(trimmed).unwrap_or_else(|| trimmed.to_string());
    if let Ok(parsed) = serde_json::from_str::<AgentJson>(&candidate) {
        return Ok(parsed);
    }

    let repaired = balance_json_braces(&candidate);
    serde_json::from_str::<AgentJson>(&repaired).context("repaired agent JSON still did not parse")
}

fn extract_json_object(text: &str) -> Option<String> {
    let start = text.find('{')?;
    let end = text.rfind('}').unwrap_or(text.len().saturating_sub(1));
    if end < start {
        return None;
    }
    Some(text[start..=end].to_string())
}

fn balance_json_braces(text: &str) -> String {
    let mut in_string = false;
    let mut escaped = false;
    let mut balance = 0i32;

    for ch in text.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' && in_string {
            escaped = true;
            continue;
        }
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
        if ch == '{' {
            balance += 1;
        } else if ch == '}' {
            balance -= 1;
        }
    }

    let mut repaired = text.trim().to_string();
    for _ in 0..balance.max(0) {
        repaired.push('}');
    }
    repaired
}

fn play_pcm_audio(bytes: &[u8]) -> Result<()> {
    const SOURCE_SAMPLE_RATE: f64 = 24_000.0;

    if bytes.len() < 2 {
        bail!("OpenAI TTS returned empty audio");
    }

    let samples = bytes
        .chunks_exact(2)
        .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]) as f32 / i16::MAX as f32)
        .collect::<Vec<_>>();

    if samples.is_empty() {
        bail!("OpenAI TTS returned no playable samples");
    }

    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .context("no default output audio device is available")?;
    let supported_config = device
        .default_output_config()
        .context("failed to get default output audio config")?;
    let sample_format = supported_config.sample_format();
    let config: cpal::StreamConfig = supported_config.into();
    let channels = config.channels as usize;
    let output_rate = config.sample_rate.0 as f64;
    let step = SOURCE_SAMPLE_RATE / output_rate;
    let timeout = Duration::from_secs_f64(samples.len() as f64 / SOURCE_SAMPLE_RATE + 10.0);
    let (done_tx, done_rx) = mpsc::channel();

    let err_fn = |err| eprintln!("voice-mcp-host TTS output stream error: {err}");
    let stream = match sample_format {
        cpal::SampleFormat::F32 => {
            let mut state = PlaybackState::new(samples, channels, step, done_tx);
            device.build_output_stream(
                &config,
                move |data: &mut [f32], _| state.fill_f32(data),
                err_fn,
                None,
            )
        }
        cpal::SampleFormat::I16 => {
            let mut state = PlaybackState::new(samples, channels, step, done_tx);
            device.build_output_stream(
                &config,
                move |data: &mut [i16], _| state.fill_i16(data),
                err_fn,
                None,
            )
        }
        cpal::SampleFormat::U16 => {
            let mut state = PlaybackState::new(samples, channels, step, done_tx);
            device.build_output_stream(
                &config,
                move |data: &mut [u16], _| state.fill_u16(data),
                err_fn,
                None,
            )
        }
        other => bail!("unsupported output audio sample format: {other:?}"),
    }
    .context("failed to build TTS output stream")?;

    stream.play().context("failed to start TTS output stream")?;
    done_rx
        .recv_timeout(timeout)
        .context("timed out while playing TTS audio")?;
    Ok(())
}

struct PlaybackState {
    samples: Vec<f32>,
    channels: usize,
    pos: f64,
    step: f64,
    sent_done: bool,
    done_tx: mpsc::Sender<()>,
}

impl PlaybackState {
    fn new(samples: Vec<f32>, channels: usize, step: f64, done_tx: mpsc::Sender<()>) -> Self {
        Self {
            samples,
            channels,
            pos: 0.0,
            step,
            sent_done: false,
            done_tx,
        }
    }

    fn next_sample(&mut self) -> f32 {
        let idx = self.pos.floor() as usize;
        if idx >= self.samples.len() {
            if !self.sent_done {
                let _ = self.done_tx.send(());
                self.sent_done = true;
            }
            return 0.0;
        }

        let next_idx = (idx + 1).min(self.samples.len() - 1);
        let frac = (self.pos - idx as f64) as f32;
        let sample = self.samples[idx] + (self.samples[next_idx] - self.samples[idx]) * frac;
        self.pos += self.step;
        sample.clamp(-1.0, 1.0)
    }

    fn fill_f32(&mut self, data: &mut [f32]) {
        for frame in data.chunks_mut(self.channels) {
            let sample = self.next_sample();
            for channel in frame {
                *channel = sample;
            }
        }
    }

    fn fill_i16(&mut self, data: &mut [i16]) {
        for frame in data.chunks_mut(self.channels) {
            let sample = (self.next_sample() * i16::MAX as f32) as i16;
            for channel in frame {
                *channel = sample;
            }
        }
    }

    fn fill_u16(&mut self, data: &mut [u16]) {
        for frame in data.chunks_mut(self.channels) {
            let sample = ((self.next_sample() + 1.0) * 0.5 * u16::MAX as f32) as u16;
            for channel in frame {
                *channel = sample;
            }
        }
    }
}
