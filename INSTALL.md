# Installation

## Windows

1. Download `voice-mcp-host_0.1.0_x64-setup.exe` from the GitHub release.
2. Run the installer. Windows SmartScreen may show a warning — click **More info → Run anyway**.
3. Launch the app from the Start menu.
4. Click **Download model** and wait for the model to download (~150 MB).
5. Press **F2** in any application to start dictating.

## macOS

1. Download `voice-mcp-host_0.1.0_aarch64.dmg` (Apple Silicon) from the GitHub release.
2. Open the DMG and drag the app to Applications.
3. **First launch — Gatekeeper will block it.** Do one of the following:
   - Right-click the app in Finder → **Open** → confirm in the dialog.
   - Or run in Terminal: `xattr -d com.apple.quarantine /Applications/voice-mcp-host.app`
4. On first launch, grant **Microphone** access when prompted.
5. Grant **Accessibility** access when prompted (required for paste to work):
   - System Settings → Privacy & Security → Accessibility → toggle on voice-mcp-host.
6. Click **Download model** and wait for the model (~150 MB).
7. Press **F5** in any application to start dictating.

### Why Accessibility permission on macOS?

voice-mcp-host sends a synthetic Cmd+V keystroke to paste your dictation into the focused app.
macOS requires Accessibility permission to send synthetic keystrokes. Without it, the paste silently does nothing — the app will detect this and tell you before attempting.

### Intel Mac

Intel Mac builds are planned for 0.1.1. For now, Intel Mac users can build from source:

```bash
git clone https://github.com/your-org/voice-mcp-host
cd voice-mcp-host
npm install
npm run tauri build -- --target x86_64-apple-darwin
```

Requires: Rust, Node.js 20+, Xcode Command Line Tools.
