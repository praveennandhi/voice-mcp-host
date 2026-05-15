use anyhow::{bail, Context, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter};

const HF_BASE: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

/// Known models with their approximate sizes. SHA-256 hashes should be verified
/// against the upstream repo before release: https://github.com/ggerganov/whisper.cpp
const MODELS: &[ModelInfo] = &[
    ModelInfo {
        name: "ggml-tiny.en.bin",
        size_bytes: 77_704_715,
        sha256: None, // TODO: fill in before release
    },
    ModelInfo {
        name: "ggml-base.en.bin",
        size_bytes: 147_964_211,
        sha256: None, // TODO: fill in before release
    },
    ModelInfo {
        name: "ggml-small.en.bin",
        size_bytes: 487_601_967,
        sha256: None, // TODO: fill in before release
    },
    ModelInfo {
        name: "ggml-medium.en.bin",
        size_bytes: 1_533_763_179,
        sha256: None, // TODO: fill in before release
    },
];

struct ModelInfo {
    name: &'static str,
    size_bytes: u64,
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
    pub downloaded: bool,
    pub size_bytes: u64,
}

pub fn list_models(cache_dir: &Path) -> Vec<AvailableModel> {
    MODELS
        .iter()
        .map(|m| AvailableModel {
            name: m.name.into(),
            downloaded: cache_dir.join(m.name).exists(),
            size_bytes: m.size_bytes,
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
