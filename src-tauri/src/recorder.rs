use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager};
use crate::app_state::{AppState, OverlayPayload, RecorderState};
use crate::audio::AudioCapture;
use crate::insertion;
use crate::logging;
use crate::platform::{WindowTargetOps, platform};

pub fn start_recording(app: AppHandle) {
    let state = app.state::<AppState>();
    let cfg = state.config.lock().unwrap().clone();

    // Capture focus BEFORE recording — overlay must not steal focus.
    let target = platform().capture_foreground();
    *state.target_window.lock().unwrap() = Some(target.clone());

    logging::write_event("recording_started", Some(target.context_json()));

    match AudioCapture::new(
        cfg.audio.input_device_id.as_deref(),
        cfg.dictation.max_record_seconds,
    ) {
        Ok(capture) => {
            *state.audio.lock().unwrap() = Some(capture);
            set_state(&app, RecorderState::Recording);
            emit_overlay(&app, "recording", "Listening", "Press F3 again to stop", None);
        }
        Err(e) => {
            logging::write_event("recording_start_failed", Some(serde_json::json!({ "error": e.to_string() })));
            set_state(&app, RecorderState::Error(e.to_string()));
            emit_overlay(&app, "error", "Error", &e.to_string(), Some(7000));
        }
    }
}

pub fn stop_and_transcribe(app: AppHandle) {
    let state = app.state::<AppState>();
    let cfg = state.config.lock().unwrap().clone();

    let capture = state.audio.lock().unwrap().take();
    let Some(capture) = capture else {
        return;
    };

    let duration_ms = capture.duration_ms();
    set_state(&app, RecorderState::Transcribing);
    emit_overlay(&app, "transcribing", "Transcribing", "Turning speech into text", None);

    logging::write_event("recording_stopped", Some(serde_json::json!({
        "duration_ms": duration_ms,
    })));

    // Reject too-short recordings
    if duration_ms < cfg.dictation.min_record_ms {
        logging::write_event("recording_too_short", Some(serde_json::json!({ "duration_ms": duration_ms })));
        set_state(&app, RecorderState::Error("Recording too short".into()));
        emit_overlay(&app, "error", "Too short", "Hold the hotkey longer", Some(4000));
        return;
    }

    // Move heavy work to a background thread
    let target = state.target_window.lock().unwrap().clone();
    let app_clone = app.clone();

    std::thread::spawn(move || {
        let t_start = Instant::now();
        let audio = match capture.stop() {
            Ok(a) => a,
            Err(e) => {
                logging::write_event("audio_stop_failed", Some(serde_json::json!({ "error": e.to_string() })));
                set_state(&app_clone, RecorderState::Error(e.to_string()));
                emit_overlay(&app_clone, "error", "Error", &e.to_string(), Some(7000));
                return;
            }
        };

        logging::write_event("transcription_started", None);

        let state = app_clone.state::<AppState>();
        let transcriber = state.transcriber.lock().unwrap();

        let Some(ref transcriber) = *transcriber else {
            let msg = "Model not loaded — download a model in Settings first";
            set_state(&app_clone, RecorderState::Error(msg.into()));
            emit_overlay(&app_clone, "error", "No model", msg, Some(7000));
            return;
        };

        let cfg = state.config.lock().unwrap().clone();
        let lang = &cfg.dictation.language;

        match transcriber.transcribe(&audio, lang) {
            Ok(text) => {
                let transcribe_ms = t_start.elapsed().as_millis();
                let chars = text.len();

                if cfg.privacy.verbose_transcript_logging {
                    logging::write_event("transcription_completed", Some(serde_json::json!({
                        "chars": chars, "duration_ms": transcribe_ms, "text": text
                    })));
                } else {
                    logging::write_event("transcription_completed", Some(serde_json::json!({
                        "chars": chars, "duration_ms": transcribe_ms
                    })));
                }

                if text.trim().is_empty() {
                    set_state(&app_clone, RecorderState::Ready);
                    emit_overlay(&app_clone, "ready", "Nothing heard", "Try speaking louder", Some(4000));
                    return;
                }

                set_state(&app_clone, RecorderState::Pasting);
                emit_overlay(&app_clone, "pasting", "Inserting", "Sending text to your app", None);

                logging::write_event("paste_attempted", target.as_ref().map(|t| t.context_json()));

                if let Some(ref target) = target {
                    if let Err(e) = platform().focus_target(target) {
                        logging::write_event("target_focus_failed", Some(serde_json::json!({
                            "error": e,
                            "target": target.context_json(),
                        })));
                    }
                    std::thread::sleep(std::time::Duration::from_millis(80));
                }

                let fallback_path = config_dir_fallback();
                let report = insertion::paste_text(
                    &text,
                    cfg.insertion.paste_delay_ms,
                    cfg.insertion.restore_delay_ms,
                    fallback_path.as_ref(),
                );

                logging::write_event("paste_completed", Some(serde_json::to_value(&report).unwrap_or_default()));

                if report.paste_status == "success" {
                    set_state(&app_clone, RecorderState::Ready);
                    emit_overlay(&app_clone, "ready", "Inserted", "Text added", Some(1200));
                } else {
                    let msg = if report.error_message.is_empty() {
                        "Paste failed - transcript is on clipboard".into()
                    } else {
                        report.error_message.clone()
                    };
                    set_state(&app_clone, RecorderState::Error(msg.clone()));
                    emit_overlay(&app_clone, "error", "Paste failed", &msg, Some(7000));
                }
            }
            Err(e) => {
                logging::write_event("transcription_failed", Some(serde_json::json!({ "error": e.to_string() })));
                set_state(&app_clone, RecorderState::Error(e.to_string()));
                emit_overlay(&app_clone, "error", "Transcription failed", &e.to_string(), Some(7000));
            }
        }
    });
}

