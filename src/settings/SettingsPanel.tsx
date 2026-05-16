import { useCallback, useEffect, useState } from 'react';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { api } from '../lib/api';
import type { AppStatus, AvailableModel, Config, DownloadProgress } from '../lib/types';

const IS_MACOS = navigator.userAgent.includes('Macintosh');
const FASTER_WHISPER_MODELS = [
  'h2oai/faster-whisper-large-v3-turbo',
  'Systran/faster-whisper-large-v3',
  'Systran/faster-whisper-medium',
  'Systran/faster-whisper-small',
];

function modelDisplayName(model?: AvailableModel, fallbackName = '') {
  if (model) {
    return model.recommended ? `${model.display_name} (Recommended)` : model.display_name;
  }
  return fallbackName.replace('ggml-', '').replace('.bin', '');
}

function modelSizeLabel(sizeBytes: number) {
  const mb = sizeBytes / 1_000_000;
  return mb >= 1000 ? `${(mb / 1000).toFixed(1)} GB` : `${Math.round(mb)} MB`;
}

function modelOptionLabel(model: AvailableModel) {
  const status = model.downloaded ? 'installed' : modelSizeLabel(model.size_bytes);
  const prefix = model.recommended ? 'Recommended - ' : '';
  return `${prefix}${model.display_name} (${status})`;
}

function accelerationDisplayName(acceleration: string) {
  if (acceleration === 'cuda') return 'NVIDIA CUDA';
  if (acceleration === 'metal') return 'Metal';
  if (acceleration === 'cpu') return 'CPU';
  if (acceleration === 'legacy') return 'CPU (legacy)';
  return acceleration.toUpperCase();
}

function defaultFasterWhisperCompute(device: string) {
  return device === 'cpu' ? 'int8' : 'float16';
}

function fasterWhisperComputeOptions(device: string) {
  return device === 'cpu'
    ? ['int8', 'int8_float32', 'float32']
    : ['float16', 'int8_float16', 'int8'];
}

function SettingsHeader({ title = 'voice-mcp-host' }: { title?: string }) {
  return (
    <div className="settings-header">
      <h1>{title}</h1>
      <button className="quit-button" onClick={api.quitApp}>Quit</button>
    </div>
  );
}

