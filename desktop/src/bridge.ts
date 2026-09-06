import { invoke } from '@tauri-apps/api/core';
import type { Bridge } from './types';

export const bridge: Bridge = {
  getSettings: () => invoke('get_settings'),
  saveSettings: (input) => invoke('save_settings', { input }),
  testConnection: (input) => invoke('test_connection', { input }),
  runtimeStatus: (check) => invoke('runtime_status', { check }),
  importLog: (json) => invoke('import_log', { json }),
  importLink: (link) => invoke('import_link', { link }),
  analyze: (id, player) => invoke('analyze_game', { id, player }),
  ask: (id, player, event_index, conversation_id, text) =>
    invoke('ask', { question: { id, player, event_index, conversation_id, text } }),
};

export function errorMessage(error: unknown): string {
  if (error && typeof error === 'object' && 'message' in error) {
    const at =
      'event_index' in error && typeof error.event_index === 'number'
        ? `（事件 ${error.event_index}）`
        : '';
    return `${String(error.message)}${at}`;
  }
  return '操作失败，请重试。请通过桌面入口启动本应用。';
}
