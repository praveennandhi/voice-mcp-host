use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

use crate::agent_provider;
use crate::agent_memory::{append_turns, with_tool_result};
use crate::agent_tools;
use crate::agent_types::{AgentOutputMode, AgentRequest, AgentResult, ToolCall};
use crate::app_state::AppState;
use crate::config::Config;
use crate::workspace;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingToolCall {
    pub user_command: String,
    pub tool: ToolCall,
}

pub fn run(
    state: &AppState,
    cfg: &Config,
    command: &str,
    selected_text: Option<&str>,
    target_app: &str,
) -> Result<AgentResult> {
    let mut command = command.trim().to_string();
    if let Some(pending_result) = maybe_resolve_pending(state, cfg, &command)? {
        match pending_result {
            PendingResolution::Handled(result) => return Ok(result),
            PendingResolution::Revised(revised_command) => {
                command = revised_command;
            }
        }
    }

    let history = state.agent_session.lock().unwrap().clone();
    if let Some(tool) = agent_tools::prepare_direct_workspace_note_write(cfg, &command, &history)? {
        *state.pending_tool_call.lock().unwrap() = Some(PendingToolCall {
            user_command: command.clone(),
            tool: tool.clone(),
        });
        let text = agent_tools::confirmation_text(&tool);
        append_turns(state, &command, &text, "confirm");
        return Ok(AgentResult {
            mode: AgentOutputMode::Speak,
            text,
            tool_call: None,
        });
    }

    let workspace_context = workspace::context(&cfg.workspace);
    let mut result = agent_provider::run_agent(&cfg.agent, AgentRequest {
        command: &command,
        selected_text,
        target_app,
        history: &history,
        workspace_context: Some(&workspace_context),
    })?;

    if result.mode == AgentOutputMode::Insert {
        if let Some(tool) = agent_tools::coerce_workspace_note_write(cfg, &command, &result.text) {
            if let Err(e) = workspace::validate_tool_args(&tool.name, &tool.args) {
                let text = format!("I could not prepare that workspace action: {e}. Please include the file name and content.");
                append_turns(state, &command, &text, "speak");
                return Ok(AgentResult {
                    mode: AgentOutputMode::Speak,
                    text,
                    tool_call: None,
                });
            }
            *state.pending_tool_call.lock().unwrap() = Some(PendingToolCall {
                user_command: command.clone(),
                tool: tool.clone(),
            });
            let text = agent_tools::confirmation_text(&tool);
            append_turns(state, &command, &text, "confirm");
            return Ok(AgentResult {
                mode: AgentOutputMode::Speak,
                text,
                tool_call: None,
            });
        }
    }

    if result.mode != AgentOutputMode::Tool {
        append_turns(state, &command, &result.text, mode_label(result.mode));
        return Ok(result);
    }

    let Some(tool) = result.tool_call.clone() else {
        bail!("agent requested tool mode without a tool call");
    };
    let tool = agent_tools::normalize_workspace_tool(cfg, tool);
    if let Err(e) = workspace::validate_tool_args(&tool.name, &tool.args) {
        let text = format!("I could not prepare that workspace action: {e}. Please include the file name and content.");
        append_turns(state, &command, &text, "speak");
        return Ok(AgentResult {
            mode: AgentOutputMode::Speak,
            text,
            tool_call: None,
        });
    }

    if workspace::requires_confirmation(&tool.name) {
        *state.pending_tool_call.lock().unwrap() = Some(PendingToolCall {
            user_command: command.clone(),
            tool: tool.clone(),
        });
        let text = agent_tools::confirmation_text(&tool);
        append_turns(state, &command, &text, "confirm");
        return Ok(AgentResult {
            mode: AgentOutputMode::Speak,
            text,
            tool_call: None,
        });
    }

    let tool_result = workspace::execute(&cfg.workspace, &tool.name, &tool.args)?;
    let tool_history = with_tool_result(&history, &command, &tool_result.summary, &tool_result.content);
    result = agent_provider::run_agent(&cfg.agent, AgentRequest {
        command: &command,
        selected_text,
        target_app,
        history: &tool_history,
        workspace_context: Some(&workspace_context),
    })?;
    append_turns(state, &command, &result.text, mode_label(result.mode));
    Ok(result)
}

