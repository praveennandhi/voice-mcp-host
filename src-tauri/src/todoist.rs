use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::Duration;

use crate::config::TodoistConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoistToolResult {
    pub tool: String,
    pub summary: String,
    pub content: String,
}

pub fn context(config: &TodoistConfig) -> String {
    if !config.enabled {
        return "Todoist connector is disabled.".into();
    }
    if token(config).is_none() {
        return "Todoist connector is enabled, but no API token is configured.".into();
    }
    "Todoist connector is available. Tool: todoist.create_task. Writes require confirmation.".into()
}

pub fn validate_tool_args(name: &str, args: &serde_json::Value) -> Result<()> {
    match name {
        "todoist.create_task" => {
            required_string(args, "content")?;
            optional_string(args, "description")?;
            optional_string(args, "due_string")?;
            Ok(())
        }
        other => bail!("unknown Todoist tool: {other}"),
    }
}

pub fn requires_confirmation(name: &str) -> bool {
    matches!(name, "todoist.create_task")
}

pub fn confirmation_text(tool: &str, args: &serde_json::Value) -> String {
    match tool {
        "todoist.create_task" => {
            let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("(missing task)");
            let due = args.get("due_string").and_then(|v| v.as_str()).unwrap_or("");
            if due.trim().is_empty() {
                format!("Create Todoist task `{content}`?")
            } else {
                format!("Create Todoist task `{content}` due {due}?")
            }
        }
        _ => format!("Run `{tool}`?"),
    }
}

pub fn execute(config: &TodoistConfig, name: &str, args: &serde_json::Value) -> Result<TodoistToolResult> {
    validate_tool_args(name, args)?;
    match name {
        "todoist.create_task" => create_task(config, args),
        other => bail!("unknown Todoist tool: {other}"),
    }
}

fn create_task(config: &TodoistConfig, args: &serde_json::Value) -> Result<TodoistToolResult> {
    if !config.enabled {
        bail!("Todoist connector is disabled");
    }
    let api_token = token(config).context("Todoist API token is not configured")?;
    let content = required_string(args, "content")?;
    let description = optional_string(args, "description")?;
    let due_string = optional_string(args, "due_string")?;

    let mut body = json!({ "content": content });
    if let Some(description) = description {
        body["description"] = json!(description);
    }
    if let Some(due_string) = due_string {
        body["due_string"] = json!(due_string);
    }

    let response = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .context("failed to build Todoist HTTP client")?
        .post("https://api.todoist.com/api/v1/tasks")
        .bearer_auth(api_token)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(body.to_string())
        .send()
        .context("Todoist create task request failed")?;

    let status = response.status();
    let response_text = response.text().unwrap_or_default();
    if !status.is_success() {
        bail!("Todoist returned HTTP {status}: {response_text}");
    }

    Ok(TodoistToolResult {
        tool: "todoist.create_task".into(),
        summary: format!("Created Todoist task: {content}."),
        content: response_text,
    })
}

fn token(config: &TodoistConfig) -> Option<String> {
    config
        .api_token
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(str::trim)
        .map(str::to_string)
}

fn required_string<'a>(args: &'a serde_json::Value, key: &str) -> Result<&'a str> {
    args.get(key)
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .with_context(|| format!("missing required string argument: {key}"))
}

fn optional_string<'a>(args: &'a serde_json::Value, key: &str) -> Result<Option<&'a str>> {
    Ok(args
        .get(key)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty()))
}
