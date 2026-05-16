use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

use crate::config::WorkspaceConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceStatus {
    pub enabled: bool,
    pub configured: bool,
    pub folder_path: Option<String>,
    pub exists: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceToolResult {
    pub tool: String,
    pub summary: String,
    pub content: String,
}

pub fn status(config: &WorkspaceConfig) -> WorkspaceStatus {
    let root = root_path(config).ok();
    WorkspaceStatus {
        enabled: config.enabled,
        configured: config.folder_path.as_ref().is_some_and(|p| !p.trim().is_empty()),
        folder_path: config.folder_path.clone(),
        exists: root.as_ref().is_some_and(|p| p.is_dir()),
    }
}

pub fn context(config: &WorkspaceConfig) -> String {
    let st = status(config);
    if !st.enabled {
        return "Workspace Notes skill is disabled.".into();
    }
    if !st.configured {
        return "Workspace Notes skill is enabled, but no folder path is configured.".into();
    }
    if !st.exists {
        return format!(
            "Workspace Notes skill is enabled, but the folder does not exist: {}",
            st.folder_path.unwrap_or_default()
        );
    }

    "Workspace Notes skill is available. Tools: workspace.list_files, workspace.read_file, workspace.search_files, workspace.create_note, workspace.append_note. Writes require confirmation. Paths must be relative and stay inside the configured workspace.".into()
}

pub fn execute(config: &WorkspaceConfig, name: &str, args: &serde_json::Value) -> Result<WorkspaceToolResult> {
    match name {
        "workspace.list_files" => list_files(config),
        "workspace.read_file" => {
            let path = required_string(args, "path")?;
            read_file(config, path)
        }
        "workspace.search_files" => {
            let query = required_string(args, "query")?;
            search_files(config, query)
        }
        "workspace.create_note" => {
            let path = required_string(args, "path")?;
            let content = required_string(args, "content")?;
            write_file(config, path, content, false)
        }
        "workspace.append_note" => {
            let path = required_string(args, "path")?;
            let content = required_string(args, "content")?;
            write_file(config, path, content, true)
        }
        other => bail!("unknown workspace tool: {other}"),
    }
}

pub fn requires_confirmation(name: &str) -> bool {
    matches!(name, "workspace.create_note" | "workspace.append_note")
}

fn list_files(config: &WorkspaceConfig) -> Result<WorkspaceToolResult> {
    let root = root_path(config)?;
    let mut files = Vec::new();
    collect_text_files(&root, &root, &mut files, 80)?;
    let content = if files.is_empty() {
        "No Markdown or text files found.".into()
    } else {
        files.join("\n")
    };
    Ok(WorkspaceToolResult {
        tool: "workspace.list_files".into(),
        summary: format!("Found {} text files.", files.len()),
        content,
    })
}

fn read_file(config: &WorkspaceConfig, rel_path: &str) -> Result<WorkspaceToolResult> {
    let path = safe_path(config, rel_path)?;
    ensure_text_file(&path)?;
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", rel_path))?;
    Ok(WorkspaceToolResult {
        tool: "workspace.read_file".into(),
        summary: format!("Read {rel_path}."),
        content: truncate(content, 16_000),
    })
}

fn search_files(config: &WorkspaceConfig, query: &str) -> Result<WorkspaceToolResult> {
    let root = root_path(config)?;
    let needle = query.to_ascii_lowercase();
    let mut files = Vec::new();
    collect_text_files(&root, &root, &mut files, 120)?;
    let mut matches = Vec::new();
    for rel in files {
        let path = safe_path(config, &rel)?;
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        for (idx, line) in content.lines().enumerate() {
            if line.to_ascii_lowercase().contains(&needle) {
                matches.push(format!("{}:{}: {}", rel, idx + 1, line.trim()));
                if matches.len() >= 40 {
                    break;
                }
            }
        }
        if matches.len() >= 40 {
            break;
        }
    }

    Ok(WorkspaceToolResult {
        tool: "workspace.search_files".into(),
        summary: format!("Found {} matches for \"{}\".", matches.len(), query),
        content: if matches.is_empty() { "No matches found.".into() } else { matches.join("\n") },
    })
}

fn write_file(config: &WorkspaceConfig, rel_path: &str, content: &str, append: bool) -> Result<WorkspaceToolResult> {
    let path = safe_path(config, rel_path)?;
    ensure_markdown_or_text_path(&path)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if append {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&path)?;
        if path.exists() && path.metadata()?.len() > 0 {
            writeln!(file)?;
        }
        write!(file, "{content}")?;
    } else {
        if path.exists() {
            bail!("file already exists: {rel_path}. Use append_note or choose another path.");
        }
        std::fs::write(&path, content)?;
    }
    Ok(WorkspaceToolResult {
        tool: if append { "workspace.append_note" } else { "workspace.create_note" }.into(),
        summary: if append { format!("Appended to {rel_path}.") } else { format!("Created {rel_path}.") },
        content: String::new(),
    })
}

