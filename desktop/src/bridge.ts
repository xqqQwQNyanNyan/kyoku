import { Channel, invoke } from '@tauri-apps/api/core';
import type {
  AnalysisProgress,
  Bridge,
  MigrationProgress,
  QuestionProgress,
  QuestionRun,
} from './types';

function questionChannel(run: QuestionRun) {
  const channel = new Channel<QuestionProgress>();
  channel.onmessage = run.onProgress;
  return { requestId: run.requestId, onProgress: channel };
}

export const bridge: Bridge = {
  getStorage: () => invoke('get_storage'),
  chooseDataDirectory: () => invoke('choose_data_directory'),
  migrateData: (directory, requestId, onProgress) => {
    const channel = new Channel<MigrationProgress>();
    channel.onmessage = onProgress;
    return invoke('migrate_data', { directory, requestId, onProgress: channel });
  },
  cancelDataMigration: (requestId) => invoke('cancel_data_migration', { requestId }),
  getSettings: () => invoke('get_settings'),
  saveSettings: (input) => invoke('save_settings', { input }),
  testConnection: (input, onProgress) => {
    const channel = new Channel<QuestionProgress>();
    channel.onmessage = onProgress;
    return invoke('test_connection', { input, onProgress: channel });
  },
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
  analyze: (id, player, run) => {
    const channel = new Channel<AnalysisProgress>();
    channel.onmessage = run.onProgress;
    return invoke('analyze_game', { id, player, requestId: run.requestId, onProgress: channel });
  },
  cancelAnalysis: (id, requestId) => invoke('cancel_analysis', { id, requestId }),
  ask: (id, player, event_index, conversation_id, text, context_label, run) =>
    invoke('ask', {
      question: { id, player, event_index, conversation_id, text, context_label },
      ...questionChannel(run),
    }),
  cancelQuestion: (id, requestId) => invoke('cancel_question', { id, requestId }),
  listSessions: () => invoke('list_sessions'),
  getSession: (id) => invoke('get_session', { id }),
  deleteSession: (id) => invoke('delete_session', { id }),
  renameSession: (id, title) => invoke('rename_session', { id, title }),
  openSessionGame: (id) => invoke('open_session_game', { id }),
  setSessionPosition: (id, game_key, position) =>
    invoke('set_session_position', { id, gameKey: game_key, position }),
  continueSession: (id, text, run) =>
    invoke('continue_session', { id, text, ...questionChannel(run) }),
  retrySession: (id, turn, run) =>
    invoke('retry_session', { id, turn: turn ?? null, ...questionChannel(run) }),
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