pub fn mode_label(mode: AgentOutputMode) -> &'static str {
    match mode {
        AgentOutputMode::Insert => "insert",
        AgentOutputMode::Speak => "speak",
        AgentOutputMode::Tool => "tool",
    }
}

enum PendingResolution {
    Handled(AgentResult),
    Revised(String),
}

fn maybe_resolve_pending(state: &AppState, cfg: &Config, command: &str) -> Result<Option<PendingResolution>> {
    if state.pending_tool_call.lock().unwrap().is_none() {
        return Ok(None);
    }

    if let Some(revised_command) = extract_revised_instruction(command) {
        *state.pending_tool_call.lock().unwrap() = None;
        append_turns(state, command, "Okay, I will update that request.", "confirm");
        return Ok(Some(PendingResolution::Revised(revised_command)));
    }

    let answer = normalize_confirmation(command);
    if answer.is_empty() {
        let text = "Please say yes to confirm, or no to cancel.".to_string();
        append_turns(state, command, &text, "confirm");
        return Ok(Some(PendingResolution::Handled(AgentResult {
            mode: AgentOutputMode::Speak,
            text,
            tool_call: None,
        })));
    }

    if !is_confirmation_answer(&answer) {
        let text = "Please say yes to confirm, no to cancel, or say what you want changed.".to_string();
        append_turns(state, command, &text, "confirm");
        return Ok(Some(PendingResolution::Handled(AgentResult {
            mode: AgentOutputMode::Speak,
            text,
            tool_call: None,
        })));
    }

    let pending = state.pending_tool_call.lock().unwrap().take();
    let Some(pending) = pending else {
        return Ok(None);
    };

    if is_negative_confirmation(&answer) {
        let text = "Cancelled.".to_string();
        append_turns(state, command, &text, "speak");
        return Ok(Some(PendingResolution::Handled(AgentResult {
            mode: AgentOutputMode::Speak,
            text,
            tool_call: None,
        })));
    }

    let tool_result = workspace::execute(&cfg.workspace, &pending.tool.name, &pending.tool.args)?;
    let text = tool_result.summary;
    append_turns(state, command, &text, "tool");
    Ok(Some(PendingResolution::Handled(AgentResult {
        mode: AgentOutputMode::Speak,
        text,
        tool_call: None,
    })))
}

fn extract_revised_instruction(command: &str) -> Option<String> {
    let text = command.trim();
    if text.is_empty() {
        return None;
    }

    let normalized = normalize_confirmation(text);
    let revision_starters = [
        "no i need",
        "no use",
        "no make",
        "no create",
        "no save",
        "no add",
        "no put",
        "no change",
        "not that",
        "different file",
        "use a different",
        "use another",
        "actually",
        "instead",
    ];
    if !revision_starters.iter().any(|starter| normalized.contains(starter)) {
        return None;
    }

    let mut revised = text
        .trim_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace())
        .to_string();
    for prefix in ["agent", "no", "nope", "cancel", "actually"] {
        let lower = revised.to_ascii_lowercase();
        if lower == prefix {
            return None;
        }
        if let Some(rest) = lower.strip_prefix(&format!("{prefix} ")) {
            let offset = revised.len() - rest.len();
            revised = revised[offset..].trim().to_string();
        }
    }

    if revised.is_empty() {
        None
    } else if revised.to_ascii_lowercase().starts_with("i need ") {
        Some(format!("Create {}", revised))
    } else if revised.to_ascii_lowercase().starts_with("use ") {
        Some(format!("Save this using {}", revised))
    } else {
        Some(revised)
    }
}

fn normalize_confirmation(command: &str) -> String {
    let text = command
        .trim()
        .to_ascii_lowercase()
        .trim_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace())
        .to_string();

    let mut words = text
        .split(|c: char| c.is_ascii_punctuation() || c.is_whitespace())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    if let Some(rest) = words.strip_prefix("agent ") {
        words = rest.trim().to_string();
    } else if words == "agent" {
        words.clear();
    }

    words
}

fn is_confirmation_answer(answer: &str) -> bool {
    matches!(
        answer,
        "yes"
            | "yes please"
            | "yeah"
            | "yep"
            | "confirm"
            | "do it"
            | "go ahead"
            | "please do it"
            | "no"
            | "nope"
            | "cancel"
    )
}

fn is_negative_confirmation(answer: &str) -> bool {
    matches!(answer, "no" | "nope" | "cancel")
}
