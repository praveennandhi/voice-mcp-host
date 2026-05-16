use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use crate::config::AsrConfig;
use crate::logging;
use crate::model_store;

// Subprocess-based ASR using the precompiled whisper.cpp CLI binary.
// This avoids a compile-time LLVM / bindgen dependency entirely.
// The binary is downloaded by model_store::download_engine() and lives
// next to the model files in the model cache directory.

pub struct Transcriber {
    backend: AsrBackend,
}

enum AsrBackend {
    WhisperCpp(WhisperCppTranscriber),
    FasterWhisper(FasterWhisperTranscriber),
}

unsafe impl Send for Transcriber {}
unsafe impl Sync for Transcriber {}

impl Transcriber {
    pub fn load(cfg: &AsrConfig, cache_dir: &Path) -> Result<Self> {
        let backend = match cfg.backend.as_str() {
            "faster_whisper" => AsrBackend::FasterWhisper(FasterWhisperTranscriber::load(cfg)?),
            _ => {
                let model_path = model_store::model_path(cache_dir, &cfg.model_name);
                AsrBackend::WhisperCpp(WhisperCppTranscriber::load(&model_path)?)
            }
        };
        Ok(Self { backend })
    }

    pub fn transcribe(&self, audio: &[f32], language: &str) -> Result<String> {
        match &self.backend {
            AsrBackend::WhisperCpp(t) => t.transcribe(audio, language),
            AsrBackend::FasterWhisper(t) => t.transcribe(audio, language),
        }
    }
}

struct WhisperCppTranscriber {
    model_path: PathBuf,
    engines: Vec<(String, PathBuf)>,
}

impl WhisperCppTranscriber {
    pub fn load(model_path: &Path) -> Result<Self> {
        let cache_dir = model_path
            .parent()
            .context("model path has no parent directory")?;
        let engines = model_store::engine_candidates(cache_dir)
            .into_iter()
            .map(|(kind, path)| (kind.label().to_string(), path))
            .collect::<Vec<_>>();

        if engines.is_empty() {
            let engine_path = engine_path_for(model_path)?;
            bail!(
                "whisper-cli not found at {}. Use the settings panel to download the engine.",
                engine_path.display()
            );
        }
        Ok(Self {
            model_path: model_path.to_owned(),
            engines,
        })
    }

    pub fn transcribe(&self, audio: &[f32], language: &str) -> Result<String> {
        let tmp_dir = std::env::temp_dir();
        let wav_path = tmp_dir.join(format!("vmh-{}.wav", std::process::id()));

        // Log sample stats so we can tell if the mic is capturing non-silent audio
        let sample_count = audio.len();
        let max_amplitude = audio.iter().copied().fold(0.0f32, |a, s| a.max(s.abs()));
        logging::write_event("audio_stats", Some(serde_json::json!({
            "samples": sample_count,
            "max_amplitude": max_amplitude,
            "duration_s": sample_count as f32 / 16000.0,
        })));

        write_wav(&wav_path, audio)?;

        let result = self.run_with_fallback(&wav_path, language);
        let _ = std::fs::remove_file(&wav_path);
        result
    }