fn set_state(app: &AppHandle, s: RecorderState) {
    *app.state::<AppState>().recorder_state.lock().unwrap() = s;
}

fn emit_overlay(app: &AppHandle, state: &str, title: &str, subtitle: &str, hide_after_ms: Option<u64>) {
    let payload = OverlayPayload {
        state: state.into(),
        title: title.into(),
        subtitle: subtitle.into(),
        hide_after_ms,
    };
    *app.state::<AppState>().overlay_state.lock().unwrap() = payload.clone();

    if let Some(win) = app.get_webview_window("overlay") {
        let _ = win.show();
        let _ = win.emit("overlay-state", payload.clone());
        let win_retry = win.clone();
        let payload_retry = payload.clone();
        std::thread::spawn(move || {
            for _ in 0..6 {
                std::thread::sleep(std::time::Duration::from_millis(75));
                let _ = win_retry.emit("overlay-state", payload_retry.clone());
            }
        });
        // Critical: do NOT call set_focus() on the overlay.
        // Focusing it would cause paste to land in the overlay instead of the target app.
    }

    // Schedule auto-hide
    if let Some(delay_ms) = hide_after_ms {
        let app = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(delay_ms));
            if let Some(win) = app.get_webview_window("overlay") {
                let _ = win.hide();
            }
            // Return recorder to Idle after READY/ERROR timeout
            let state = app.state::<AppState>();
            let current = state.recorder_state.lock().unwrap().clone();
            if matches!(current, RecorderState::Ready | RecorderState::Error(_)) {
                *state.recorder_state.lock().unwrap() = RecorderState::Idle;
                *state.overlay_state.lock().unwrap() = OverlayPayload::idle();
            }
        });
    }
}

fn config_dir_fallback() -> Option<std::path::PathBuf> {
    #[cfg(windows)]
    {
        let appdata = std::env::var("APPDATA").ok()?;
        Some(std::path::PathBuf::from(appdata).join("voice-mcp-host").join("last_transcript.txt"))
    }
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME").ok()?;
        Some(std::path::PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("voice-mcp-host")
            .join("last_transcript.txt"))
    }
}
