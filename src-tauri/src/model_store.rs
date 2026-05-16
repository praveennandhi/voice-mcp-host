use anyhow::{bail, Context, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter};

const HF_BASE: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

#[cfg(windows)]
const CPU_ENGINE_URL: &str = "https://github.com/ggml-org/whisper.cpp/releases/download/v1.8.4/whisper-bin-x64.zip";
#[cfg(windows)]
const CUDA_ENGINE_URL: &str = "https://github.com/ggml-org/whisper.cpp/releases/download/v1.8.4/whisper-cublas-12.4.0-bin-x64.zip";
/// Known models with their approximate sizes. SHA-256 hashes should be verified
/// against the upstream repo before release: https://github.com/ggerganov/whisper.cpp
const MODELS: &[ModelInfo] = &[
    ModelInfo {
        name: "ggml-tiny.en.bin",
        display_name: "Tiny English",
        description: "Smallest English-only model. Fast, lowest accuracy.",
        size_bytes: 77_704_715,
        recommended: false,
        sha256: None, // TODO: fill in before release
    },
    ModelInfo {
        name: "ggml-base.en.bin",
        display_name: "Base English",
        description: "Good starter model for older CPUs. Fast, basic accuracy.",
        size_bytes: 147_964_211,
        recommended: false,
        sha256: None, // TODO: fill in before release
    },
    ModelInfo {
        name: "ggml-small.en.bin",
        display_name: "Small English",
        description: "Balanced English-only model. Better accuracy without a huge download.",
        size_bytes: 487_601_967,
        recommended: false,
        sha256: None, // TODO: fill in before release
    },
    ModelInfo {
        name: "ggml-medium.en.bin",
        display_name: "Medium English",
        description: "Strong English-only accuracy. Larger and slower than Small.",
        size_bytes: 1_533_763_179,
        recommended: false,
        sha256: None, // TODO: fill in before release
    },
    ModelInfo {
        name: "ggml-large-v3-turbo-q5_0.bin",
        display_name: "Large v3 Turbo Q5",
        description: "Recommended. Fast Turbo model with a smaller download and strong accuracy.",
        size_bytes: 574_000_000,
        recommended: true,
        sha256: None, // TODO: fill in before release
    },
    ModelInfo {
        name: "ggml-large-v3-turbo-q8_0.bin",
        display_name: "Large v3 Turbo Q8",
        description: "Higher-quality Turbo quantization. Bigger download than Q5.",
        size_bytes: 874_000_000,
        recommended: false,
        sha256: None, // TODO: fill in before release
    },
    ModelInfo {
        name: "ggml-large-v3-turbo.bin",
        display_name: "Large v3 Turbo",
        description: "Full Turbo model. Best Turbo quality, largest Turbo download.",
        size_bytes: 1_620_000_000,
        recommended: false,
        sha256: None, // TODO: fill in before release
    },
    ModelInfo {
        name: "ggml-large-v3-q5_0.bin",
        display_name: "Large v3 Q5",
        description: "High-accuracy non-Turbo model. Slower than Turbo.",
        size_bytes: 1_180_000_000,
        recommended: false,
        sha256: None, // TODO: fill in before release
    },
    ModelInfo {
        name: "ggml-large-v3.bin",
        display_name: "Large v3",
        description: "Full large model. Highest accuracy, slowest and biggest download.",
        size_bytes: 3_100_000_000,
        recommended: false,
        sha256: None, // TODO: fill in before release
    },
];

struct ModelInfo {
    name: &'static str,
    display_name: &'static str,
    description: &'static str,
    size_bytes: u64,
    recommended: bool,
    sha256: Option<&'static str>,
}

#[derive(Clone, Serialize)]
pub struct DownloadProgress {
    pub model: String,
    pub downloaded: u64,
    pub total: u64,
    pub percent: f32,
}

#[derive(Clone, Serialize)]
pub struct AvailableModel {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub downloaded: bool,
    pub size_bytes: u64,
    pub recommended: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineKind {
    Cpu,
    Cuda,
    #[cfg(target_os = "macos")]
    Macos,
    Legacy,
}

impl EngineKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Cuda => "cuda",
            #[cfg(target_os = "macos")]
            Self::Macos => "metal",
            Self::Legacy => "legacy",
        }
    }
}

