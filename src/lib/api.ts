import { invoke } from '@tauri-apps/api/core';
import type { AgentChatResponse, AgentSessionTurn, AppStatus, AvailableModel, Config, PermissionsStatus } from './types';

export const api = {
  getConfig: () => invoke<Config>('get_config'),
  saveConfig: (config: Config) => invoke<void>('save_config', { config }),
  getStatus: () => invoke<AppStatus>('get_status'),
  downloadModel: (modelName: string) => invoke<void>('download_model', { modelName }),
  downloadEngine: () => invoke<void>('download_engine'),
  listAudioDevices: () => invoke<string[]>('list_audio_devices'),
  listModels: () => invoke<AvailableModel[]>('list_models'),
  checkPermissions: () => invoke<PermissionsStatus>('check_permissions'),
  requestAccessibilityPermission: () => invoke<boolean>('request_accessibility_permission'),
  openLogDir: () => invoke<void>('open_log_dir'),
  getAgentSession: () => invoke<AgentSessionTurn[]>('get_agent_session'),
  clearAgentSession: () => invoke<void>('clear_agent_session'),
  sendAgentChat: (message: string) => invoke<AgentChatResponse>('send_agent_chat', { message }),
  getVersion: () => invoke<string>('get_version'),
  quitApp: () => invoke<void>('quit_app'),
};
