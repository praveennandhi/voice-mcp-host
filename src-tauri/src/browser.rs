use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use crate::config::BrowserConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserToolResult {
    pub tool: String,
    pub summary: String,
    pub content: String,
}

pub fn context(config: &BrowserConfig) -> String {
    if !config.enabled {
        return "Browser connector is disabled.".into();
    }
    if config.visible {
        "Browser connector is available in visible read-only mode. Tools: browser.open_url, browser.search_web, browser.extract_page_text. It opens searches/pages in the user's default browser for visibility, and can read public pages/search results, but it cannot click, type, log in, submit forms, or change websites.".into()
    } else {
        "Browser connector is available in read-only mode. Tools: browser.open_url, browser.search_web, browser.extract_page_text. It can read public pages and search results, but it cannot click, type, log in, submit forms, or change websites.".into()
    }
}

pub fn validate_tool_args(name: &str, args: &serde_json::Value) -> Result<()> {
    match name {
        "browser.open_url" | "browser.extract_page_text" => {
            let url = required_string(args, "url")?;
            validate_url(url)
        }
        "browser.search_web" => {
            required_string(args, "query")?;
            Ok(())
        }
        other => bail!("unknown browser tool: {other}"),
    }
}

pub fn requires_confirmation(_name: &str) -> bool {
    false
}

pub fn execute(config: &BrowserConfig, name: &str, args: &serde_json::Value) -> Result<BrowserToolResult> {
    if !config.enabled {
        bail!("Browser connector is disabled");
    }
    validate_tool_args(name, args)?;
    let visible_url = if config.visible {
        open_visible_target(name, args)?
    } else {
        None
    };

    let Some(script) = browser_script_path() else {
        return execute_http_fallback(name, args, visible_url);
    };
    let payload = serde_json::json!({
        "tool": name,
        "args": args,
    });

    let output = Command::new("node")
        .arg(&script)
        .arg(payload.to_string())
        .output()
        .with_context(|| format!("failed to start browser sidecar at {}", script.display()));

    let output = match output {
        Ok(output) => output,
        Err(_) => return execute_http_fallback(name, args, visible_url),
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        if stderr.contains("ERR_MODULE_NOT_FOUND") || stderr.contains("Cannot find package 'playwright'") {
            return execute_http_fallback(name, args, visible_url);
        }
        bail!("browser sidecar failed: {}{}", stderr.trim(), sidecar_stdout(&stdout));
    }

    let mut result: BrowserToolResult = serde_json::from_slice(&output.stdout)
        .context("browser sidecar returned invalid JSON")?;
    if let Some(visible_url) = visible_url {
        result.summary = format!("{} Opened visibly: {visible_url}", result.summary);
        result.content = attach_visible_url(&result.content, &visible_url);
    }
    Ok(result)
}

fn execute_http_fallback(name: &str, args: &serde_json::Value, visible_url: Option<String>) -> Result<BrowserToolResult> {
    match name {
        "browser.search_web" => search_web_http(required_string(args, "query")?, visible_url),
        "browser.open_url" | "browser.extract_page_text" => read_page_http(name, required_string(args, "url")?, visible_url),
        other => bail!("unknown browser tool: {other}"),
    }
}

fn search_web_http(query: &str, visible_url: Option<String>) -> Result<BrowserToolResult> {
    let url = format!("https://www.bing.com/search?format=rss&q={}", url_encode(query));
    let body = http_get(&url)?;
    let items = extract_rss_items(&body);
    Ok(BrowserToolResult {
        tool: "browser.search_web".into(),
        summary: if let Some(ref visible_url) = visible_url {
            format!("Found {} web results for: {query}. Opened visibly: {visible_url}", items.len())
        } else {
            format!("Found {} web results for: {query}", items.len())
        },
        content: serde_json::json!({
            "query": query,
            "results": items,
            "fallback": "rust_http",
            "visible_url": visible_url
        })
        .to_string(),
    })
}

fn read_page_http(name: &str, url: &str, visible_url: Option<String>) -> Result<BrowserToolResult> {
    validate_url(url)?;
    let body = http_get(url)?;
    let title = extract_between_case_insensitive(&body, "<title", "</title>")
        .and_then(|raw| raw.split_once('>').map(|(_, text)| text.to_string()))
        .map(|text| decode_entities(&text))
        .unwrap_or_else(|| url.to_string());
    let text = html_to_text(&body);
    Ok(BrowserToolResult {
        tool: name.into(),
        summary: format!(
            "{} page: {}{}",
            if name == "browser.open_url" { "Opened" } else { "Read" },
            title,
            visible_url
                .as_ref()
                .map(|url| format!(". Opened visibly: {url}"))
                .unwrap_or_default()
        ),
        content: serde_json::json!({
            "title": title,
            "url": url,
            "text": text,
            "fallback": "rust_http",
            "visible_url": visible_url
        })
        .to_string(),
    })
}

