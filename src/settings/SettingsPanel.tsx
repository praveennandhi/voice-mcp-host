import { useEffect, useState, useCallback } from 'react';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { api } from '../lib/api';
import type { AppStatus, AvailableModel, Config, DownloadProgress } from '../lib/types';

const IS_MACOS = navigator.userAgent.includes('Macintosh');

export default function SettingsPanel() {
  const [config, setConfig] = useState<Config | null>(null);
  const [status, setStatus] = useState<AppStatus | null>(null);
  const [devices, setDevices] = useState<string[]>([]);
  const [models, setModels] = useState<AvailableModel[]>([]);
  const [version, setVersion] = useState('');
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [downloading, setDownloading] = useState(false);
  const [downloadProgress, setDownloadProgress] = useState(0);
  const [saveTimer, setSaveTimer] = useState<ReturnType<typeof setTimeout> | null>(null);

  const refresh = useCallback(async () => {
    const [cfg, st, devs, mods, ver] = await Promise.all([
      api.getConfig(),
      api.getStatus(),
      api.listAudioDevices(),
      api.listModels(),
      api.getVersion(),
    ]);
    setConfig(cfg);
    setStatus(st);
    setDevices(devs);
    setModels(mods);
    setVersion(ver);
  }, []);

  useEffect(() => {
    refresh();

    // Listen for download progress
    const win = getCurrentWebviewWindow();
    const unlisten = win.listen<DownloadProgress>('download-progress', (event) => {
      setDownloadProgress(event.payload.percent);
    });

    return () => { unlisten.then(fn => fn()); };
  }, [refresh]);

  const handleConfigChange = (patch: Partial<Config>) => {
    if (!config) return;
    const updated = { ...config, ...patch };
    setConfig(updated);
    // Debounce save
    if (saveTimer) clearTimeout(saveTimer);
    setSaveTimer(setTimeout(() => api.saveConfig(updated), 600));
  };

  const handleDownload = async () => {
    if (!config) return;
    setDownloading(true);
    setDownloadProgress(0);
    try {
      await api.downloadModel(config.asr.model_name);
      await refresh();
    } catch (e) {
      console.error('Download failed:', e);
    } finally {
      setDownloading(false);
    }
  };

  const handleGrantAccessibility = async () => {
    await api.requestAccessibilityPermission();
    const perms = await api.checkPermissions();
    setStatus(s => s ? { ...s, permissions: perms } : s);
  };

  if (!config || !status) {
    return <div className="settings-root"><div className="settings-body">Loading…</div></div>;
  }

  const needsPermissions = IS_MACOS && !status.permissions.microphone.includes('Granted') || !status.permissions.accessibility.includes('Granted');
  const modelReady = status.model_downloaded && status.transcriber_ready;

  // Show permission screen if on macOS and permissions are missing
  if (IS_MACOS && (
    status.permissions.microphone === 'Denied' ||
    status.permissions.accessibility === 'Denied'
  )) {
    return (
      <div className="settings-root">
        <div className="settings-header"><h1>voice-mcp-host needs permissions</h1></div>
        <div className="settings-body">
          <div className="section">
            <div className="permission-row">
              <span className="label">Microphone</span>
              <span className={`state status-badge ${status.permissions.microphone === 'Granted' ? 'ok' : 'error'}`}>
                {status.permissions.microphone === 'Granted' ? '✓ Granted' : '✗ Denied'}
              </span>
            </div>
            <div className="permission-row">
              <span className="label">Accessibility (required for paste)</span>
              <span className={`state status-badge ${status.permissions.accessibility === 'Granted' ? 'ok' : 'error'}`}>
                {status.permissions.accessibility === 'Granted' ? '✓ Granted' : '✗ Denied'}
              </span>
              {status.permissions.accessibility !== 'Granted' && (
                <button onClick={handleGrantAccessibility}>Grant…</button>
              )}
            </div>
            <p style={{ color: 'var(--text-muted)', fontSize: 13, lineHeight: 1.6 }}>
              Accessibility is required for voice-mcp-host to send Cmd+V to the focused app.
              Without it, paste silently does nothing. Grant it in System Settings → Privacy &amp; Security → Accessibility.
            </p>
            <div className="button-row">
              <button onClick={refresh}>Re-check ↻</button>
            </div>
          </div>
        </div>
      </div>
    );
  }

  // Show download screen if model is missing
  if (!status.model_downloaded) {
    const modelInfo = models.find(m => m.name === config.asr.model_name);
    const sizeMB = modelInfo ? Math.round(modelInfo.size_bytes / 1_000_000) : 150;
    return (
      <div className="settings-root">
        <div className="settings-header"><h1>voice-mcp-host</h1></div>
        <div className="settings-body">
          <div className="download-block">
            <h2 style={{ fontSize: 16, fontWeight: 600 }}>Download Whisper model</h2>
            <p>
              voice-mcp-host runs transcription locally on your device.<br />
              The {config.asr.model_name.replace('ggml-', '').replace('.bin', '')} model is ~{sizeMB} MB.
            </p>
            {downloading ? (
              <>
                <div className="progress-bar-wrap">
                  <div className="progress-bar-fill" style={{ width: `${downloadProgress}%` }} />
                </div>
                <span style={{ fontSize: 13, color: 'var(--text-muted)' }}>{downloadProgress.toFixed(0)}%</span>
              </>
            ) : (
              <button className="primary" onClick={handleDownload}>
                Download {config.asr.model_name.replace('ggml-', '').replace('.bin', '')} (~{sizeMB} MB)
              </button>
            )}
            <div className="form-row" style={{ marginTop: 8 }}>
              <label>Model</label>
              <select
                value={config.asr.model_name}
                onChange={e => handleConfigChange({ asr: { ...config.asr, model_name: e.target.value } })}
                disabled={downloading}
              >
                {models.map(m => (
                  <option key={m.name} value={m.name}>
                    {m.name.replace('ggml-', '').replace('.bin', '')} (~{Math.round(m.size_bytes / 1_000_000)} MB)
                  </option>
                ))}
              </select>
            </div>
          </div>
        </div>
      </div>
    );
  }

  // Main settings UI
  return (
    <div className="settings-root">
      <div className="settings-header">
        <h1>voice-mcp-host</h1>
      </div>

      <div className="settings-body">
        <div className="section">
          <div className="form-row">
            <label>Hotkey</label>
            <input
              type="text"
              value={config.dictation.primary_hotkey}
              onChange={e => handleConfigChange({ dictation: { ...config.dictation, primary_hotkey: e.target.value } })}
              placeholder={IS_MACOS ? 'F5' : 'F2'}
              style={{ maxWidth: 120 }}
            />
          </div>
          <div className="form-row">
            <label>Model</label>
            <select
              value={config.asr.model_name}
              onChange={e => handleConfigChange({ asr: { ...config.asr, model_name: e.target.value } })}
            >
              {models.map(m => (
                <option key={m.name} value={m.name}>
                  {m.name.replace('ggml-', '').replace('.bin', '')}
                  {m.downloaded ? ' ✓' : ' (not downloaded)'}
                </option>
              ))}
            </select>
          </div>
          <div className="form-row">
            <label>Status</label>
            <span className={`status-badge ${modelReady ? 'ok' : 'warn'}`}>
              {modelReady ? '✓ Ready' : '⚠ Model loading…'}
            </span>
          </div>
          <div className="form-row">
            <label>Microphone</label>
            <select
              value={config.audio.input_device_id ?? ''}
              onChange={e => handleConfigChange({ audio: { ...config.audio, input_device_id: e.target.value || null } })}
            >
              <option value="">Default device</option>
              {devices.map(d => <option key={d} value={d}>{d}</option>)}
            </select>
          </div>
          <div className="form-row">
            <label>Language</label>
            <input
              type="text"
              value={config.dictation.language}
              onChange={e => handleConfigChange({ dictation: { ...config.dictation, language: e.target.value } })}
              style={{ maxWidth: 80 }}
            />
          </div>
        </div>

        <button className="advanced-toggle" onClick={() => setShowAdvanced(v => !v)}>
          {showAdvanced ? '▾' : '▸'} Advanced
        </button>

        {showAdvanced && (
          <div className="section">
            <div className="section-title">Timing</div>
            <div className="form-row">
              <label>Paste delay (ms)</label>
              <input
                type="number"
                value={config.insertion.paste_delay_ms}
                min={0} max={2000}
                style={{ maxWidth: 90 }}
                onChange={e => handleConfigChange({ insertion: { ...config.insertion, paste_delay_ms: Number(e.target.value) } })}
              />
            </div>
            <div className="form-row">
              <label>Restore delay (ms)</label>
              <input
                type="number"
                value={config.insertion.restore_delay_ms}
                min={0} max={5000}
                style={{ maxWidth: 90 }}
                onChange={e => handleConfigChange({ insertion: { ...config.insertion, restore_delay_ms: Number(e.target.value) } })}
              />
            </div>
            <div className="form-row">
              <label>Min record (ms)</label>
              <input
                type="number"
                value={config.dictation.min_record_ms}
                min={100} max={5000}
                style={{ maxWidth: 90 }}
                onChange={e => handleConfigChange({ dictation: { ...config.dictation, min_record_ms: Number(e.target.value) } })}
              />
            </div>
            <div className="form-row">
              <label>Max record (s)</label>
              <input
                type="number"
                value={config.dictation.max_record_seconds}
                min={10} max={600}
                style={{ maxWidth: 90 }}
                onChange={e => handleConfigChange({ dictation: { ...config.dictation, max_record_seconds: Number(e.target.value) } })}
              />
            </div>
          </div>
        )}

        <div className="button-row">
          <button onClick={api.openLogDir}>Open log dir</button>
          {!status.model_downloaded && (
            <button className="primary" onClick={handleDownload} disabled={downloading}>
              Download model
            </button>
          )}
        </div>
      </div>

      <div className="settings-footer">
        <span>voice-mcp-host {version} · MIT licensed</span>
        <span style={{ color: 'var(--text-muted)' }}>
          {IS_MACOS ? 'F5 to dictate' : 'F2 to dictate'}
        </span>
      </div>
    </div>
  );
}
