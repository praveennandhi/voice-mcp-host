use std::sync::Mutex;
use crate::agent::AgentSessionTurn;
use crate::agent_runtime::PendingToolCall;
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

#[derive(Debug, Clone, Serialize)]
pub struct OverlayPayload {
    pub state: String,
    pub title: String,
    pub subtitle: String,
    pub hide_after_ms: Option<u64>,
}

impl OverlayPayload {
    pub fn idle() -> Self {
        Self {
            state: "idle".into(),
            title: String::new(),
            subtitle: String::new(),
            hide_after_ms: None,
        }
    }
}

pub struct AppState {
    pub audio: Mutex<Option<AudioCapture>>,
    pub transcriber: Mutex<Option<Transcriber>>,
    pub target_window: Mutex<Option<TargetWindow>>,
    pub selected_text: Mutex<Option<String>>,
    pub config: Mutex<Config>,
    pub recorder_state: Mutex<RecorderState>,
    pub overlay_state: Mutex<OverlayPayload>,
    pub agent_session: Mutex<Vec<AgentSessionTurn>>,
    pub pending_tool_call: Mutex<Option<PendingToolCall>>,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        Self {
            audio: Mutex::new(None),
            transcriber: Mutex::new(None),
            target_window: Mutex::new(None),
            selected_text: Mutex::new(None),
            config: Mutex::new(config),
            recorder_state: Mutex::new(RecorderState::Idle),
            overlay_state: Mutex::new(OverlayPayload::idle()),
            agent_session: Mutex::new(Vec::new()),
            pending_tool_call: Mutex::new(None),
        }
    }
}