#[derive(Clone, Serialize)]
pub struct EngineStatus {
    pub gpu_detected: bool,
    pub preferred_acceleration: String,
    pub active_acceleration: Option<String>,
    pub engine_downloaded: bool,
}

pub fn list_models(cache_dir: &Path) -> Vec<AvailableModel> {
    MODELS
        .iter()
        .map(|m| AvailableModel {
            name: m.name.into(),
            display_name: m.display_name.into(),
            description: m.description.into(),
            downloaded: cache_dir.join(m.name).exists(),
            size_bytes: m.size_bytes,
            recommended: m.recommended,
        })
        .collect()
}

pub fn model_path(cache_dir: &Path, name: &str) -> PathBuf {
    cache_dir.join(name)
}

pub fn is_downloaded(cache_dir: &Path, name: &str) -> bool {
    cache_dir.join(name).exists()
}

pub fn download(app: &AppHandle, cache_dir: &Path, model_name: &str) -> Result<PathBuf> {
    let info = MODELS
        .iter()
        .find(|m| m.name == model_name)
        .with_context(|| format!("unknown model: {model_name}"))?;

    std::fs::create_dir_all(cache_dir).context("failed to create model cache directory")?;

    let dest = cache_dir.join(model_name);
    let tmp = cache_dir.join(format!("{model_name}.tmp"));
    let url = format!("{HF_BASE}/{model_name}");

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(3600))
        .build()
        .context("failed to build HTTP client")?;

    let mut response = client.get(&url).send().context("download request failed")?;

    if !response.status().is_success() {
        bail!("download failed: HTTP {}", response.status());
    }

    let total = response.content_length().unwrap_or(info.size_bytes);
    let mut file = std::fs::File::create(&tmp).context("failed to create temp file")?;
    let mut downloaded = 0u64;
    let mut buf = vec![0u8; 65536];

    loop {
        let n = response.read(&mut buf).context("read error during download")?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).context("write error during download")?;
        downloaded += n as u64;

        let _ = app.emit(
            "download-progress",
            DownloadProgress {
                model: model_name.into(),
                downloaded,
                total,
                percent: (downloaded as f32 / total as f32) * 100.0,
            },
        );
    }

    drop(file);

    if let Some(expected) = info.sha256 {
        verify_sha256(&tmp, expected).context("SHA-256 verification failed")?;
    }

    std::fs::rename(&tmp, &dest).context("failed to move model into place")?;

    Ok(dest)
}

pub fn is_engine_downloaded(cache_dir: &Path) -> bool {
    engine_status(cache_dir).engine_downloaded
}

pub fn engine_status(cache_dir: &Path) -> EngineStatus {
    let active = active_engine(cache_dir).map(|(kind, _)| kind.label().to_string());
    EngineStatus {
        gpu_detected: gpu_detected(),
        preferred_acceleration: preferred_engine_kind().label().to_string(),
        engine_downloaded: active.is_some(),
        active_acceleration: active,
    }
}

pub fn engine_candidates(cache_dir: &Path) -> Vec<(EngineKind, PathBuf)> {
    let mut kinds = vec![preferred_engine_kind()];
    if cfg!(windows) && kinds[0] == EngineKind::Cuda {
        kinds.push(EngineKind::Cpu);
    }
    kinds.push(EngineKind::Legacy);

    let mut candidates = Vec::new();
    for kind in kinds {
        let path = engine_path_for_kind(cache_dir, kind);
        if path.exists() && !candidates.iter().any(|(_, p)| p == &path) {
            candidates.push((kind, path));
        }
    }
    #[cfg(target_os = "macos")]
    {
        for path in bundled_macos_engine_candidates() {
            if path.exists() && !candidates.iter().any(|(_, p)| p == &path) {
                candidates.push((EngineKind::Macos, path));
            }
        }
    }
    candidates
}

pub fn active_engine(cache_dir: &Path) -> Option<(EngineKind, PathBuf)> {
    engine_candidates(cache_dir).into_iter().next()
}

