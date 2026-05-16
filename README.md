# voice-mcp-host

Local-first desktop dictation for Windows and macOS.

Press a hotkey, speak, and the transcript is pasted into the app you were already using.

## What It Does

voice-mcp-host is a small background app for system-wide speech-to-text:

- Press `F3` on Windows, or `F5` on macOS.
- Speak.
- Press the hotkey again to stop.
- The app transcribes locally and pastes the text at your cursor.

It works in any app that accepts pasted text: browser fields, email, Slack, notes, editors, documents, and terminals.

## Current Status

Version `0.1.0` is focused on local dictation.

Windows is the primary tested platform right now. macOS support is in progress and needs real-device QA for permissions, packaging, and paste behavior.

Future versions are planned to add:

- Ask mode: speak a question and paste an LLM answer.
- Agent mode: connect LLMs to MCP tools with approval prompts.
- More models, backends, and platform polish.

## Install

### Windows

1. Go to the latest release:

   ```text
   https://github.com/<owner>/voice-mcp-host/releases/latest
   ```

2. Download:

   ```text
   voice-mcp-host_0.1.0_x64-setup.exe
   ```

3. Run the installer.

4. Open `voice-mcp-host` from the Start menu.

5. On first launch, the app guides you through downloading:

   - a transcription engine
   - a Whisper model

6. When status says `Ready`, press `F3` anywhere to dictate.

Windows SmartScreen may warn because early builds are unsigned. Choose **More info** then **Run anyway** if you trust the release source.

### macOS

macOS builds are intended, but should be considered test builds until signed/notarized releases are available.

See [INSTALL.md](INSTALL.md) for current macOS test instructions.

## Requirements

Normal users do not need developer tools.

You do **not** need:

- Python
- Node.js
- npm
- Rust
- Git
- Visual Studio
- CUDA Toolkit
- an API key
- an internet account

You only need:

- Windows 10/11 for the Windows installer
- internet access for first-time model/engine download
- a microphone

## Privacy

Dictation runs locally on your machine.

By default:

- audio is captured locally
- transcription runs locally
- transcript text is pasted locally
- no cloud API key is required
- no audio is uploaded by the app

The app may download engines/models from upstream sources during setup. After models are downloaded, normal dictation does not require internet access.

## ASR Backends

voice-mcp-host currently supports two local transcription backends.

### whisper.cpp

Recommended default.

- NVIDIA GPU on Windows: CUDA engine
- AMD/Intel/no GPU on Windows: CPU engine
- macOS target: Metal engine

The app chooses the appropriate whisper.cpp engine automatically.

### faster-whisper

Optional advanced backend.

The Windows installer bundles a portable Python runtime and faster-whisper packages, so users do not need to install Python themselves.

Device choices:

- CUDA: NVIDIA GPU
- CPU: fallback path

Compute choices are restricted by device so invalid combinations such as CPU/float16 are not shown.

## Daily Use

- `F3`: start listening on Windows
- `F3` again: stop and transcribe
- `X` on Settings: minimize/keep app running
- `Quit`: fully exit the background app

The overlay shows the current state:

- Listening
- Transcribing
- Inserting
- Inserted

## Troubleshooting

### F3 Does Nothing

Check that voice-mcp-host is still running. Open it from the Start menu and confirm status is `Ready`.

Another app may also be using `F3`. Change the hotkey in Settings if needed.

### Text Does Not Paste

The app copies the transcript to the clipboard before sending paste. If paste fails, try pressing `Ctrl+V` manually in the target app.

### CPU Is Slow

CPU transcription is expected to be slower than NVIDIA CUDA. If you have an NVIDIA GPU, use the CUDA whisper.cpp engine or faster-whisper CUDA.

### Model Download Is Large

Whisper models are large because they run locally. The recommended Large v3 Turbo Q5 model balances speed, quality, and download size.

## Build From Source

Only developers need this.

Prerequisites:

- Node.js 20+
- Rust stable
- Tauri build prerequisites for your OS

Windows dev run:

```powershell
npm ci
npm run tauri dev
```

Windows release build:

```powershell
npm run release:windows
```

The Windows release script prepares the bundled faster-whisper runtime and produces an NSIS installer.

## Project Direction

The project is intended to grow in three tiers:

- Tier 1: local dictation
- Tier 2: LLM ask mode
- Tier 3: MCP-powered agent mode

The core goal is a local-first, open-source voice interface that can later connect to user-controlled LLMs and tools without locking people into one vendor.

## License

MIT.
