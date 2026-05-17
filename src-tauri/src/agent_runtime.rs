use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

use crate::agent_provider;
use crate::agent_memory::{append_turns, with_tool_result};
use crate::agent_tools;
use crate::agent_types::{AgentOutputMode, AgentRequest, AgentResult, ToolCall};
use crate::app_state::AppState;
use crate::config::Config;
use crate::todoist;
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
    if let Some(tool) = direct_todoist_create(cfg, &command) {
        *state.pending_tool_call.lock().unwrap() = Some(PendingToolCall {
            user_command: command.clone(),
            tool: tool.clone(),
        });
        let text = confirmation_text(&tool);
        append_turns(state, &command, &text, "confirm");
        return Ok(AgentResult {
            mode: AgentOutputMode::Speak,
            text,
            tool_call: None,
        });
    }
    if let Some(tool) = direct_todoist_followup(state, cfg, &command) {
        *state.pending_tool_call.lock().unwrap() = Some(PendingToolCall {
            user_command: command.clone(),
            tool: tool.clone(),
        });
        let text = confirmation_text(&tool);
        append_turns(state, &command, &text, "confirm");
        return Ok(AgentResult {
            mode: AgentOutputMode::Speak,
            text,
            tool_call: None,
        });
    }
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

    let tool_context = tool_context(state, cfg);
    let mut result = agent_provider::run_agent(&cfg.agent, AgentRequest {
        command: &command,
        selected_text,
        target_app,
        history: &history,
        workspace_context: Some(&tool_context),
    })?;

    if result.mode == AgentOutputMode::Insert {
        if let Some(tool) = agent_tools::coerce_workspace_note_write(cfg, &command, &result.text) {
            if let Err(e) = validate_tool_args(cfg, &tool) {
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
            let text = confirmation_text(&tool);
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
    if let Err(e) = validate_tool_args(cfg, &tool) {
        let text = format!("I could not prepare that workspace action: {e}. Please include the file name and content.");
        append_turns(state, &command, &text, "speak");
        return Ok(AgentResult {
            mode: AgentOutputMode::Speak,
            text,
            tool_call: None,
        });
    }

    if requires_confirmation(&tool.name) {
        *state.pending_tool_call.lock().unwrap() = Some(PendingToolCall {
            user_command: command.clone(),
            tool: tool.clone(),
        });
        let text = confirmation_text(&tool);
        append_turns(state, &command, &text, "confirm");
        return Ok(AgentResult {
            mode: AgentOutputMode::Speak,
            text,
            tool_call: None,
        });
    }

    let tool_result = execute_tool(cfg, &tool)?;
    apply_tool_side_effects(state, &tool_result);
    let tool_history = with_tool_result(&history, &command, &tool_result.summary, &tool_result.content);
    result = agent_provider::run_agent(&cfg.agent, AgentRequest {
        command: &command,
        selected_text,
        target_app,
        history: &tool_history,
        workspace_context: Some(&tool_context),
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

    let tool_result = execute_tool(cfg, &pending.tool)?;
    apply_tool_side_effects(state, &tool_result);
    let text = tool_result.summary;
    append_turns(state, command, &text, "tool");
    Ok(Some(PendingResolution::Handled(AgentResult {
        mode: AgentOutputMode::Speak,
        text,
        tool_call: None,
    })))
}

struct RuntimeToolResult {
    summary: String,
    content: String,
    last_todoist_task: Option<todoist::TodoistTaskRef>,
    clear_last_todoist_task: bool,
}

fn tool_context(state: &AppState, cfg: &Config) -> String {
    let last_task = state.last_todoist_task.lock().unwrap().clone();
    let todoist_context = if let Some(task) = last_task {
        format!(
            "{}\nLast Todoist task: id={}, content={}",
            todoist::context(&cfg.connectors.todoist),
            task.id,
            task.content
        )
    } else {
        format!("{}\nLast Todoist task: none", todoist::context(&cfg.connectors.todoist))
    };
    format!(
        "{}\n\n{}",
        workspace::context(&cfg.workspace),
        todoist_context
    )
}

fn validate_tool_args(_cfg: &Config, tool: &ToolCall) -> Result<()> {
    if tool.name.starts_with("workspace.") {
        workspace::validate_tool_args(&tool.name, &tool.args)
    } else if tool.name.starts_with("todoist.") {
        todoist::validate_tool_args(&tool.name, &tool.args)
    } else {
        bail!("unknown tool: {}", tool.name)
    }
}

fn requires_confirmation(name: &str) -> bool {
    workspace::requires_confirmation(name) || todoist::requires_confirmation(name)
}

fn confirmation_text(tool: &ToolCall) -> String {
    if tool.name.starts_with("todoist.") {
        todoist::confirmation_text(&tool.name, &tool.args)
    } else {
        agent_tools::confirmation_text(tool)
    }
}

fn direct_todoist_create(cfg: &Config, command: &str) -> Option<ToolCall> {
    if !cfg.connectors.todoist.enabled {
        return None;
    }

    let lower = command.to_ascii_lowercase();
    let mentions_todoist = lower.contains("todoist") || lower.contains("to do ist");
    let mentions_task = lower.contains("task") || lower.contains("ask");
    let asks_create = lower.contains("create")
        || lower.contains("add")
        || lower.contains("save")
        || lower.contains("put");
    if !(mentions_todoist && mentions_task && asks_create) {
        return None;
    }

    let (content, due_string) = parse_todoist_task(command)?;
    Some(ToolCall {
        name: "todoist.create_task".into(),
        args: serde_json::json!({
            "content": content,
            "due_string": due_string,
        }),
    })
}

fn direct_todoist_followup(state: &AppState, cfg: &Config, command: &str) -> Option<ToolCall> {
    if !cfg.connectors.todoist.enabled {
        return None;
    }
    let lower = command.to_ascii_lowercase();
    let asks_complete = lower.contains("complete")
        || lower.contains("mark done")
        || lower.contains("mark it done")
        || lower.contains("check off")
        || lower.contains("close the task")
        || lower.contains("finish the task");
    let refers_last = lower.contains("task you created")
        || lower.contains("the task")
        || lower.contains("that task")
        || lower.contains("it");
    if !(asks_complete && refers_last) {
        return None;
    }

    let task = state.last_todoist_task.lock().unwrap().clone()?;
    Some(ToolCall {
        name: "todoist.complete_task".into(),
        args: serde_json::json!({
            "task_id": task.id,
            "content": task.content,
        }),
    })
}

fn parse_todoist_task(command: &str) -> Option<(String, Option<String>)> {
    let mut text = command
        .trim()
        .trim_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace())
        .to_string();
    text = strip_leading_word(text, "agent");
    text = strip_leading_word(text, "please");

    let lower = text.to_ascii_lowercase();
    let task_pos = lower.find("task").or_else(|| lower.find("ask"))?;
    let mut remainder = text[task_pos..].to_string();
    if let Some(pos) = remainder.to_ascii_lowercase().find(" to ") {
        remainder = remainder[pos + 4..].to_string();
    } else {
        remainder = strip_leading_word(remainder, "task");
        remainder = strip_leading_word(remainder, "ask");
    }

    for prefix in ["called", "named", "for", "about", "to"] {
        remainder = strip_leading_word(remainder, prefix);
    }

    let (content, due_string) = split_todoist_due(&remainder);
    let content = content
        .trim()
        .trim_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace())
        .to_string();
    if content.is_empty() {
        None
    } else {
        Some((content, due_string))
    }
}

fn split_todoist_due(text: &str) -> (String, Option<String>) {
    let lower = text.to_ascii_lowercase();
    let markers = [
        " tomorrow",
        " today",
        " tonight",
        " next ",
        " on monday",
        " on tuesday",
        " on wednesday",
        " on thursday",
        " on friday",
        " on saturday",
        " on sunday",
    ];

    let due_start = markers
        .iter()
        .filter_map(|marker| lower.find(marker))
        .min();

    if let Some(pos) = due_start {
        let content = text[..pos].trim().to_string();
        let due = text[pos..].trim().to_string();
        (content, if due.is_empty() { None } else { Some(due) })
    } else {
        (text.trim().to_string(), None)
    }
}

fn strip_leading_word(text: String, word: &str) -> String {
    let trimmed = text.trim();
    let lower = trimmed.to_ascii_lowercase();
    if lower == word {
        String::new()
    } else if let Some(rest) = lower.strip_prefix(&format!("{word} ")) {
        let offset = trimmed.len() - rest.len();
        trimmed[offset..].trim().to_string()
    } else {
        trimmed.to_string()
    }
}

fn execute_tool(cfg: &Config, tool: &ToolCall) -> Result<RuntimeToolResult> {
    if tool.name.starts_with("workspace.") {
        let result = workspace::execute(&cfg.workspace, &tool.name, &tool.args)?;
        Ok(RuntimeToolResult {
            summary: result.summary,
            content: result.content,
            last_todoist_task: None,
            clear_last_todoist_task: false,
        })
    } else if tool.name.starts_with("todoist.") {
        let clear_last_todoist_task = tool.name == "todoist.complete_task";
        let result = todoist::execute(&cfg.connectors.todoist, &tool.name, &tool.args)?;
        Ok(RuntimeToolResult {
            summary: result.summary,
            content: result.content,
            last_todoist_task: result.task,
            clear_last_todoist_task,
        })
    } else {
        bail!("unknown tool: {}", tool.name)
    }
}

fn apply_tool_side_effects(state: &AppState, result: &RuntimeToolResult) {
    if result.clear_last_todoist_task {
        *state.last_todoist_task.lock().unwrap() = None;
    }
    if let Some(task) = result.last_todoist_task.clone() {
        *state.last_todoist_task.lock().unwrap() = Some(task);
    }
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
