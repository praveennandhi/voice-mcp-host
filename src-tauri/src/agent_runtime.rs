use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

use crate::agent::{self, AgentOutputMode, AgentRequest, AgentResult, AgentSessionTurn, ToolCall};
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
    if let Some(result) = maybe_resolve_pending(state, cfg, command)? {
        return Ok(result);
    }

    let history = state.agent_session.lock().unwrap().clone();
    let workspace_context = workspace::context(&cfg.workspace);
    let mut result = agent::run_agent(&cfg.agent, AgentRequest {
        command,
        selected_text,
        target_app,
        history: &history,
        workspace_context: Some(&workspace_context),
    })?;

    if result.mode != AgentOutputMode::Tool {
        append_turns(state, command, &result.text, mode_label(result.mode));
        return Ok(result);
    }

    let Some(tool) = result.tool_call.clone() else {
        bail!("agent requested tool mode without a tool call");
    };

    if workspace::requires_confirmation(&tool.name) {
        *state.pending_tool_call.lock().unwrap() = Some(PendingToolCall {
            user_command: command.trim().to_string(),
            tool: tool.clone(),
        });
        let text = confirmation_text(&tool);
        append_turns(state, command, &text, "confirm");
        return Ok(AgentResult {
            mode: AgentOutputMode::Speak,
            text,
            tool_call: None,
        });
    }

    let tool_result = workspace::execute(&cfg.workspace, &tool.name, &tool.args)?;
    let tool_history = with_tool_result(&history, command, &tool_result.summary, &tool_result.content);
    result = agent::run_agent(&cfg.agent, AgentRequest {
        command,
        selected_text,
        target_app,
        history: &tool_history,
        workspace_context: Some(&workspace_context),
    })?;
    append_turns(state, command, &result.text, mode_label(result.mode));
    Ok(result)
}

pub fn append_turns(state: &AppState, user: &str, assistant: &str, mode: &str) {
    let mut session = state.agent_session.lock().unwrap();
    session.push(AgentSessionTurn {
        role: "user".into(),
        content: user.trim().to_string(),
        mode: None,
    });
    session.push(AgentSessionTurn {
        role: "assistant".into(),
        content: assistant.trim().to_string(),
        mode: Some(mode.into()),
    });
    if session.len() > 40 {
        let remove_count = session.len() - 40;
        session.drain(0..remove_count);
    }
}

pub fn mode_label(mode: AgentOutputMode) -> &'static str {
    match mode {
        AgentOutputMode::Insert => "insert",
        AgentOutputMode::Speak => "speak",
        AgentOutputMode::Tool => "tool",
    }
}

fn maybe_resolve_pending(state: &AppState, cfg: &Config, command: &str) -> Result<Option<AgentResult>> {
    let answer = command.trim().to_ascii_lowercase();
    if !matches!(answer.as_str(), "yes" | "yeah" | "yep" | "confirm" | "do it" | "go ahead" | "no" | "nope" | "cancel") {
        return Ok(None);
    }

    let pending = state.pending_tool_call.lock().unwrap().take();
    let Some(pending) = pending else {
        return Ok(None);
    };

    if matches!(answer.as_str(), "no" | "nope" | "cancel") {
        let text = "Cancelled.".to_string();
        append_turns(state, command, &text, "speak");
        return Ok(Some(AgentResult {
            mode: AgentOutputMode::Speak,
            text,
            tool_call: None,
        }));
    }

    let tool_result = workspace::execute(&cfg.workspace, &pending.tool.name, &pending.tool.args)?;
    let text = tool_result.summary;
    append_turns(state, command, &text, "tool");
    Ok(Some(AgentResult {
        mode: AgentOutputMode::Speak,
        text,
        tool_call: None,
    }))
}

fn confirmation_text(tool: &ToolCall) -> String {
    let path = tool.args.get("path").and_then(|v| v.as_str()).unwrap_or("(missing path)");
    match tool.name.as_str() {
        "workspace.create_note" => format!("Create a new note at `{path}` in your workspace?"),
        "workspace.append_note" => format!("Append to `{path}` in your workspace?"),
        _ => format!("Run `{}` in your workspace?", tool.name),
    }
}

fn with_tool_result(
    history: &[AgentSessionTurn],
    command: &str,
    summary: &str,
    content: &str,
) -> Vec<AgentSessionTurn> {
    let mut next = history.to_vec();
    next.push(AgentSessionTurn {
        role: "user".into(),
        content: command.trim().to_string(),
        mode: None,
    });
    next.push(AgentSessionTurn {
        role: "tool".into(),
        content: format!("{summary}\n\n{content}"),
        mode: Some("workspace".into()),
    });
    next
}
