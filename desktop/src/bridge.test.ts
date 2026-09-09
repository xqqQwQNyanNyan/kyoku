// @vitest-environment jsdom
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { Channel } from '@tauri-apps/api/core';
import type { AnalysisProgress, MigrationProgress, QuestionProgress } from './types';
import { bridge, errorMessage } from './bridge';

afterEach(clearMocks);

describe('桌面通信', () => {
  it('Mortal 分析传递事件进度，取消使用相同牌谱和请求编号', async () => {
    const progress = vi.fn();
    const commands: string[] = [];
    mockIPC((command, args) => {
      commands.push(command);
      if (command === 'analyze_game') {
        if (!args || !('onProgress' in args)) throw new Error('缺少分析进度通道');
        expect(args).toEqual({
          id: 7,
          player: 2,
          requestId: 'analysis-one',
          onProgress: expect.any(Channel),
        });
        (args.onProgress as Channel<AnalysisProgress>).onmessage({
          phase: 'analyzing',
          completed: 45,
          total: 120,
        });
        return [];
      }
      expect(args).toEqual({ id: 7, requestId: 'analysis-one' });
    });
    await bridge.analyze(7, 2, { requestId: 'analysis-one', onProgress: progress });
    await bridge.cancelAnalysis(7, 'analysis-one');
    expect(progress).toHaveBeenCalledWith({ phase: 'analyzing', completed: 45, total: 120 });
    expect(commands).toEqual(['analyze_game', 'cancel_analysis']);
  });
  it('数据迁移传递 Windows 原路径、编号和进度，取消对应同一次迁移', async () => {
    const directory = 'D:\\复盘资料\\Kyoku';
    const progress = vi.fn();
    const calls: string[] = [];
    mockIPC((command, args) => {
      calls.push(command);
      if (command === 'migrate_data') {
        if (!args || !('onProgress' in args)) throw new Error('缺少迁移进度通道');
        expect(args).toEqual({
          directory,
          requestId: 'migration-one',
          onProgress: expect.any(Channel),
        });
        (args.onProgress as Channel<MigrationProgress>).onmessage({
          copied_files: 1,
          total_files: 2,
          copied_bytes: 100,
          total_bytes: 200,
        });
        return { directory, available: true };
      }
      expect(args).toEqual({ requestId: 'migration-one' });
    });
    expect(await bridge.migrateData(directory, 'migration-one', progress)).toEqual({
      directory,
      available: true,
    });
    await bridge.cancelDataMigration('migration-one');
    expect(progress).toHaveBeenCalledOnce();
    expect(calls).toEqual(['migrate_data', 'cancel_data_migration']);
  });
  it('新问题、续聊和重试都传递本轮编号与进度通道，停止使用同一编号', async () => {
    const progress = vi.fn();
    const run = { requestId: 'request-one', onProgress: progress };
    const commands: string[] = [];
    mockIPC((command, args) => {
      commands.push(command);
      if (!args || !('requestId' in args)) throw new Error('缺少本轮编号');
      expect(args.requestId).toBe(run.requestId);
      if (command === 'cancel_question') {
        expect(args).toEqual({ id: 'session-one', requestId: run.requestId });
      } else {
        if (!('onProgress' in args)) throw new Error('缺少进度通道');
        expect(args.onProgress).toBeInstanceOf(Channel);
        (args.onProgress as Channel<QuestionProgress>).onmessage({ phase: 'model', request: 1 });
      }
    });
    await bridge.ask(1, 0, 2, 'session-one', '问题', '牌谱', run);
    await bridge.continueSession('session-one', '追问', run);
    await bridge.retrySession('session-one', 0, run);
    await bridge.cancelQuestion('session-one', run.requestId);
    expect(commands).toEqual(['ask', 'continue_session', 'retry_session', 'cancel_question']);
    expect(progress).toHaveBeenCalledTimes(3);
  });
  it('删除牌谱时传递已确认的会话集合，使用 Tauri 参数名', async () => {
    mockIPC((command, args) => {
      expect(command).toBe('delete_replay');
      expect(args).toEqual({ key: 'game-one', sessionIds: ['session-one'] });
      return { session_ids: ['session-one'], replay_deleted: true, error: null };
    });
    expect(await bridge.deleteReplay('game-one', ['session-one'])).toEqual({
      session_ids: ['session-one'],
      replay_deleted: true,
      error: null,
    });
  });
  it('保存浏览位置时使用 Tauri 命令参数名，位置字段保持 Rust 序列化格式', async () => {
    let saved: unknown;
    mockIPC((command, args) => {
      expect(command).toBe('set_session_position');
      if (!args || !('gameKey' in args)) {
        throw 'invalid args `gameKey` for command `set_session_position`: missing required key gameKey';
      }
      saved = args;
    });

    await bridge.setSessionPosition('session-one', 'game-one', { player: 1, event_index: 109 });

    expect(saved).toEqual({
      id: 'session-one',
      gameKey: 'game-one',
      position: { player: 1, event_index: 109 },
    });
  });
});

describe('错误提示', () => {
  it('保留 Tauri 返回的字符串错误，不误报启动方式', () => {
    const error =
      'invalid args `gameKey` for command `set_session_position`: missing required key gameKey';
    expect(errorMessage(error)).toBe(error);
  });

  it('保留结构化错误的消息和事件位置', () => {
    expect(errorMessage({ message: '保存浏览位置失败', event_index: 0 })).toBe(
      '保存浏览位置失败（事件 0）',
    );
  });

  it('保留 JavaScript 错误的消息', () => {
    expect(errorMessage(new Error('连接失败'))).toBe('连接失败');
  });

  it.each([undefined, null, '', '  ', {}])('未知错误 %j 不推断启动方式', (error) => {
    expect(errorMessage(error)).toBe('操作失败，请重试。');
  });
});
