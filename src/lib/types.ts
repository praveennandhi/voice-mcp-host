export interface Config {
  schema_version: number;
  dictation: DictationConfig;
  audio: AudioConfig;
  asr: AsrConfig;
  insertion: InsertionConfig;
  privacy: PrivacyConfig;
}

export interface DictationConfig {
  primary_hotkey: string;
  language: string;
  min_record_ms: number;
  max_record_seconds: number;
}

export interface AudioConfig {
  input_device_id: string | null;
  samplerate: number;
}

export interface AsrConfig {
  model_name: string;
  model_cache_dir: string | null;
}

export interface InsertionConfig {
  paste_delay_ms: number;
  restore_delay_ms: number;
  copy_to_clipboard_on_failure: boolean;
}

export interface PrivacyConfig {
  verbose_transcript_logging: boolean;
}

export interface AppStatus {
  model_downloaded: boolean;
  model_name: string;
  transcriber_ready: boolean;
  recorder_state: string;
  permissions: PermissionsStatus;
}

export interface PermissionsStatus {
  microphone: PermissionState;
  accessibility: PermissionState;
}

export type PermissionState = 'Granted' | 'Denied' | 'Unknown' | 'NotRequired';

export interface AvailableModel {
  name: string;
  downloaded: boolean;
  size_bytes: number;
}

export interface DownloadProgress {
  model: string;
  downloaded: number;
  total: number;
  percent: number;
}

export interface OverlayState {
  state: 'idle' | 'recording' | 'transcribing' | 'pasting' | 'ready' | 'error';
  title: string;
  subtitle: string;
  hide_after_ms?: number;
}