pub fn server_candidates(cache_dir: &Path) -> Vec<(EngineKind, PathBuf)> {
    let mut candidates = Vec::new();

    for (kind, engine_path) in engine_candidates(cache_dir) {
        let server_path = engine_path.with_file_name(server_binary_name());
        if server_path.exists() && !candidates.iter().any(|(_, p)| p == &server_path) {
            candidates.push((kind, server_path));
        }
    }

    #[cfg(target_os = "macos")]
    {
        for path in bundled_macos_server_candidates() {
            if path.exists() && !candidates.iter().any(|(_, p)| p == &path) {
                candidates.push((EngineKind::Macos, path));
            }
        }
    }

    candidates
}

/// Download the preferred precompiled whisper-cli binary. On Windows, an
/// NVIDIA GPU selects the CUDA build; otherwise CPU is used.
#[cfg(windows)]
pub fn download_engine(app: &AppHandle, cache_dir: &Path) -> Result<PathBuf> {
    let preferred = preferred_engine_kind();
    if preferred == EngineKind::Cuda {
        match download_engine_kind(app, cache_dir, EngineKind::Cuda) {
            Ok(path) => return Ok(path),
            Err(e) => {
                let _ = app.emit(
                    "engine-fallback",
                    serde_json::json!({
                        "from": "cuda",
                        "to": "cpu",
                        "error": e.to_string(),
                    }),
                );
            }
        }
    }

    download_engine_kind(app, cache_dir, fallback_engine_kind())
}

#[cfg(target_os = "macos")]
pub fn download_engine(_app: &AppHandle, _cache_dir: &Path) -> Result<PathBuf> {
    for path in bundled_macos_engine_candidates() {
        if path.exists() {
            return Ok(path);
        }
    }
    bail!("macOS whisper-cli engine was not bundled. Rebuild on macOS with cmake installed: brew install cmake && npm run tauri build");
}

#[cfg(target_os = "macos")]
fn bundled_macos_engine_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            candidates.push(exe_dir.join("../Resources/bundled/whisper-macos/whisper-cli"));
            candidates.push(exe_dir.join("bundled/whisper-macos/whisper-cli"));
        }
    }
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("bundled")
            .join("whisper-macos")
            .join("whisper-cli"),
    );
    candidates
}

#[cfg(target_os = "macos")]
fn bundled_macos_server_candidates() -> Vec<PathBuf> {
    bundled_macos_engine_candidates()
        .into_iter()
        .map(|path| path.with_file_name("whisper-server"))
        .collect()
}

#[cfg(windows)]
fn download_engine_kind(app: &AppHandle, cache_dir: &Path, kind: EngineKind) -> Result<PathBuf> {
    let out_dir = engine_dir_for_kind(cache_dir, kind);
    std::fs::create_dir_all(&out_dir).context("failed to create engine cache directory")?;

    let zip_tmp = out_dir.join(format!("whisper-engine-{}.zip.tmp", kind.label()));
    let dest = engine_path_for_kind(cache_dir, kind);

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .user_agent("voice-mcp-host/0.1")
        .build()
        .context("failed to build HTTP client")?;

    let mut response = client
        .get(engine_url(kind))
        .send()
        .context("engine download request failed")?;
    if !response.status().is_success() {
        bail!("engine download failed: HTTP {}", response.status());
    }

    let total = response.content_length().unwrap_or(50_000_000);
    let mut file = std::fs::File::create(&zip_tmp).context("failed to create zip temp file")?;
    let mut downloaded = 0u64;
    let mut buf = vec![0u8; 65536];

    loop {
        let n = response
            .read(&mut buf)
            .context("read error during engine download")?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])
            .context("write error during engine download")?;
        downloaded += n as u64;
        let _ = app.emit(
            "download-progress",
            DownloadProgress {
                model: format!("whisper-cli-{}", kind.label()),
                downloaded,
                total,
                percent: (downloaded as f32 / total as f32) * 100.0,
            },
        );
    }
    drop(file);

    let zip_file = std::fs::File::open(&zip_tmp).context("failed to open zip")?;
    let mut archive = zip::ZipArchive::new(zip_file).context("failed to read zip")?;

    let mut found_binary = false;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).context("failed to read zip entry")?;
        let entry_name = entry.name().to_string();
        let is_cli = entry_name.ends_with("whisper-cli.exe") || entry_name.ends_with("whisper-cli");
        let is_server = entry_name.ends_with("whisper-server.exe") || entry_name.ends_with("whisper-server");
        let is_needed_library = entry_name.ends_with(".dll") || entry_name.ends_with(".dylib");
        if is_cli || is_server || is_needed_library {
            let file_name = std::path::Path::new(&entry_name)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(entry_name.as_str())
                .to_string();
            let out_path = out_dir.join(&file_name);
            let mut out = std::fs::File::create(&out_path)
                .with_context(|| format!("failed to create {file_name}"))?;
            std::io::copy(&mut entry, &mut out)
                .with_context(|| format!("failed to extract {file_name}"))?;
            if is_cli {
                found_binary = true;
            }
        }
    }

    let _ = std::fs::remove_file(&zip_tmp);

    if !found_binary {
        bail!("whisper-cli binary not found inside the downloaded zip");
    }

    Ok(dest)
}

