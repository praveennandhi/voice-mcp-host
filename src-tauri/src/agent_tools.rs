use anyhow::Result;

use crate::agent_provider;
use crate::agent_types::{AgentSessionTurn, ToolCall};
use crate::config::Config;
use crate::workspace;

pub fn confirmation_text(tool: &ToolCall) -> String {
    let path = tool.args.get("path").and_then(|v| v.as_str()).unwrap_or("(missing path)");
    match tool.name.as_str() {
        "workspace.save_note" => format!("Save this to `{path}` in your workspace?"),
        "workspace.create_note" => format!("Create a new note at `{path}` in your workspace?"),
        "workspace.append_note" => format!("Append to `{path}` in your workspace?"),
        _ => format!("Run `{}` in your workspace?", tool.name),
    }
}

pub fn normalize_workspace_tool(cfg: &Config, mut tool: ToolCall) -> ToolCall {
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

pub fn prepare_direct_workspace_note_write(
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
    let content = agent_provider::draft_workspace_note(&cfg.agent, command, &path, history)?;
    Ok(Some(ToolCall {
        name: preferred_note_write_tool(cfg, &path, &lower).into(),
        args: serde_json::json!({
            "path": path,
            "content": content.trim(),
        }),
    }))
}

pub fn coerce_workspace_note_write(cfg: &Config, command: &str, content: &str) -> Option<ToolCall> {
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
        return Some(best_existing_note(cfg, &["to-do", "todo", "tasks", "task"]).unwrap_or_else(|| "todo.md".to_string()));
    }
    if lower.contains("idea") {
        return Some(best_existing_note(cfg, &["ideas", "idea", "notes"]).unwrap_or_else(|| "ideas.md".to_string()));
    }
    if lower.contains("note") || lower.contains("notes") {
        return Some(best_existing_note(cfg, &["notes", "note"]).unwrap_or_else(|| "notes.md".to_string()));
    }

    None
}

fn best_existing_note(cfg: &Config, aliases: &[&str]) -> Option<String> {
    let files = workspace::text_files(&cfg.workspace, 120).ok()?;
    for alias in aliases {
        if let Some(path) = files
            .iter()
            .find(|path| normalized_stem(path) == normalize_note_name(alias))
        {
            return Some(path.clone());
        }
    }

    for alias in aliases {
        if let Some(path) = files
            .iter()
            .find(|path| normalized_stem(path).contains(&normalize_note_name(alias)))
        {
            return Some(path.clone());
        }
    }

    None
}

fn normalized_stem(path: &str) -> String {
    let file_name = path
        .rsplit('/')
        .next()
        .unwrap_or(path)
        .trim_end_matches(".markdown")
        .trim_end_matches(".md")
        .trim_end_matches(".text")
        .trim_end_matches(".txt");
    normalize_note_name(file_name)
}

fn normalize_note_name(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

fn extract_markdown_path(command: &str) -> Option<String> {
    for raw in command.split_whitespace() {
        let cleaned = raw.trim_matches(|c: char| {
            c == '"' || c == '\'' || c == '`' || c == ',' || c == '.' || c == ':' || c == ';'
        });
        if cleaned.to_ascii_lowercase().ends_with(".md") {
            return Some(clean_markdown_path(cleaned));
        }
    }
    None
}

fn clean_markdown_path(path: &str) -> String {
    path
        .replace('\\', "/")
        .split('/')
        .map(clean_markdown_segment)
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

fn clean_markdown_segment(segment: &str) -> String {
    if !segment.to_ascii_lowercase().ends_with(".md") {
        return segment.to_string();
    }

    let stem = segment[..segment.len().saturating_sub(3)]
        .trim_matches(|c: char| !c.is_ascii_alphanumeric())
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");

    if stem.is_empty() {
        "note.md".to_string()
    } else {
        format!("{stem}.md")
    }
}