export default function SettingsPanel() {
  const [config, setConfig] = useState<Config | null>(null);
  const [status, setStatus] = useState<AppStatus | null>(null);
  const [devices, setDevices] = useState<string[]>([]);
  const [models, setModels] = useState<AvailableModel[]>([]);
  const [version, setVersion] = useState('');
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [downloading, setDownloading] = useState(false);
  const [downloadProgress, setDownloadProgress] = useState(0);
  const [downloadError, setDownloadError] = useState<string | null>(null);
  const [engineFallback, setEngineFallback] = useState<string | null>(null);
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

    const win = getCurrentWebviewWindow();
    const unlistenProgress = win.listen<DownloadProgress>('download-progress', (event) => {
      setDownloadProgress(event.payload.percent);
    });
    const unlistenFallback = win.listen<{ from: string; to: string; error: string }>('engine-fallback', (event) => {
      setEngineFallback(`CUDA install failed, using CPU fallback. ${event.payload.error}`);
    });

    return () => {
      unlistenProgress.then(fn => fn());
      unlistenFallback.then(fn => fn());
    };
  }, [refresh]);

  const handleConfigChange = (patch: Partial<Config>) => {
    if (!config) return;
    const updated = { ...config, ...patch };
    setConfig(updated);
    if (saveTimer) clearTimeout(saveTimer);
    setSaveTimer(setTimeout(() => api.saveConfig(updated), 600));
  };

  const handleDownload = async () => {
    if (!config) return;
    setDownloading(true);
    setDownloadProgress(0);
    setDownloadError(null);
    try {
      await api.downloadModel(config.asr.model_name);
      await refresh();
    } catch (e) {
      setDownloadError(String(e));
    } finally {
      setDownloading(false);
    }
  };

  const handleDownloadEngine = async () => {
    setDownloading(true);
    setDownloadProgress(0);
    setDownloadError(null);
    setEngineFallback(null);
    try {
      await api.downloadEngine();
      await refresh();
    } catch (e) {
      setDownloadError(String(e));
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
    return <div className="settings-root"><div className="settings-body">Loading...</div></div>;
  }

  const modelReady = status.model_downloaded && status.transcriber_ready;
  const isFasterWhisper = config.asr.backend === 'faster_whisper';
  const fasterWhisperComputeOptionsForDevice = fasterWhisperComputeOptions(config.asr.faster_whisper_device);
  const selectedFasterWhisperCompute = fasterWhisperComputeOptionsForDevice.includes(config.asr.faster_whisper_compute_type)
    ? config.asr.faster_whisper_compute_type
    : defaultFasterWhisperCompute(config.asr.faster_whisper_device);
  const preferredAcceleration = accelerationDisplayName(status.preferred_acceleration);
  const activeAcceleration = status.active_acceleration
    ? accelerationDisplayName(status.active_acceleration)
    : 'Not installed';
  const activeMatchesPreferred = status.active_acceleration === status.preferred_acceleration;

  if (IS_MACOS && (
    status.permissions.microphone === 'Denied' ||
    status.permissions.accessibility === 'Denied'
  )) {
    return (
      <div className="settings-root">
        <SettingsHeader title="voice-mcp-host needs permissions" />
        <div className="settings-body">
          <div className="section">
            <div className="permission-row">
              <span className="label">Microphone</span>
              <span className={`state status-badge ${status.permissions.microphone === 'Granted' ? 'ok' : 'error'}`}>
                {status.permissions.microphone === 'Granted' ? 'Granted' : 'Denied'}
              </span>
            </div>
            <div className="permission-row">
              <span className="label">Accessibility (required for paste)</span>
              <span className={`state status-badge ${status.permissions.accessibility === 'Granted' ? 'ok' : 'error'}`}>
                {status.permissions.accessibility === 'Granted' ? 'Granted' : 'Denied'}
              </span>
              {status.permissions.accessibility !== 'Granted' && (
                <button onClick={handleGrantAccessibility}>Grant...</button>
              )}
            </div>
            <p className="muted-copy">
              Accessibility is required for voice-mcp-host to send Cmd+V to the focused app.
              Grant it in System Settings, Privacy &amp; Security, Accessibility.
            </p>
            <div className="button-row">
              <button onClick={refresh}>Re-check</button>
            </div>
          </div>
        </div>
      </div>
    );
  }

  if (!isFasterWhisper && !status.engine_downloaded) {
    return (
      <div className="settings-root">
        <SettingsHeader />
        <div className="settings-body">
          <div className="download-block">
            <h2>Download transcription engine</h2>
            <p>
              {status.gpu_detected
                ? 'NVIDIA GPU detected. CUDA will be used for transcription.'
                : 'No NVIDIA GPU detected. CPU transcription will be used.'}
            </p>
            <div className="engine-status-grid">
              <span>Preferred</span>
              <strong>{preferredAcceleration}</strong>
              <span>Active</span>
              <strong>{activeAcceleration}</strong>
            </div>
            {downloading ? (
              <>
                <div className="progress-bar-wrap">
                  <div className="progress-bar-fill" style={{ width: `${downloadProgress}%` }} />
                </div>
                <span className="download-percent">{downloadProgress.toFixed(0)}%</span>
              </>
            ) : (
              <button className="primary" onClick={handleDownloadEngine}>
                Download {preferredAcceleration} engine
              </button>
            )}
            {engineFallback && <p className="inline-warning">{engineFallback}</p>}
            {downloadError && <p className="inline-error">Error: {downloadError}</p>}
            <div className="manual-engine-note">
              <span>Manual install path</span>
              <code>
                {IS_MACOS
                  ? '~/Library/Application Support/voice-mcp-host/models/engines/metal/'
                  : `%LOCALAPPDATA%\\voice-mcp-host\\models\\engines\\${status.preferred_acceleration}\\`}
              </code>
              <button onClick={refresh}>Re-check</button>
            </div>
          </div>
        </div>
      </div>
    );
  }

  if (!isFasterWhisper && !status.model_downloaded) {
    const modelInfo = models.find(m => m.name === config.asr.model_name);
    const modelName = modelDisplayName(modelInfo, config.asr.model_name);
    const modelSize = modelInfo ? modelSizeLabel(modelInfo.size_bytes) : 'unknown size';
    return (
      <div className="settings-root">
        <SettingsHeader />
        <div className="settings-body">
          <div className="download-block">
            <h2>Download Whisper model</h2>
            <p>
              voice-mcp-host runs transcription locally on your device.
              The {modelName} model is about {modelSize}.
            </p>
            {modelInfo?.description && <p className="model-help">{modelInfo.description}</p>}
            {downloading ? (
              <>
                <div className="progress-bar-wrap">
                  <div className="progress-bar-fill" style={{ width: `${downloadProgress}%` }} />
                </div>
                <span className="download-percent">{downloadProgress.toFixed(0)}%</span>
              </>
            ) : (
              <button className="primary" onClick={handleDownload}>
                Download {modelName}
              </button>
            )}
            {downloadError && <p className="inline-error">Error: {downloadError}</p>}
            <div className="form-row compact-row">
              <label>Model</label>
              <select
                value={config.asr.model_name}
                onChange={e => handleConfigChange({ asr: { ...config.asr, model_name: e.target.value } })}
                disabled={downloading}
              >
                {models.map(m => (
                  <option key={m.name} value={m.name}>
                    {modelOptionLabel(m)}
                  </option>
                ))}
              </select>
            </div>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="settings-root">
      <SettingsHeader />

      <div className="settings-body">
        <div className="section">
          <div className="form-row">
            <label>Hotkey</label>
            <input
              type="text"
              value={config.dictation.primary_hotkey}
              onChange={e => handleConfigChange({ dictation: { ...config.dictation, primary_hotkey: e.target.value } })}
              placeholder={IS_MACOS ? 'F5' : 'F3'}
              style={{ maxWidth: 120 }}
            />
          </div>
          <div className="form-row">
            <label>ASR backend</label>
            <select
              value={config.asr.backend}
              onChange={e => handleConfigChange({
                asr: { ...config.asr, backend: e.target.value as Config['asr']['backend'] },
              })}
            >
              <option value="whisper_cpp">whisper.cpp (Windows + Mac default)</option>
              <option value="faster_whisper">faster-whisper (NVIDIA performance)</option>
            </select>
          </div>
          <div className="form-row">
            <label>Model</label>
            {isFasterWhisper ? (
              <select
                value={config.asr.faster_whisper_model_name}
                onChange={e => handleConfigChange({
                  asr: { ...config.asr, faster_whisper_model_name: e.target.value },
                })}
              >
                {FASTER_WHISPER_MODELS.map(m => (
                  <option key={m} value={m}>{m}</option>
                ))}
              </select>
            ) : (
              <select
                value={config.asr.model_name}
                onChange={e => handleConfigChange({ asr: { ...config.asr, model_name: e.target.value } })}
              >
                {models.map(m => (
                  <option key={m.name} value={m.name}>
                    {modelOptionLabel(m)}
                  </option>
                ))}
              </select>
            )}
          </div>
          <div className="form-row">
            <label>Status</label>
            <span className={`status-badge ${modelReady ? 'ok' : 'warn'}`}>
              {modelReady ? 'Ready' : isFasterWhisper ? 'faster-whisper unavailable' : 'Model loading...'}
            </span>
          </div>
          <div className="form-row">
            <label>Engine</label>
            <span className={`status-badge ${isFasterWhisper || activeMatchesPreferred ? 'ok' : 'warn'}`}>
              {isFasterWhisper
                ? `${config.asr.faster_whisper_device.toUpperCase()} / ${selectedFasterWhisperCompute}`
                : activeAcceleration}
              {!isFasterWhisper && !activeMatchesPreferred ? ` fallback, preferred ${preferredAcceleration}` : ''}
            </span>
          </div>
          {isFasterWhisper && (
            <>
              <div className="form-row">
                <label>Device</label>
                <select
                  value={config.asr.faster_whisper_device}
                  onChange={e => {
                    const device = e.target.value;
                    handleConfigChange({
                      asr: {
                        ...config.asr,
                        faster_whisper_device: device,
                        faster_whisper_compute_type: defaultFasterWhisperCompute(device),
                      },
                    });
                  }}
                  style={{ maxWidth: 140 }}
                >
                  <option value="cuda">CUDA</option>
                  <option value="cpu">CPU</option>
                </select>
              </div>
              <div className="form-row">
                <label>Compute</label>
                <select
                  value={selectedFasterWhisperCompute}
                  onChange={e => handleConfigChange({
                    asr: { ...config.asr, faster_whisper_compute_type: e.target.value },
                  })}
                  style={{ maxWidth: 140 }}
                >
                  {fasterWhisperComputeOptionsForDevice.map(option => (
                    <option key={option} value={option}>{option}</option>
                  ))}
                </select>
              </div>
              <div className="form-row">
                <label />
                <p className="backend-note">
                  Uses the bundled faster-whisper runtime included with the app. No system Python install is required.
                </p>
              </div>
            </>
          )}
          {!isFasterWhisper && !activeMatchesPreferred && (
            <div className="form-row">
              <label />
              <div className="engine-upgrade-row">
                <button className="primary" onClick={handleDownloadEngine} disabled={downloading}>
                  {downloading ? `Downloading ${downloadProgress.toFixed(0)}%` : `Install ${preferredAcceleration} engine`}
                </button>
                <span>
                  Your NVIDIA GPU is detected, but the app is using the older CPU engine in the model folder.
                </span>
              </div>
            </div>
          )}
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
          {showAdvanced ? 'v' : '>'} Advanced
        </button>

        {showAdvanced && (
          <div className="section">
            <div className="section-title">Timing</div>
            <div className="form-row">
              <label>Paste delay (ms)</label>
              <input
                type="number"
                value={config.insertion.paste_delay_ms}
                min={0}
                max={2000}
                style={{ maxWidth: 90 }}
                onChange={e => handleConfigChange({ insertion: { ...config.insertion, paste_delay_ms: Number(e.target.value) } })}
              />
            </div>
            <div className="form-row">
              <label>Restore delay (ms)</label>
              <input
                type="number"
                value={config.insertion.restore_delay_ms}
                min={0}
                max={5000}
                style={{ maxWidth: 90 }}
                onChange={e => handleConfigChange({ insertion: { ...config.insertion, restore_delay_ms: Number(e.target.value) } })}
              />
            </div>
            <div className="form-row">
              <label>Min record (ms)</label>
              <input
                type="number"
                value={config.dictation.min_record_ms}
                min={100}
                max={5000}
                style={{ maxWidth: 90 }}
                onChange={e => handleConfigChange({ dictation: { ...config.dictation, min_record_ms: Number(e.target.value) } })}
              />
            </div>
            <div className="form-row">
              <label>Max record (s)</label>
              <input
                type="number"
                value={config.dictation.max_record_seconds}
                min={10}
                max={600}
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
        <span>voice-mcp-host {version} - MIT licensed</span>
        <span style={{ color: 'var(--text-muted)' }}>
          {config.dictation.primary_hotkey || (IS_MACOS ? 'F5' : 'F3')} to dictate
        </span>
      </div>
    </div>
  );
}
