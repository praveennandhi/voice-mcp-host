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
    if let Some(tool) = prepare_direct_workspace_note_write(cfg, command, &history)? {
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

    let workspace_context = workspace::context(&cfg.workspace);
    let mut result = agent::run_agent(&cfg.agent, AgentRequest {
        command,
        selected_text,
        target_app,
        history: &history,
        workspace_context: Some(&workspace_context),
    })?;

    if result.mode == AgentOutputMode::Insert {
        if let Some(tool) = coerce_workspace_note_write(cfg, command, &result.text) {
            if let Err(e) = workspace::validate_tool_args(&tool.name, &tool.args) {
                let text = format!("I could not prepare that workspace action: {e}. Please include the file name and content.");
                append_turns(state, command, &text, "speak");
                return Ok(AgentResult {
                    mode: AgentOutputMode::Speak,
                    text,
                    tool_call: None,
                });
            }
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
    }

    if result.mode != AgentOutputMode::Tool {
        append_turns(state, command, &result.text, mode_label(result.mode));
        return Ok(result);
    }

    let Some(tool) = result.tool_call.clone() else {
        bail!("agent requested tool mode without a tool call");
    };
    let tool = normalize_workspace_tool(cfg, tool);
    if let Err(e) = workspace::validate_tool_args(&tool.name, &tool.args) {
        let text = format!("I could not prepare that workspace action: {e}. Please include the file name and content.");
        append_turns(state, command, &text, "speak");
        return Ok(AgentResult {
            mode: AgentOutputMode::Speak,
            text,
            tool_call: None,
        });
    }

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
    if state.pending_tool_call.lock().unwrap().is_none() {
        return Ok(None);
    }

    let answer = normalize_confirmation(command);
    if answer.is_empty() {
        let text = "Please say yes to confirm, or no to cancel.".to_string();
        append_turns(state, command, &text, "confirm");
        return Ok(Some(AgentResult {
            mode: AgentOutputMode::Speak,
            text,
            tool_call: None,
        }));
    }

    if !is_confirmation_answer(&answer) {
        let text = "Please say yes to confirm, or no to cancel.".to_string();
        append_turns(state, command, &text, "confirm");
        return Ok(Some(AgentResult {
            mode: AgentOutputMode::Speak,
            text,
            tool_call: None,
        }));
    }

    let pending = state.pending_tool_call.lock().unwrap().take();
    let Some(pending) = pending else {
        return Ok(None);
    };

    if is_negative_confirmation(&answer) {
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

fn confirmation_text(tool: &ToolCall) -> String {
    let path = tool.args.get("path").and_then(|v| v.as_str()).unwrap_or("(missing path)");
    match tool.name.as_str() {
        "workspace.save_note" => format!("Save this to `{path}` in your workspace?"),
        "workspace.create_note" => format!("Create a new note at `{path}` in your workspace?"),
        "workspace.append_note" => format!("Append to `{path}` in your workspace?"),
        _ => format!("Run `{}` in your workspace?", tool.name),
    }
}

fn normalize_workspace_tool(cfg: &Config, mut tool: ToolCall) -> ToolCall {
    if !matches!(tool.name.as_str(), "workspace.create_note" | "workspace.append_note") {
        return tool;
    }

    let Some(path) = tool.args.get("path").and_then(|v| v.as_str()) else {
        return tool;
    };

    let create_existing = tool.name == "workspace.create_note" && workspace::note_exists(&cfg.workspace, path);
    let append_missing = tool.name == "workspace.append_note" && !workspace::note_exists(&cfg.workspace, path);
    if create_existing || append_missing {
        tool.name = "workspace.save_note".into();
    }

    tool
}

fn prepare_direct_workspace_note_write(
    cfg: &Config,
    command: &str,
    history: &[AgentSessionTurn],
) -> Result<Option<ToolCall>> {
    if !cfg.workspace.enabled {
        return Ok(None);
    }

    let lower = command.to_ascii_lowercase();
    let asks_for_note = mentions_workspace_note(&lower);
    let asks_for_write = lower.contains("create")
        || lower.contains("save")
        || lower.contains("write")
        || lower.contains("put")
        || lower.contains("add");

    if !(asks_for_note && asks_for_write) {
        return Ok(None);
    }

    let Some(path) = infer_workspace_note_path(cfg, command) else {
        return Ok(None);
    };
    let content = agent::draft_workspace_note(&cfg.agent, command, &path, history)?;
    Ok(Some(ToolCall {
        name: preferred_note_write_tool(cfg, &path, &lower).into(),
        args: serde_json::json!({
            "path": path,
            "content": content.trim(),
        }),
    }))
}

fn coerce_workspace_note_write(cfg: &Config, command: &str, content: &str) -> Option<ToolCall> {
    let lower = command.to_ascii_lowercase();
    let asks_for_note = mentions_workspace_note(&lower);
    let asks_for_write = lower.contains("create")
        || lower.contains("save")
        || lower.contains("write")
        || lower.contains("put")
        || lower.contains("add");

    if !(asks_for_note && asks_for_write) {
        return None;
    }

    let path = infer_workspace_note_path(cfg, command)?;
    Some(ToolCall {
        name: "workspace.save_note".into(),
        args: serde_json::json!({
            "path": path,
            "content": content.trim(),
        }),
    })
}

fn preferred_note_write_tool(cfg: &Config, path: &str, lower_command: &str) -> &'static str {
    let explicit_new = lower_command.contains("new note")
        || lower_command.contains("new file")
        || lower_command.contains("separate note")
        || lower_command.contains("separate file");
    if explicit_new && !workspace::note_exists(&cfg.workspace, path) {
        "workspace.create_note"
    } else {
        "workspace.save_note"
    }
}

fn mentions_workspace_note(lower_command: &str) -> bool {
    lower_command.contains("note")
        || lower_command.contains(".md")
        || lower_command.contains("markdown")
        || lower_command.contains("todo")
        || lower_command.contains("to do")
        || lower_command.contains("task")
        || lower_command.contains("tasks")
        || lower_command.contains("idea")
        || lower_command.contains("ideas")
}

fn infer_workspace_note_path(cfg: &Config, command: &str) -> Option<String> {
    if let Some(path) = extract_markdown_path(command) {
        return Some(path);
    }

    let lower = command.to_ascii_lowercase();
    if lower.contains("todo") || lower.contains("to do") || lower.contains("task") {
        return Some(first_existing_note(cfg, &["todo.md", "to-do.md", "tasks.md", "task.md"]).unwrap_or("todo.md").to_string());
    }
    if lower.contains("idea") {
        return Some(first_existing_note(cfg, &["ideas.md", "idea.md", "notes.md"]).unwrap_or("ideas.md").to_string());
    }
    if lower.contains("note") || lower.contains("notes") {
        return Some(first_existing_note(cfg, &["notes.md", "note.md"]).unwrap_or("notes.md").to_string());
    }

    None
}

fn first_existing_note(cfg: &Config, candidates: &[&'static str]) -> Option<&'static str> {
    candidates
        .iter()
        .copied()
        .find(|path| workspace::note_exists(&cfg.workspace, path))
}

fn extract_markdown_path(command: &str) -> Option<String> {
    for raw in command.split_whitespace() {
        let cleaned = raw.trim_matches(|c: char| {
            c == '"' || c == '\'' || c == '`' || c == ',' || c == '.' || c == ':' || c == ';'
        });
        if cleaned.to_ascii_lowercase().ends_with(".md") {
            return Some(cleaned.replace('\\', "/"));
        }
    }
    None
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
