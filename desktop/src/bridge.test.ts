// @vitest-environment jsdom
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { Channel } from '@tauri-apps/api/core';
import type { QuestionProgress } from './types';
import { bridge, errorMessage } from './bridge';

afterEach(clearMocks);

describe('桌面通信', () => {
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
