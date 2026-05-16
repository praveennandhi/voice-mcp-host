use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSessionTurn {
    pub role: String,
    pub content: String,
    pub mode: Option<String>,
}

pub struct AgentRequest<'a> {
    pub command: &'a str,
    pub selected_text: Option<&'a str>,
    pub target_app: &'a str,
    pub history: &'a [AgentSessionTurn],
    pub workspace_context: Option<&'a str>,
}

pub struct AgentResult {
    pub mode: AgentOutputMode,
    pub text: String,
    pub tool_call: Option<ToolCall>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentOutputMode {
    Insert,
    Speak,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub name: String,
    #[serde(default)]
    pub args: serde_json::Value,
}