pub fn engine_path_for_kind(cache_dir: &Path, kind: EngineKind) -> PathBuf {
    if kind == EngineKind::Legacy {
        return cache_dir.join(engine_binary_name());
    }
    engine_dir_for_kind(cache_dir, kind).join(engine_binary_name())
}

fn engine_dir_for_kind(cache_dir: &Path, kind: EngineKind) -> PathBuf {
    cache_dir.join("engines").join(kind.label())
}

fn engine_binary_name() -> &'static str {
    if cfg!(windows) {
        "whisper-cli.exe"
    } else {
        "whisper-cli"
    }
}

fn server_binary_name() -> &'static str {
    if cfg!(windows) {
        "whisper-server.exe"
    } else {
        "whisper-server"
    }
}

#[cfg(windows)]
fn engine_url(kind: EngineKind) -> &'static str {
    match kind {
        EngineKind::Cuda => CUDA_ENGINE_URL,
        EngineKind::Cpu => CPU_ENGINE_URL,
        EngineKind::Legacy => unreachable!("legacy engines are never downloaded"),
    }
}

fn preferred_engine_kind() -> EngineKind {
    #[cfg(windows)]
    {
        if gpu_detected() {
            EngineKind::Cuda
        } else {
            EngineKind::Cpu
        }
    }
    #[cfg(target_os = "macos")]
    {
        EngineKind::Macos
    }
}

#[cfg(windows)]
fn fallback_engine_kind() -> EngineKind {
    EngineKind::Cpu
}

fn gpu_detected() -> bool {
    #[cfg(windows)]
    {
        nvidia_smi_detects_gpu() || wmic_detects_nvidia_gpu()
    }
    #[cfg(target_os = "macos")]
    {
        true
    }
}

#[cfg(windows)]
fn nvidia_smi_detects_gpu() -> bool {
    hidden_command("nvidia-smi")
        .arg("-L")
        .output()
        .map(|out| out.status.success() && String::from_utf8_lossy(&out.stdout).contains("GPU"))
        .unwrap_or(false)
}

#[cfg(windows)]
fn wmic_detects_nvidia_gpu() -> bool {
    hidden_command("wmic")
        .args(["path", "win32_VideoController", "get", "name"])
        .output()
        .map(|out| {
            out.status.success()
                && String::from_utf8_lossy(&out.stdout)
                    .to_ascii_lowercase()
                    .contains("nvidia")
        })
        .unwrap_or(false)
}

#[cfg(windows)]
fn hidden_command<S: AsRef<std::ffi::OsStr>>(program: S) -> std::process::Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut command = std::process::Command::new(program);
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

fn verify_sha256(path: &Path, expected: &str) -> Result<()> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 65536];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let actual = hex::encode(hasher.finalize());
    if actual != expected {
        bail!("SHA-256 mismatch: expected {expected}, got {actual}");
    }
    Ok(())
}
