# voice-mcp-host

> **Working name — final name TBD.** This is a placeholder folder name so the project can move forward without bikeshedding.

A local-first, hotkey-driven dictation app for Windows and macOS that progressively unlocks LLM and tool-use capabilities.

**Hold a hotkey. Talk. Text appears at your cursor.**

That's the whole pitch in one sentence. The rest is optional power.

---

## Status

**Planning. No code exists yet.** This README is the design contract — what we agreed to build, what we agreed *not* to build, and in what order. Code starts after this document survives a night of sleep.

License: **MIT**. The intent is genuinely "use this, fork it, ship it." Not a commercial play.

---

## What it is

A voice tool with three progressive tiers. Each tier is independently useful — you can stop at any tier and still have a working app.

### Tier 1 — Dictation *(no config required)*

Install. Set a hotkey. Talk. Text appears at the cursor in whatever app is focused.

- Fully offline. No internet, no account, no API keys.
- Whisper running locally — CPU everywhere, Metal on Apple Silicon, CUDA on Windows with NVIDIA.
- Works in any application that accepts text input.

If you never go past Tier 1, you have a real dictation app and that's fine.

### Tier 2 — Ask mode *(add an LLM key)*

Drop in an OpenAI-compatible endpoint and key in settings. Prefix your dictation with `"ask, ..."`:

> *"ask, what's the capital of Mongolia?"*

The reply gets pasted at your cursor. Works with:

- **Local:** Ollama, LM Studio, llama.cpp server, anything OpenAI-compatible running on your machine
- **Cloud:** OpenAI, Groq, Together, Fireworks, or anything that speaks the OpenAI chat API

Within-session conversation memory is on — follow-up turns know what the previous turn was about. Clear it with a button or a timeout.

### Tier 3 — Agent mode *(add MCP servers)*

Configure one or more [MCP](https://modelcontextprotocol.io) servers. Prefix with `"agent, ..."` and the LLM can call tools:

> *"agent, add this paragraph to my Obsidian daily note"*
> *"agent, turn off the office lights"*
> *"agent, what did I commit yesterday in the rust repo?"*

Each proposed tool call surfaces an **approval prompt** in the overlay before it executes — Allow / Deny / Always allow this tool. Tool results feed back to the LLM, which either chains more calls or produces a final reply (pasted at the cursor, same as dictation).

---

## Modes recap

| Prefix | Mode | Requires |
|---|---|---|
| *(none)* | Dictate | Nothing |
| `"ask, ..."` | LLM chat | LLM endpoint + key |
| `"agent, ..."` | Tool-using agent | LLM + ≥1 MCP server |

The prefix word is configurable in settings.

---

## On memory, intentionally

**This app does not implement its own long-term memory subsystem.** That is a feature, not a gap.

MCP already has memory servers — Anthropic's reference `memory` server, `mem0`, and others. Plug one in via Tier 3 and every agent interaction gets persistent memory across sessions, with whichever memory implementation you prefer. Swap it out when something better comes along.

What's built in:

- **Within-session memory** — multi-turn context inside an active conversation (Tier 2+).
- **Long-term memory** — *not built in. Use a memory MCP server (Tier 3).*

This keeps the core app small and lets you pick your own memory stack.

---

## What's NOT in v1

Equally important as what's in it.

- TTS / spoken replies *(Phase B — likely Piper local, ElevenLabs/OpenAI cloud)*
- A user-friendly MCP server marketplace *(v1 = paste JSON config; polish later)*
- Streaming token-by-token paste *(replies paste atomically)*
- Mac code-signing / notarization *(v1 ships unsigned; bypass docs included)*
- Anthropic / Gemini native LLM adapters *(OpenAI-compatible only at first)*
- Voice commands beyond the three modes *("new line," "press enter," etc.)*
- Mobile, web, or browser-extension versions

---

## Milestones

| Version | Adds | Platforms |
|---|---|---|
| 0.1 alpha | Tier 1 (dictate) | **Windows + macOS** |
| 0.2 beta | Tier 2 (ask mode) | Win + Mac |
| 1.0 | Tier 3 (agent mode) | Win + Mac |

Each milestone is a real release. If the project stalls after 0.1, there's still a usable cross-platform dictation app.

See [`docs/0.1-desktop-dictation.md`](docs/0.1-desktop-dictation.md) for the 0.1 engineering spec.

---

## Architecture (high-level)

- **Shell:** Tauri 2 — Rust core, React + TypeScript UI
- **ASR:** [`whisper-rs`](https://github.com/tazz4843/whisper-rs) (whisper.cpp bindings) — Metal on Mac, CUDA on Windows with NVIDIA, CPU everywhere else
- **Hotkey:** `tauri-plugin-global-shortcut`
- **Paste:** platform-specific
  - Windows: `windows-sys` clipboard + SendInput
  - macOS: AppKit + Accessibility API (`enigo` or direct AX)
- **LLM client:** `reqwest` against any OpenAI-compatible endpoint, supports tool/function calling
- **MCP host:** `rmcp` (official Rust MCP SDK); servers run as child processes over stdio

Whisper models download on first run. Nothing else gets bundled into the installer beyond the binary itself.

---

## How this relates to other tools

Honest context. These exist and you should know about them:

- **[Wispr Flow](https://wisprflow.ai)** — closed-source, cloud-based, polished. If you don't care about local-first or OSS, use this.
- **[OpenWhispr](https://github.com/OpenWhispr/openwhispr)** — open-source dictation with BYO models, also exploring MCP. Closely adjacent.
- **Claude Desktop, ChatGPT Desktop, Cursor** — voice + MCP exists or is coming on the LLM-vendor side, but tied to specific accounts/clouds.
- **Built-in Windows / macOS dictation** — works, but not LLM/MCP aware.

This project's specific bet: **fully local-first, BYO-LLM, BYO-MCP, hotkey-first, OSS.** If you have a homelab or just want your voice tool to work without a vendor account, this is for you.

---

## Contributing

Too early. Once 0.1 ships there'll be issues and a contribution guide. Until then this repo is design notes.

---

*This README is the source of truth for what this project is and isn't, until code exists to argue otherwise.*
