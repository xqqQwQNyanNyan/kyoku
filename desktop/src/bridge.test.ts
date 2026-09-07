// @vitest-environment jsdom
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks';
import { afterEach, describe, expect, it } from 'vitest';
import { bridge, errorMessage } from './bridge';

afterEach(clearMocks);

describe('桌面通信', () => {
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
