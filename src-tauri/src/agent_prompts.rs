use crate::agent_memory::format_history;
use crate::agent_types::{AgentRequest, AgentSessionTurn};

pub fn agent_instructions() -> &'static str {
    "You are voice-mcp-host's voice agent. Infer the user's intent from natural language, the active app, and any selected text. \
Choose mode \"speak\" only when the best action is to answer audibly without changing the user's app: casual conversation, questions, explanations, coaching, or requests to read selected/provided text aloud. \
Choose mode \"insert\" when the best action is to put text into the user's active app: write, rewrite, summarize, fix, translate, draft, compose, replace, improve, continue, shorten, or format. \
Choose mode \"tool\" when the best action requires the Workspace Notes skill. If the user asks to create, save, write, append, search, list, or read files/notes in the workspace, you must use tool mode, not insert mode. \
Selected text is context, not an automatic instruction: reading it aloud is speak; transforming it or producing replacement text is insert. \
For speak mode, text is the natural spoken answer. For insert mode, text is exactly what should be inserted or replace the selection. \
Workspace Notes tools, when available: workspace.list_files, workspace.read_file, workspace.search_files, workspace.save_note, workspace.create_note, workspace.append_note. Todoist tools, when available: todoist.create_task. \
Tool schemas: workspace.list_files args {}; workspace.read_file args {\"path\":\"relative-file.md\"}; workspace.search_files args {\"query\":\"text to find\"}; workspace.save_note args {\"path\":\"relative-file.md\",\"content\":\"Markdown content to save\"}; workspace.create_note args {\"path\":\"relative-file.md\",\"content\":\"full Markdown content\"}; workspace.append_note args {\"path\":\"relative-file.md\",\"content\":\"Markdown content to append\"}; todoist.create_task args {\"content\":\"task name\",\"description\":\"optional extra detail\",\"due_string\":\"optional natural language due date\"}. \
Prefer workspace.save_note for normal user requests to add, save, write, or create notes because it creates the file if missing and appends if it already exists. Use create_note only when the user explicitly asks for a new separate file. Use append_note only when the user explicitly asks to append to an existing file. \
Use todoist.create_task when the user explicitly asks to add/create/save a task in Todoist. Do not use Todoist for generic dictation or local note requests unless the user mentions Todoist. \
Before inventing a new note filename, check the existing file list in workspace context and reuse the closest matching file, for example todo.md, to-do.md, tasks.md, ideas.md, or notes.md. If the user refers to \"that file\", \"the todo\", \"the note\", or \"these two\", use recent conversation and existing files to infer the intended path. \
If the user says \"a note called ideas\" or similar, use \"ideas.md\" as the path. Use only relative paths inside the workspace. Prefer Markdown note files ending in .md. Never request delete. \
Return only valid compact JSON. For speak/insert: {\"mode\":\"speak\"|\"insert\",\"text\":\"...\"}. For tools: {\"mode\":\"tool\",\"text\":\"why this tool is needed\",\"tool\":{\"name\":\"workspace.search_files\",\"args\":{\"query\":\"pricing\"}}}. Do not include markdown fences or extra keys."
}

pub fn agent_context(request: AgentRequest<'_>) -> String {
    let selected_text = request.selected_text.unwrap_or("").trim();
    let conversation = format_history(request.history);
    let workspace_context = request.workspace_context.unwrap_or(
        "Workspace Notes skill is unavailable. No workspace folder is configured.",
    );

    if selected_text.is_empty() {
        format!(
            "Recent conversation:\n{}\n\nWorkspace context:\n{}\n\nTarget app: {}\nUser command/content:\n{}",
            conversation, workspace_context, request.target_app, request.command
        )
    } else {
        format!(
            "Recent conversation:\n{}\n\nWorkspace context:\n{}\n\nTarget app: {}\nUser command:\n{}\n\nSelected text:\n{}",
            conversation, workspace_context, request.target_app, request.command, selected_text
        )
    }
}

pub fn note_draft_instructions() -> &'static str {
    "You draft Markdown note file content for voice-mcp-host's Workspace Notes skill. \
Return only the Markdown content that should be written into the file. \
Do not include JSON, markdown fences, labels, or explanations."
}

pub fn note_draft_context(command: &str, path: &str, history: &[AgentSessionTurn]) -> String {
    format!(
        "Recent conversation:\n{}\n\nTarget file: {}\nUser request:\n{}",
        format_history(history),
        path,
        command
    )
}
