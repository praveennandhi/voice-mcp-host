use tauri::{AppHandle, Manager, State, WebviewWindow};
use tauri_plugin_opener::OpenerExt;
use crate::agent_provider;
use crate::agent_runtime;
use crate::agent_types::{AgentOutputMode, AgentSessionTurn};
use crate::app_state::{AppState, OverlayPayload};
use crate::asr::Transcriber;
use crate::config::{self, Config};
use crate::hotkeys;
use crate::logging;
use crate::model_store;
use crate::platform::{PermissionsOps, PermissionsStatus, platform};
use crate::workspace::{self, WorkspaceStatus};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct AppStatus {
    pub model_downloaded: bool,
    pub engine_downloaded: bool,
    pub gpu_detected: bool,
    pub preferred_acceleration: String,
    pub active_acceleration: Option<String>,
    pub model_name: String,
    pub transcriber_ready: bool,
    pub recorder_state: String,
    pub permissions: PermissionsStatus,
    pub workspace: WorkspaceStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentChatResponse {
    pub messages: Vec<AgentSessionTurn>,
    pub mode: String,
    pub text: String,
}

#[tauri::command]
pub fn get_config(state: State<AppState>) -> Config {
    state.config.lock().unwrap().clone()
}

#[tauri::command]
pub fn save_config(app: AppHandle, state: State<AppState>, config: Config) -> Result<(), String> {
    let old_hotkey = state.config.lock().unwrap().dictation.primary_hotkey.clone();
    let new_hotkey = config.dictation.primary_hotkey.clone();

    config::save(&config).map_err(|e| e.to_string())?;
    *state.config.lock().unwrap() = config;
    if old_hotkey != new_hotkey {
        hotkeys::register(&app).map_err(|e| e.to_string())?;
    }
    load_transcriber(&app);
    Ok(())
}

#[tauri::command]
pub fn get_status(state: State<AppState>) -> AppStatus {
    let cfg = state.config.lock().unwrap().clone();
    let cache_dir = cfg.model_cache_dir();
    let model_downloaded = if cfg.asr.backend == "faster_whisper" {
        true
    } else {
        model_store::is_downloaded(&cache_dir, &cfg.asr.model_name)
    };
    let engine_status = model_store::engine_status(&cache_dir);
    let transcriber_ready = state.transcriber.lock().unwrap().is_some();
    let recorder_state = state.recorder_state.lock().unwrap().label().into();

    AppStatus {
        model_downloaded,
        engine_downloaded: engine_status.engine_downloaded,
        gpu_detected: engine_status.gpu_detected,
        preferred_acceleration: engine_status.preferred_acceleration,
        active_acceleration: engine_status.active_acceleration,
        model_name: cfg.asr.model_name.clone(),
        transcriber_ready,
        recorder_state,
        permissions: platform().check_permissions(),
        workspace: workspace::status(&cfg.workspace),
    }
}

#[tauri::command]
pub fn get_overlay_state(state: State<AppState>) -> OverlayPayload {
    state.overlay_state.lock().unwrap().clone()
}

#[tauri::command]
pub fn get_agent_session(state: State<AppState>) -> Vec<AgentSessionTurn> {
    state.agent_session.lock().unwrap().clone()
}

#[tauri::command]
pub fn clear_agent_session(state: State<AppState>) {
    state.agent_session.lock().unwrap().clear();
    *state.pending_tool_call.lock().unwrap() = None;
}

#[tauri::command]
pub async fn send_agent_chat(app: AppHandle, message: String) -> Result<AgentChatResponse, String> {
    let message = message.trim().to_string();
    if message.is_empty() {
        return Err("Message is empty".into());
    }

    let state = app.state::<AppState>();
    let cfg = state.config.lock().unwrap().clone();
    let message_for_agent = message.clone();
    let app_for_agent = app.clone();
    let cfg_for_agent = cfg.clone();

    let result = tokio::task::spawn_blocking(move || {
        let state = app_for_agent.state::<AppState>();
        agent_runtime::run(&state, &cfg_for_agent, &message_for_agent, None, "voice-mcp-host chat")
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;

    let mode = match result.mode {
        AgentOutputMode::Insert => "insert",
        AgentOutputMode::Speak => "speak",
        AgentOutputMode::Tool => "tool",
    }
    .to_string();

    let state = app.state::<AppState>();
    let messages = state.agent_session.lock().unwrap().clone();

    if result.mode == AgentOutputMode::Speak && cfg.agent.speak_responses {
        let agent_cfg = cfg.agent.clone();
        let spoken_text = result.text.clone();
        std::thread::spawn(move || {
            if let Err(e) = agent_provider::speak_response(&agent_cfg, &spoken_text) {
                logging::write_event("agent_tts_failed", Some(serde_json::json!({
                    "error": e.to_string(),
                    "source": "chat",
                })));
            } else {
                logging::write_event("agent_tts_completed", Some(serde_json::json!({
                    "chars": spoken_text.len(),
                    "source": "chat",
                })));
            }
        });
    }

    logging::write_event("agent_chat_completed", Some(serde_json::json!({
        "mode": mode,
        "chars": result.text.len(),
        "preview": text_preview(&result.text, 220),
    })));

    Ok(AgentChatResponse {
        messages,
        mode,
        text: result.text,
    })
}

fn text_preview(text: &str, max_chars: usize) -> String {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= max_chars {
        normalized
    } else {
        let mut preview = normalized
            .chars()
            .take(max_chars.saturating_sub(3))
            .collect::<String>();
        preview.push_str("...");
        preview
    }
}

#[tauri::command]
pub fn start_overlay_drag(window: WebviewWindow) -> Result<(), String> {
    window.start_dragging().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn download_model(app: AppHandle, model_name: String) -> Result<(), String> {
    let state = app.state::<AppState>();
    let cfg = state.config.lock().unwrap().clone();
    let cache_dir = cfg.model_cache_dir();
    let app_clone = app.clone();

    tokio::task::spawn_blocking(move || {
        model_store::download(&app_clone, &cache_dir, &model_name)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;

    // Try to load the transcriber now that the model is on disk
    load_transcriber(&app);
    Ok(())
}

#[tauri::command]
pub async fn download_engine(app: AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let cfg = state.config.lock().unwrap().clone();
    let cache_dir = cfg.model_cache_dir();
    let app_clone = app.clone();

    tokio::task::spawn_blocking(move || {
        model_store::download_engine(&app_clone, &cache_dir)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;

    load_transcriber(&app);
    Ok(())
}

#[tauri::command]
pub fn list_audio_devices() -> Vec<String> {
    crate::audio::list_devices()
}

#[tauri::command]
pub fn list_models(state: State<AppState>) -> Vec<model_store::AvailableModel> {
    let cfg = state.config.lock().unwrap().clone();
    model_store::list_models(&cfg.model_cache_dir())
}

#[tauri::command]
pub fn check_permissions() -> PermissionsStatus {
    platform().check_permissions()
}

#[tauri::command]
pub fn request_accessibility_permission() -> bool {
    platform().request_accessibility_permission()
}

#[tauri::command]
pub fn open_log_dir(app: AppHandle) -> Result<(), String> {
    let log_dir = logging::log_dir_path();
    std::fs::create_dir_all(&log_dir).map_err(|e| e.to_string())?;
    app.opener()
        .open_path(log_dir.to_string_lossy().as_ref(), None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_version() -> String {
    env!("CARGO_PKG_VERSION").into()
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}

/// Called at startup and after model download completes.
pub fn load_transcriber(app: &AppHandle) {
    let state = app.state::<AppState>();
    let cfg = state.config.lock().unwrap().clone();
    let cache_dir = cfg.model_cache_dir();
    let model_path = model_store::model_path(&cache_dir, &cfg.asr.model_name);

    if cfg.asr.backend != "faster_whisper"
        && (!model_path.exists() || !model_store::is_engine_downloaded(&cache_dir))
    {
        return;
    }

    match Transcriber::load(&cfg.asr, &cache_dir) {
        Ok(t) => {
            *state.transcriber.lock().unwrap() = Some(t);
            logging::write_event("transcriber_loaded", Some(serde_json::json!({
                "model": cfg.asr.model_name,
                "faster_whisper_model": cfg.asr.faster_whisper_model_name,
                "backend": cfg.asr.backend,
                "engine": model_store::engine_status(&cache_dir).active_acceleration
            })));
        }
        Err(e) => {
            logging::write_event("transcriber_load_failed", Some(serde_json::json!({
                "error": e.to_string()
            })));
            if state.transcriber.lock().unwrap().is_none() {
                logging::write_event("transcriber_unavailable", Some(serde_json::json!({
                    "backend": cfg.asr.backend
                })));
            }
        }
    }
}
