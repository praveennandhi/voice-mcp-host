export interface Config {
  schema_version: number;
  dictation: DictationConfig;
  audio: AudioConfig;
  asr: AsrConfig;
  agent: AgentConfig;
  workspace: WorkspaceConfig;
  connectors: ConnectorsConfig;
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
  backend: 'whisper_cpp' | 'faster_whisper';
  model_name: string;
  faster_whisper_model_name: string;
  faster_whisper_device: string;
  faster_whisper_compute_type: string;
  faster_whisper_python_path: string | null;
  model_cache_dir: string | null;
}

export interface AgentConfig {
  enabled: boolean;
  trigger_word: string;
  provider: string;
  model: string;
  api_key: string | null;
  base_url: string;
  auto_replace_selection: boolean;
  speak_responses: boolean;
  tts_model: string;
  tts_voice: string;
}

export interface WorkspaceConfig {
  enabled: boolean;
  folder_path: string | null;
}

export interface ConnectorsConfig {
  todoist: TodoistConfig;
}

export interface TodoistConfig {
  enabled: boolean;
  api_token: string | null;
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
  engine_downloaded: boolean;
  gpu_detected: boolean;
  preferred_acceleration: string;
  active_acceleration: string | null;
  model_name: string;
  transcriber_ready: boolean;
  recorder_state: string;
  permissions: PermissionsStatus;
  workspace: WorkspaceStatus;
}

export interface WorkspaceStatus {
  enabled: boolean;
  configured: boolean;
  folder_path: string | null;
  exists: boolean;
}

export interface PermissionsStatus {
  microphone: PermissionState;
  accessibility: PermissionState;
}

export type PermissionState = 'Granted' | 'Denied' | 'Unknown' | 'NotRequired';

export interface AvailableModel {
  name: string;
  display_name: string;
  description: string;
  downloaded: boolean;
  size_bytes: number;
  recommended: boolean;
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

export interface AgentSessionTurn {
  role: 'user' | 'assistant' | string;
  content: string;
  mode: 'speak' | 'insert' | string | null;
}

export interface AgentChatResponse {
  messages: AgentSessionTurn[];
  mode: 'speak' | 'insert' | string;
  text: string;
}
