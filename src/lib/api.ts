import { invoke } from '@tauri-apps/api/core';
import type { AppStatus, AvailableModel, Config, PermissionsStatus } from './types';

export const api = {
  getConfig: () => invoke<Config>('get_config'),
  saveConfig: (config: Config) => invoke<void>('save_config', { config }),
  getStatus: () => invoke<AppStatus>('get_status'),
  downloadModel: (modelName: string) => invoke<void>('download_model', { modelName }),
  listAudioDevices: () => invoke<string[]>('list_audio_devices'),
  listModels: () => invoke<AvailableModel[]>('list_models'),
  checkPermissions: () => invoke<PermissionsStatus>('check_permissions'),
  requestAccessibilityPermission: () => invoke<boolean>('request_accessibility_permission'),
  openLogDir: () => invoke<void>('open_log_dir'),
  getVersion: () => invoke<string>('get_version'),
};