fn root_path(config: &WorkspaceConfig) -> Result<PathBuf> {
    if !config.enabled {
        bail!("workspace is disabled");
    }
    let path = config
        .folder_path
        .as_deref()
        .filter(|p| !p.trim().is_empty())
        .context("workspace folder path is not configured")?;
    let root = expand_home(path);
    if !root.is_dir() {
        bail!("workspace folder does not exist: {}", root.display());
    }
    Ok(root.canonicalize()?)
}

fn safe_path(config: &WorkspaceConfig, rel_path: &str) -> Result<PathBuf> {
    let root = root_path(config)?;
    let rel = Path::new(rel_path);
    if rel.is_absolute() {
        bail!("workspace paths must be relative");
    }
    for component in rel.components() {
        if matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_)) {
            bail!("workspace path escapes the configured folder");
        }
    }
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        let canonical_parent = if parent.exists() {
            parent.canonicalize()?
        } else {
            let existing = nearest_existing_parent(parent)?;
            existing.canonicalize()?
        };
        if !canonical_parent.starts_with(&root) {
            bail!("workspace path escapes the configured folder");
        }
    }
    Ok(path)
}

fn nearest_existing_parent(path: &Path) -> Result<PathBuf> {
    let mut current = path;
    while !current.exists() {
        current = current.parent().context("path has no existing parent")?;
    }
    Ok(current.to_path_buf())
}

fn expand_home(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_else(|_| ".".into());
        PathBuf::from(home).join(rest)
    } else {
        PathBuf::from(path)
    }
}

fn collect_text_files(root: &Path, dir: &Path, out: &mut Vec<String>, limit: usize) -> Result<()> {
    if out.len() >= limit {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_text_files(root, &path, out, limit)?;
        } else if is_text_path(&path) {
            if let Ok(rel) = path.strip_prefix(root) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
        if out.len() >= limit {
            break;
        }
    }
    Ok(())
}

fn ensure_text_file(path: &Path) -> Result<()> {
    if !path.is_file() {
        bail!("file does not exist: {}", path.display());
    }
    ensure_markdown_or_text_path(path)
}

fn ensure_markdown_or_text_path(path: &Path) -> Result<()> {
    if is_text_path(path) {
        Ok(())
    } else {
        bail!("workspace v1 only supports .md, .markdown, .txt, and .text files")
    }
}

fn is_text_path(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref(),
        Some("md" | "markdown" | "txt" | "text")
    )
}

fn required_string<'a>(args: &'a serde_json::Value, key: &str) -> Result<&'a str> {
    args.get(key)
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .with_context(|| format!("missing required string argument: {key}"))
}

fn truncate(mut content: String, limit: usize) -> String {
    if content.len() > limit {
        content.truncate(limit);
        content.push_str("\n...[truncated]");
    }
    content
}
