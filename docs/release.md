# Release

## Windows

Build the unsigned Windows installer from a Windows machine:

```powershell
npm run release:windows
```

Artifacts are written to:

```text
src-tauri\target\release\bundle\nsis\
```

Upload the generated `.exe` installer and `SHA256SUMS.txt` to the GitHub release.

## Windows User Install

Normal users should install from GitHub Releases:

1. Open:

   ```text
   https://github.com/<github-owner>/voice-mcp-host/releases/latest
   ```

2. Download:

   ```text
   voice-mcp-host_0.1.0_x64-setup.exe
   ```

3. Run the installer and open `voice-mcp-host` from the Start menu.

The PowerShell installer is optional for advanced users/admins:

```powershell
powershell -ExecutionPolicy Bypass -Command "& ([scriptblock]::Create((irm https://raw.githubusercontent.com/<github-owner>/voice-mcp-host/main/scripts/install-windows.ps1))) -Repo <github-owner>/voice-mcp-host"
```

Replace `<github-owner>` with the GitHub user or organization that owns the `voice-mcp-host` repo.

For a fork:

```powershell
powershell -ExecutionPolicy Bypass -Command "& ([scriptblock]::Create((irm https://raw.githubusercontent.com/<fork-owner>/voice-mcp-host/main/scripts/install-windows.ps1))) -Repo <fork-owner>/voice-mcp-host"
```

## What The Installer Includes

The installer includes the Tauri desktop app, whisper.cpp integration, and a bundled portable
Python runtime for the optional faster-whisper backend. Users do not need to install Python.

Whisper engines and models are downloaded by the app on first launch so users can choose the right path:

- Windows/NVIDIA default recommendation: `whisper.cpp` + CUDA + `Large v3 Turbo Q5`
- Windows/CPU fallback: `whisper.cpp` + CPU
- macOS target: `whisper.cpp` + Metal
- Optional faster-whisper backend: bundled Python + faster-whisper packages; the selected model downloads on first use.

The Windows release script prepares the bundled faster-whisper runtime before creating the NSIS installer:

```powershell
npm run release:windows
```

To skip the faster-whisper runtime during local packaging only:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/build-release-windows.ps1 -SkipFasterWhisperBundle
```

## macOS

Build the unsigned macOS DMG from a macOS machine:

```bash
npm ci
npm run tauri build -- --bundles dmg
```

Artifacts are written to:

```text
src-tauri/target/release/bundle/dmg/
```

Unsigned builds require the Gatekeeper bypass documented in `INSTALL.md`.
