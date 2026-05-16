# Installation

## Windows

### Recommended Install

1. Open the latest GitHub release:

   ```text
   https://github.com/<owner>/voice-mcp-host/releases/latest
   ```

2. Download:

   ```text
   voice-mcp-host_0.1.0_x64-setup.exe
   ```

3. Double-click the installer.

4. Open `voice-mcp-host` from the Start menu.

5. Follow the in-app setup:

   - download the transcription engine
   - download the Whisper model

6. Press `F3` in any app to dictate.

### Windows Notes

- No Python is required.
- No developer tools are required.
- The optional faster-whisper backend is bundled in the installer.
- NVIDIA GPUs use CUDA acceleration when available.
- AMD/Intel/no GPU machines use CPU transcription.
- Early releases are unsigned, so Windows SmartScreen may show a warning.

### Closing The App

- Pressing `X` minimizes Settings and keeps dictation running.
- Press `Quit` in Settings to fully exit the background app.

## macOS

macOS support is still being tested.

### Test Install

1. Download the macOS DMG from the GitHub release when available.
2. Open the DMG and drag `voice-mcp-host` to Applications.
3. On first launch, unsigned builds may be blocked by Gatekeeper.

Use one of these options:

```bash
xattr -d com.apple.quarantine /Applications/voice-mcp-host.app
```

Or right-click the app in Finder, choose **Open**, then confirm.

### macOS Permissions

macOS requires:

- Microphone permission for recording
- Accessibility permission for paste

Accessibility is required because voice-mcp-host sends a synthetic `Cmd+V` to paste into the focused app.

Enable it here:

```text
System Settings -> Privacy & Security -> Accessibility -> voice-mcp-host
```

### macOS Hotkey

The default macOS hotkey is:

```text
F5
```

## Build From Source

Only contributors need this.

### Windows

```powershell
npm ci
npm run tauri dev
```

Build installer:

```powershell
npm run release:windows
```

### macOS

```bash
npm ci
npm run tauri dev
```

Build DMG:

```bash
npm run tauri build -- --bundles dmg
```

Requires Rust, Node.js 20+, and Xcode Command Line Tools.
