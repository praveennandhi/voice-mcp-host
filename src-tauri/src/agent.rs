use anyhow::{bail, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde_json::json;
use std::sync::mpsc;
use std::time::Duration;

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
        "response_format": "pcm",
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

    play_pcm_audio(&bytes)
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
