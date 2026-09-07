import { invoke } from '@tauri-apps/api/core';
import type { Bridge } from './types';

export const bridge: Bridge = {
  getSettings: () => invoke('get_settings'),
  saveSettings: (input) => invoke('save_settings', { input }),
  testConnection: (input) => invoke('test_connection', { input }),
  runtimeStatus: (check) => invoke('runtime_status', { check }),
  importLog: (json, name) => invoke('import_log', { json, name }),
  importLink: (link) => invoke('import_link', { link }),
  listReplays: () => invoke('list_replays'),
  previewReplayDeletion: (key) => invoke('preview_replay_deletion', { key }),
  deleteReplay: (key, sessionIds) => invoke('delete_replay', { key, sessionIds }),
  renameReplay: (key, name) => invoke('rename_replay', { key, name }),
  openReplay: (key) => invoke('open_replay', { key }),
  openDataDirectory: () => invoke('open_data_directory'),
  majsoulStatus: () => invoke('majsoul_status'),
  loginMajsoul: (input) => invoke('login_majsoul', { input }),
  logoutMajsoul: () => invoke('logout_majsoul'),
  analyze: (id, player) => invoke('analyze_game', { id, player }),
  ask: (id, player, event_index, conversation_id, text, context_label) =>
    invoke('ask', { question: { id, player, event_index, conversation_id, text, context_label } }),
  listSessions: () => invoke('list_sessions'),
  getSession: (id) => invoke('get_session', { id }),
  deleteSession: (id) => invoke('delete_session', { id }),
  renameSession: (id, title) => invoke('rename_session', { id, title }),
  openSessionGame: (id) => invoke('open_session_game', { id }),
  setSessionPosition: (id, game_key, position) =>
    invoke('set_session_position', { id, gameKey: game_key, position }),
  continueSession: (id, text) => invoke('continue_session', { id, text }),
  retrySession: (id, turn) => invoke('retry_session', { id, turn: turn ?? null }),
  importSession: (json) => invoke('import_session', { json }),
  exportSession: (id) => invoke('export_session', { id }),
};

export function errorMessage(error: unknown): string {
  if (typeof error === 'string' && error.trim()) return error;
  if (error && typeof error === 'object' && 'message' in error) {
    const at =
      'event_index' in error && typeof error.event_index === 'number'
        ? `（事件 ${error.event_index}）`
        : '';
    return `${String(error.message)}${at}`;
  }
  return '操作失败，请重试。';
}