    fn run_with_fallback(&self, wav_path: &Path, language: &str) -> Result<String> {
        let mut last_error = None;
        for (kind, engine_path) in &self.engines {
            match run_engine(engine_path, &self.model_path, wav_path, language, kind) {
                Ok(text) => return Ok(text),
                Err(e) => {
                    logging::write_event("whisper_cli_engine_failed", Some(serde_json::json!({
                        "engine": kind,
                        "path": engine_path.display().to_string(),
                        "error": e.to_string(),
                    })));
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("no whisper-cli engine available")))
    }
}

struct FasterWhisperTranscriber {
    model_name: String,
    device: String,
    compute_type: String,
    python_path: Option<String>,
}

impl FasterWhisperTranscriber {
    fn load(cfg: &AsrConfig) -> Result<Self> {
        let sidecar = sidecar_script_path()?;
        let device = normalize_faster_whisper_device(&cfg.faster_whisper_device);
        let compute_type = normalize_faster_whisper_compute_type(&device, &cfg.faster_whisper_compute_type);
        let output = hidden_command(python_exe(cfg.faster_whisper_python_path.as_deref()))
            .args([
                sidecar.to_string_lossy().as_ref(),
                "--preflight",
                "--json",
                "--device",
                &device,
                "--compute-type",
                &compute_type,
            ])
            .output()
            .context("failed to run faster-whisper preflight. Install Python and faster-whisper, or switch ASR backend to whisper.cpp")?;

        if !output.status.success() {
            bail!(
                "faster-whisper preflight failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let value: serde_json::Value = serde_json::from_str(stdout.trim())
            .context("faster-whisper preflight returned invalid JSON")?;
        let result = value.get("result").unwrap_or(&value);
        if result
            .get("faster_whisper_available")
            .and_then(|v| v.as_bool())
            != Some(true)
        {
            bail!("faster-whisper Python package is not installed");
        }

        let supported_key = if device == "cuda" {
            if result.get("cuda_ok").and_then(|v| v.as_bool()) != Some(true) {
                bail!("faster-whisper CUDA is not available. Switch device to CPU or install CUDA/cuDNN support");
            }
            "cuda_compute_types"
        } else {
            "cpu_compute_types"
        };

        let supported_compute_types = result
            .get(supported_key)
            .and_then(|v| v.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        if !supported_compute_types.is_empty()
            && !supported_compute_types.iter().any(|item| *item == compute_type)
        {
            bail!(
                "faster-whisper compute type '{}' is not supported on {}. Supported: {}",
                compute_type,
                device,
                supported_compute_types.join(", ")
            );
        }

        Ok(Self {
            model_name: cfg.faster_whisper_model_name.clone(),
            device,
            compute_type,
            python_path: cfg.faster_whisper_python_path.clone(),
        })
    }

    fn transcribe(&self, audio: &[f32], language: &str) -> Result<String> {
        let tmp_dir = std::env::temp_dir();
        let wav_path = tmp_dir.join(format!("vmh-fw-{}.wav", std::process::id()));
        log_audio_stats(audio);
        write_wav(&wav_path, audio)?;

        let sidecar = sidecar_script_path()?;
        let output = hidden_command(python_exe(self.python_path.as_deref()))
            .args([
                sidecar.to_string_lossy().as_ref(),
                "--transcribe",
                wav_path.to_string_lossy().as_ref(),
                "--json",
                "--model",
                &self.model_name,
                "--device",
                &self.device,
                "--compute-type",
                &self.compute_type,
                "--language",
                language,
            ])
            .output()
            .context("failed to run faster-whisper sidecar")?;
        let _ = std::fs::remove_file(&wav_path);

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        logging::write_event("faster_whisper_raw", Some(serde_json::json!({
            "model": self.model_name,
            "device": self.device,
            "compute_type": self.compute_type,
            "exit_code": output.status.code(),
            "stdout_bytes": output.stdout.len(),
            "stderr_bytes": output.stderr.len(),
            "stderr_preview": stderr.chars().take(300).collect::<String>(),
        })));

        if !output.status.success() {
            bail!("faster-whisper exited with {}: {}", output.status, stderr.trim());
        }

        let value: serde_json::Value = serde_json::from_str(stdout.trim())
            .context("faster-whisper returned invalid JSON")?;
        if value.get("ok").and_then(|v| v.as_bool()) != Some(true) {
            let message = value
                .pointer("/error/message")
                .and_then(|v| v.as_str())
                .unwrap_or("faster-whisper transcription failed");
            bail!("{message}");
        }

        Ok(value
            .pointer("/result/text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string())
    }
}

fn normalize_faster_whisper_device(device: &str) -> String {
    match device {
        "cpu" => "cpu".into(),
        _ => "cuda".into(),
    }
}

fn normalize_faster_whisper_compute_type(device: &str, compute_type: &str) -> String {
    match device {
        "cpu" => match compute_type {
            "float32" | "int8" => compute_type.into(),
            _ => "int8".into(),
        },
        _ => match compute_type {
            "float16" | "int8_float16" | "int8" => compute_type.into(),
            _ => "float16".into(),
        },
    }
}

pub fn engine_path_for(model_path: &Path) -> Result<PathBuf> {
    let dir = model_path
        .parent()
        .context("model path has no parent directory")?;
    let name = if cfg!(windows) { "whisper-cli.exe" } else { "whisper-cli" };
    Ok(dir.join(name))
}

fn write_wav(path: &Path, samples: &[f32]) -> Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)
        .context("failed to create temp WAV file")?;
    for &s in samples {
        let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        writer.write_sample(v).context("failed to write WAV sample")?;
    }
    writer.finalize().context("failed to finalize WAV file")?;
    Ok(())
}

fn log_audio_stats(audio: &[f32]) {
    let sample_count = audio.len();
    let max_amplitude = audio.iter().copied().fold(0.0f32, |a, s| a.max(s.abs()));
    logging::write_event("audio_stats", Some(serde_json::json!({
        "samples": sample_count,
        "max_amplitude": max_amplitude,
        "duration_s": sample_count as f32 / 16000.0,
    })));
}

fn sidecar_script_path() -> Result<PathBuf> {
    let exe = std::env::current_exe().context("failed to resolve current executable")?;
    let exe_dir = exe.parent().context("current executable has no parent")?;
    let candidates = [
        exe_dir.join("sidecars").join("voice_mcp_asr_sidecar.py"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sidecars").join("voice_mcp_asr_sidecar.py"),
    ];
    candidates
        .into_iter()
        .find(|p| p.exists())
        .ok_or_else(|| anyhow::anyhow!("faster-whisper sidecar script not found"))
}

fn python_exe(configured: Option<&str>) -> String {
    if let Some(path) = configured {
        if !path.trim().is_empty() {
            return path.into();
        }
    }
    if let Some(path) = bundled_or_dev_python() {
        return path.to_string_lossy().into_owned();
    }
    if cfg!(windows) { "python".into() } else { "python3".into() }
}

fn bundled_or_dev_python() -> Option<PathBuf> {
    let mut candidates = Vec::new();

        if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            candidates.push(exe_dir.join("bundled").join("faster-whisper").join("python.exe"));
            candidates.push(exe_dir.join("faster-whisper").join("python.exe"));
            candidates.push(exe_dir.join("python").join("python.exe"));
        }
    }

    #[cfg(windows)]
    {
        if let Ok(local_appdata) = std::env::var("LOCALAPPDATA") {
            candidates.push(
                PathBuf::from(local_appdata)
                    .join("voice-mcp-host")
                    .join("faster-whisper")
                    .join("python.exe"),
            );
        }
        candidates.push(
            PathBuf::from(r"C:\HermesApps\speaktype-tauri\.venv-asr\Scripts\python.exe"),
        );
    }

    candidates.into_iter().find(|path| path.exists())
}

fn run_engine(engine: &Path, model: &Path, wav: &Path, language: &str, engine_kind: &str) -> Result<String> {
    let output = hidden_command(engine)
        .args([
            "-m", &model.to_string_lossy(),
            "-f", &wav.to_string_lossy(),
            "-l", language,
            "-nt",  // no timestamps
            // Note: -np (no-prints) is intentionally omitted — some builds suppress
            // all stdout when that flag is set, producing empty output.
        ])
        .output()
        .with_context(|| format!("failed to launch {}", engine.display()))?;

    let stdout_raw = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr_raw = String::from_utf8_lossy(&output.stderr).to_string();

    logging::write_event("whisper_cli_raw", Some(serde_json::json!({
        "engine": engine_kind,
        "path": engine.display().to_string(),
        "exit_code": output.status.code(),
        "stdout_bytes": output.stdout.len(),
        "stderr_bytes": output.stderr.len(),
        "stdout_preview": &stdout_raw.chars().take(400).collect::<String>(),
        "stderr_preview": &stderr_raw.chars().take(200).collect::<String>(),
    })));

    if !output.status.success() {
        bail!("whisper-cli exited with {}: {}", output.status, stderr_raw.trim());
    }

    // Primary: parse stdout. Fallback: some builds write transcript to stderr.
    let source = if !stdout_raw.trim().is_empty() { &stdout_raw } else { &stderr_raw };

    let text = source
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        // Drop pure noise lines from whisper.cpp
        .filter(|l| !l.starts_with("whisper_") && !l.starts_with("ggml_"))
        // Strip bracket tokens ([_BEG_], [BLANK_AUDIO], etc.) from lines; keep any
        // trailing text on the same line (e.g. "[_BEG_] Hello world" → "Hello world")
        .filter_map(|l| {
            if l.starts_with('[') {
                // If the line is purely a bracket token (no text after it), skip it
                let after = l.trim_start_matches(|c: char| c != ']')
                             .trim_start_matches(']')
                             .trim();
                if after.is_empty() { None } else { Some(after) }
            } else {
                Some(l)
            }
        })
        .collect::<Vec<_>>()
        .join(" ");

    Ok(text)
}

fn hidden_command<S: AsRef<std::ffi::OsStr>>(program: S) -> Command {
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}