fn open_visible_target(name: &str, args: &serde_json::Value) -> Result<Option<String>> {
    let url = match name {
        "browser.search_web" => {
            let query = required_string(args, "query")?;
            format!("https://www.google.com/search?q={}", url_encode(query))
        }
        "browser.open_url" | "browser.extract_page_text" => required_string(args, "url")?.to_string(),
        _ => return Ok(None),
    };
    open_url_in_default_browser(&url)?;
    Ok(Some(url))
}

fn open_url_in_default_browser(url: &str) -> Result<()> {
    #[cfg(windows)]
    {
        Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", url])
            .spawn()
            .context("failed to open visible browser")?;
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(url)
            .spawn()
            .context("failed to open visible browser")?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new("xdg-open")
            .arg(url)
            .spawn()
            .context("failed to open visible browser")?;
    }
    Ok(())
}

fn attach_visible_url(content: &str, visible_url: &str) -> String {
    match serde_json::from_str::<serde_json::Value>(content) {
        Ok(mut value) => {
            value["visible_url"] = serde_json::json!(visible_url);
            value.to_string()
        }
        Err(_) => serde_json::json!({
            "text": content,
            "visible_url": visible_url
        })
        .to_string(),
    }
}

fn http_get(url: &str) -> Result<String> {
    let response = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent("voice-mcp-host/0.2.1")
        .build()
        .context("failed to build browser HTTP client")?
        .get(url)
        .send()
        .context("browser HTTP request failed")?;
    let status = response.status();
    let text = response.text().unwrap_or_default();
    if !status.is_success() {
        bail!("browser HTTP request returned {status}: {text}");
    }
    Ok(text)
}

#[derive(Debug, Serialize)]
struct SearchResult {
    title: String,
    url: String,
    snippet: String,
    published: String,
}

fn extract_rss_items(body: &str) -> Vec<SearchResult> {
    body.split("<item>")
        .skip(1)
        .take(8)
        .filter_map(|chunk| {
            let item = chunk.split("</item>").next().unwrap_or(chunk);
            let title = extract_xml_tag(item, "title");
            let url = extract_xml_tag(item, "link");
            let snippet = extract_xml_tag(item, "description");
            let published = extract_xml_tag(item, "pubDate");
            if title.is_empty() && url.is_empty() && snippet.is_empty() {
                None
            } else {
                Some(SearchResult {
                    title,
                    url,
                    snippet,
                    published,
                })
            }
        })
        .collect()
}

fn extract_xml_tag(text: &str, tag: &str) -> String {
    let start = format!("<{tag}>");
    let end = format!("</{tag}>");
    extract_between_case_insensitive(text, &start, &end)
        .map(|value| decode_entities(value.trim()))
        .unwrap_or_default()
}

fn extract_between_case_insensitive<'a>(text: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let lower = text.to_ascii_lowercase();
    let start_lower = start.to_ascii_lowercase();
    let end_lower = end.to_ascii_lowercase();
    let start_pos = lower.find(&start_lower)?;
    let content_start = start_pos + start.len();
    let end_pos = lower[content_start..].find(&end_lower)? + content_start;
    Some(&text[content_start..end_pos])
}

fn html_to_text(body: &str) -> String {
    let mut text = String::new();
    let mut in_tag = false;
    let mut last_space = false;
    for ch in body.chars() {
        match ch {
            '<' => {
                in_tag = true;
                if !last_space {
                    text.push('\n');
                    last_space = true;
                }
            }
            '>' => in_tag = false,
            _ if in_tag => {}
            c if c.is_whitespace() => {
                if !last_space {
                    text.push(' ');
                    last_space = true;
                }
            }
            c => {
                text.push(c);
                last_space = false;
            }
        }
        if text.len() >= 18_000 {
            break;
        }
    }
    decode_entities(&text)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn decode_entities(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
}

fn url_encode(text: &str) -> String {
    text.bytes()
        .flat_map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => vec![b as char],
            b' ' => vec!['+'],
            other => format!("%{other:02X}").chars().collect(),
        })
        .collect()
}

fn sidecar_stdout(stdout: &str) -> String {
    if stdout.trim().is_empty() {
        String::new()
    } else {
        format!("; stdout: {}", stdout.trim())
    }
}

fn browser_script_path() -> Option<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("src-tauri").join("sidecars").join("browser_readonly.mjs"));
        candidates.push(cwd.join("sidecars").join("browser_readonly.mjs"));
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("sidecars").join("browser_readonly.mjs"));
            candidates.push(dir.join("resources").join("sidecars").join("browser_readonly.mjs"));
            candidates.push(dir.join("..").join("Resources").join("sidecars").join("browser_readonly.mjs"));
        }
    }

    candidates.into_iter().find(|path| path.exists())
}

fn validate_url(url: &str) -> Result<()> {
    let lower = url.trim().to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        Ok(())
    } else {
        bail!("browser URL must start with http:// or https://")
    }
}

fn required_string<'a>(args: &'a serde_json::Value, key: &str) -> Result<&'a str> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .with_context(|| format!("missing required string argument: {key}"))
}
