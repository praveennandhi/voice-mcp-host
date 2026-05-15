use std::sync::Mutex;
use crate::audio::AudioCapture;
use crate::asr::Transcriber;
use crate::config::Config;
use crate::platform::TargetWindow;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum RecorderState {
    Idle,
    Recording,
    Transcribing,
    Pasting,
    Ready,
    Error(String),
}

impl RecorderState {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Recording => "recording",
            Self::Transcribing => "transcribing",
            Self::Pasting => "pasting",
            Self::Ready => "ready",
            Self::Error(_) => "error",
        }
    }
}

pub struct AppState {
    pub audio: Mutex<Option<AudioCapture>>,
    pub transcriber: Mutex<Option<Transcriber>>,
    pub target_window: Mutex<Option<TargetWindow>>,
    pub config: Mutex<Config>,
    pub recorder_state: Mutex<RecorderState>,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        Self {
            audio: Mutex::new(None),
            transcriber: Mutex::new(None),
            target_window: Mutex::new(None),
            config: Mutex::new(config),
            recorder_state: Mutex::new(RecorderState::Idle),
        }
    }
}
