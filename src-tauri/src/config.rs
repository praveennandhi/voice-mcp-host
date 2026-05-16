use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub schema_version: u32,
    pub dictation: DictationConfig,
    pub audio: AudioConfig,
    pub asr: AsrConfig,
    #[serde(default = "default_agent_config")]
    pub agent: AgentConfig,
    pub insertion: InsertionConfig,
    pub privacy: PrivacyConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DictationConfig {
    pub primary_hotkey: String,
    pub language: String,
    pub min_record_ms: u64,
    pub max_record_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    pub input_device_id: Option<String>,
    pub samplerate: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsrConfig {
    #[serde(default = "default_asr_backend")]
    pub backend: String,
    pub model_name: String,
    #[serde(default = "default_faster_whisper_model")]
    pub faster_whisper_model_name: String,
    #[serde(default = "default_faster_whisper_device")]
    pub faster_whisper_device: String,
    #[serde(default = "default_faster_whisper_compute_type")]
    pub faster_whisper_compute_type: String,
    #[serde(default)]
    pub faster_whisper_python_path: Option<String>,
    pub model_cache_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_agent_trigger_word")]
    pub trigger_word: String,
    #[serde(default = "default_agent_provider")]
    pub provider: String,
    #[serde(default = "default_agent_model")]
    pub model: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default = "default_agent_base_url")]
    pub base_url: String,
    #[serde(default = "default_agent_auto_replace_selection")]
    pub auto_replace_selection: bool,
    #[serde(default)]
    pub speak_responses: bool,
    #[serde(default = "default_agent_tts_model")]
    pub tts_model: String,
    #[serde(default = "default_agent_tts_voice")]
    pub tts_voice: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsertionConfig {
    pub paste_delay_ms: u64,
    pub restore_delay_ms: u64,
    pub copy_to_clipboard_on_failure: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacyConfig {
    pub verbose_transcript_logging: bool,
}

impl Config {
    pub fn model_cache_dir(&self) -> PathBuf {
        if let Some(ref dir) = self.asr.model_cache_dir {
            let expanded = dir
                .replace("%LOCALAPPDATA%", &std::env::var("LOCALAPPDATA").unwrap_or_default())
                .replace("~", &dirs_home());
            return PathBuf::from(expanded);
        }
        // Platform default
        #[cfg(windows)]
        {
            let local = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".into());
            PathBuf::from(local).join("voice-mcp-host").join("models")
        }
        #[cfg(target_os = "macos")]
        {
            dirs_home_path()
                .join("Library")
                .join("Application Support")
                .join("voice-mcp-host")
                .join("models")
        }
    }
}

fn dirs_home() -> String {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".into())
}

#[cfg(target_os = "macos")]
fn dirs_home_path() -> PathBuf {
    PathBuf::from(dirs_home())
}

pub fn default_config() -> Config {
    let hotkey = if cfg!(target_os = "macos") { "F5" } else { "F3" };
    Config {
        schema_version: 1,
        dictation: DictationConfig {
            primary_hotkey: hotkey.into(),
            language: "en".into(),
            min_record_ms: 300,
            max_record_seconds: 120,
        },
        audio: AudioConfig {
            input_device_id: None,
            samplerate: 16000,
        },
        asr: AsrConfig {
            backend: default_asr_backend(),
            model_name: "ggml-large-v3-turbo-q5_0.bin".into(),
            faster_whisper_model_name: default_faster_whisper_model(),
            faster_whisper_device: default_faster_whisper_device(),
            faster_whisper_compute_type: default_faster_whisper_compute_type(),
            faster_whisper_python_path: None,
            model_cache_dir: None,
        },
        agent: default_agent_config(),
        insertion: InsertionConfig {
            paste_delay_ms: 100,
            restore_delay_ms: 500,
            copy_to_clipboard_on_failure: true,
        },
        privacy: PrivacyConfig {
            verbose_transcript_logging: false,
        },
    }
}

fn default_agent_config() -> AgentConfig {
    AgentConfig {
        enabled: false,
        trigger_word: default_agent_trigger_word(),
        provider: default_agent_provider(),
        model: default_agent_model(),
        api_key: None,
        base_url: default_agent_base_url(),
        auto_replace_selection: default_agent_auto_replace_selection(),
        speak_responses: false,
        tts_model: default_agent_tts_model(),
        tts_voice: default_agent_tts_voice(),
    }
}

fn default_agent_trigger_word() -> String {
    "agent".into()
}

fn default_agent_provider() -> String {
    "openai".into()
}

fn default_agent_model() -> String {
    "gpt-5.2".into()
}

fn default_agent_base_url() -> String {
    "https://api.openai.com/v1".into()
}

fn default_agent_auto_replace_selection() -> bool {
    true
}

fn default_agent_tts_model() -> String {
    "gpt-4o-mini-tts".into()
}

fn default_agent_tts_voice() -> String {
    "coral".into()
}

fn default_asr_backend() -> String {
    "whisper_cpp".into()
}

fn default_faster_whisper_model() -> String {
    "h2oai/faster-whisper-large-v3-turbo".into()
}

fn default_faster_whisper_device() -> String {
    if cfg!(windows) { "cuda".into() } else { "cpu".into() }
}

fn default_faster_whisper_compute_type() -> String {
    if cfg!(windows) { "float16".into() } else { "int8".into() }
}

pub fn config_path() -> Result<PathBuf> {
    #[cfg(windows)]
    {
        let appdata = std::env::var("APPDATA")?;
        Ok(PathBuf::from(appdata).join("voice-mcp-host").join("config.json"))
    }
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME")?;
        Ok(PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("voice-mcp-host")
            .join("config.json"))
    }
}

pub fn load_or_default() -> Config {
    let path = match config_path() {
        Ok(p) => p,
        Err(_) => return default_config(),
    };

    if !path.exists() {
        let cfg = default_config();
        let _ = save(&cfg);
        return cfg;
    }

    match std::fs::read_to_string(&path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_else(|_| default_config()),
        Err(_) => default_config(),
    }
}

pub fn save(config: &Config) -> Result<()> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(config)?;
    std::fs::write(path, json)?;
    Ok(())
}
