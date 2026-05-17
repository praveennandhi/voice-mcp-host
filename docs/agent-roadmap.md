# Agent Roadmap

This file is project memory for Codex, Claude Code, and future contributors. Keep it updated when the agent architecture changes.

## Current Direction

voice-mcp-host is moving from dictation into a local-first desktop agent for small business workflows. Voice is an input method, not the whole product.

Current stack:

- Tauri desktop shell
- Rust backend/local runtime
- React + TypeScript frontend
- whisper.cpp for dictation/transcription
- OpenAI Responses API for agent reasoning
- OpenAI TTS for optional spoken responses
- Direct Todoist connector
- Local workspace note tools

## Product Principles

- The user should speak naturally.
- The app should infer intent, not expose tool mechanics.
- Local machine operations must stay guarded in Rust.
- External actions must require confirmation.
- Do not let the LLM directly write arbitrary local paths or run arbitrary commands.
- MCP is a future connector layer, not something to force into every first integration.
- Direct connectors are acceptable when they prove product value faster.

## Current Agent Capabilities

- Dictation without agent trigger.
- Agent trigger word for reasoning/actions.
- Session chat and voice share the same current session history.
- Workspace tools:
  - `workspace.list_files`
  - `workspace.read_file`
  - `workspace.search_files`
  - `workspace.save_note`
  - `workspace.create_note`
  - `workspace.append_note`
- Todoist tools:
  - `todoist.create_task`
  - `todoist.complete_task`
- Short-term memory:
  - recent conversation turns
  - pending confirmation
  - last created Todoist task

## Known Design Problem: Context Bloat

Right now the agent prompt includes too much shared context:

- general agent behavior
- all workspace tool schemas
- all Todoist tool schemas
- recent conversation
- workspace file list
- last Todoist task
- current command

This is acceptable for a prototype, but it will not scale as we add Gmail, Calendar, Notion, Linear, browser, memory, and MCP tools.

Risks:

- wrong tool selection
- duplicate actions
- slower responses
- higher token cost
- weaker intent handling
- hallucinated tool arguments
- model confusion from unrelated tools

## Required Next Architecture Step

Add tool routing before adding many more integrations.

Target flow:

```text
User command
  -> lightweight router
  -> domain-specific prompt/context
  -> safe tool execution
  -> response/speak/insert
```

Initial router domains:

- `dictation`
- `chat`
- `insert`
- `workspace`
- `todoist`
- later `calendar`
- later `email`
- later `mcp`

The router should decide which prompt/context to use. Do not send all tools to the model on every request.

Examples:

- `Complete the task you created`
  - Domain: `todoist`
  - Context: Todoist tools + last Todoist task
  - Do not include workspace file list.

- `Add this to ideas.md`
  - Domain: `workspace`
  - Context: workspace tools + relevant file list
  - Do not include Todoist tools.

- `Rewrite this selected text`
  - Domain: `insert`
  - Context: selected text + rewrite instruction
  - No external tools.

## Bypass LLM When Possible

Some interactions should not require model reasoning:

- pending yes/no confirmations
- cancel
- complete the last created Todoist task
- possibly open settings/logs
- obvious app-control commands

These should be deterministic Rust paths where safe.

## Memory Direction

Current memory is session-only. Planned memory layers:

1. Current session history.
2. Short-term action memory, such as last Todoist task.
3. `memory.md` or structured project memory for preferences and durable facts.
4. Workspace summaries/index.
5. Optional vector search later, only if needed.

Memory should not be dumped wholesale into every prompt. Retrieve or summarize only the relevant parts.

## Connector Strategy

Direct connectors first when they are simple and useful:

- Todoist task creation/completion is the first direct connector.
- Calendar and email may need OAuth and should be treated carefully.

MCP later:

- Add MCP as a connector runtime once the internal tool registry is clean.
- Avoid sending every MCP tool into every prompt.
- Prefer routing to relevant MCP servers/tools.

## Todoist Notes

Todoist is currently implemented as a direct Rust connector using a user-provided personal API token.

Current limitations:

- No OAuth.
- No project/label selection.
- Only create and complete task.
- Last-task memory is in-process and resets on app restart.

Next Todoist improvements:

- Better confirmation wording.
- Store recent Todoist task refs in session memory, not just one task.
- Support finding a task by name before completing it.
- Add project selection later.
- OAuth later for public users.

## UI Direction

Current UI is functional but plain. Long-term UI should feel like a polished local business agent, not a developer settings panel.

Do not spend too much time redesigning before core workflows feel good, but keep this need visible.

## Important Reminder

If the agent feels dumb, first check whether we gave it the right tool, context, and memory. Do not assume the model is the main problem.
