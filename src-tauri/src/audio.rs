use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, SupportedStreamConfig};
use std::sync::{Arc, Mutex};

pub struct AudioCapture {
    _stream: cpal::Stream,
    buffer: Arc<Mutex<Vec<f32>>>,
    native_sample_rate: u32,
    native_channels: u16,
}

// cpal::Stream is Send + Sync
unsafe impl Send for AudioCapture {}

impl AudioCapture {
    pub fn new(device_id: Option<&str>, max_record_seconds: u64) -> Result<Self> {
        let host = cpal::default_host();

        let device = if let Some(id) = device_id {
            host.input_devices()
                .context("failed to enumerate input devices")?
                .find(|d| d.name().map(|n| n == id).unwrap_or(false))
                .context("specified audio device not found")?
        } else {
            host.default_input_device()
                .context("no default input device")?
        };

        let supported_config = preferred_config(&device)?;
        let native_sample_rate = supported_config.sample_rate().0;
        let native_channels = supported_config.channels();
        let max_samples = (native_sample_rate * native_channels as u32 * max_record_seconds as u32) as usize;

        let buffer: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::with_capacity(max_samples)));
        let buf_clone = buffer.clone();
        let max_samples_clone = max_samples;

        let err_fn = |err| eprintln!("audio stream error: {err}");

        let stream = match supported_config.sample_format() {
            SampleFormat::F32 => {
                let cfg: cpal::StreamConfig = supported_config.into();
                device.build_input_stream(
                    &cfg,
                    move |data: &[f32], _| push_samples_f32(data, &buf_clone, max_samples_clone),
                    err_fn,
                    None,
                )?
            }
            SampleFormat::I16 => {
                let cfg: cpal::StreamConfig = supported_config.into();
                device.build_input_stream(
                    &cfg,
                    move |data: &[i16], _| {
                        let floats: Vec<f32> = data.iter().map(|&s| s as f32 / 32768.0).collect();
                        push_samples_f32(&floats, &buf_clone, max_samples_clone);
                    },
                    err_fn,
                    None,
                )?
            }
            SampleFormat::U16 => {
                let cfg: cpal::StreamConfig = supported_config.into();
                device.build_input_stream(
                    &cfg,
                    move |data: &[u16], _| {
                        let floats: Vec<f32> = data.iter().map(|&s| (s as f32 / 32768.0) - 1.0).collect();
                        push_samples_f32(&floats, &buf_clone, max_samples_clone);
                    },
                    err_fn,
                    None,
                )?
            }
            fmt => anyhow::bail!("unsupported sample format: {fmt}"),
        };

        stream.play().context("failed to start audio stream")?;

        Ok(Self {
            _stream: stream,
            buffer,
            native_sample_rate,
            native_channels,
        })
    }

    /// Stop recording and return a 16kHz mono f32 buffer.
    pub fn stop(self) -> Result<Vec<f32>> {
        // Drop the stream first to stop the callback.
        drop(self._stream);

        let raw = self.buffer.lock().unwrap().clone();

        // Mix to mono if needed.
        let mono = if self.native_channels > 1 {
            raw.chunks(self.native_channels as usize)
                .map(|ch| ch.iter().sum::<f32>() / ch.len() as f32)
                .collect()
        } else {
            raw
        };

        // Resample to 16kHz if device ran at a different rate.
        let samples_16k = if self.native_sample_rate != 16000 {
            resample_to_16k(&mono, self.native_sample_rate)
        } else {
            mono
        };

        Ok(samples_16k)
    }

    pub fn duration_ms(&self) -> u64 {
        let samples = self.buffer.lock().unwrap().len() as u64;
        samples * 1000 / (self.native_sample_rate as u64 * self.native_channels as u64)
    }
}

pub fn list_devices() -> Vec<String> {
    let host = cpal::default_host();
    host.input_devices()
        .map(|devs| devs.filter_map(|d| d.name().ok()).collect())
        .unwrap_or_default()
}

fn preferred_config(device: &cpal::Device) -> Result<SupportedStreamConfig> {
    // Try 16kHz mono first; fall back to device default.
    if let Ok(ranges) = device.supported_input_configs() {
        for range in ranges {
            let min = range.min_sample_rate().0;
            let max = range.max_sample_rate().0;
            if min <= 16000 && 16000 <= max {
                return Ok(range.with_sample_rate(cpal::SampleRate(16000)));
            }
        }
    }
    device.default_input_config().context("no input config available")
}

fn push_samples_f32(data: &[f32], buffer: &Arc<Mutex<Vec<f32>>>, max_samples: usize) {
    let mut buf = buffer.lock().unwrap();
    let remaining = max_samples.saturating_sub(buf.len());
    let to_push = data.len().min(remaining);
    buf.extend_from_slice(&data[..to_push]);
}

fn resample_to_16k(input: &[f32], from_rate: u32) -> Vec<f32> {
    let ratio = 16000.0 / from_rate as f64;
    let out_len = (input.len() as f64 * ratio).ceil() as usize;
    let mut output = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src_f = i as f64 / ratio;
        let src_i = src_f as usize;
        let frac = src_f - src_i as f64;
        let a = *input.get(src_i).unwrap_or(&0.0) as f64;
        let b = *input.get(src_i + 1).unwrap_or(&0.0) as f64;
        output.push((a + frac * (b - a)) as f32);
    }
    output
}
