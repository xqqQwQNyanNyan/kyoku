// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import App from './App';
import type { Bridge, Decision, Frame, Replay } from './types';

const first: Frame = {
  event_index: 1,
  round: '东一局',
  honba: 0,
  dealer: 0,
  riichi_sticks: 0,
  remaining_draws: 70,
  dora_indicators: ['3p'],
  active_player: null,
  settled: false,
  drawn: null,
  players: Array.from({ length: 4 }, () => ({
    score: 25000,
    riichi: false,
    concealed: ['1m', '2m', '3m', '4p', '5p', '6p', '7s', '8s', '9s', 'E', 'E', 'P', 'P'],
    melds: [],
    discards: [],
  })),
  event: { kind: 'start_kyoku', actor: null, target: null, tile: null },
};
const replay: Replay = {
  id: 1,
  names: ['自己', '下家', '对家', '上家'],
  rounds: [{ label: '东一局 · 0 本场', frame_index: 0 }],
  frames: [
    first,
    { ...first, event_index: 2, event: { kind: 'tsumo', actor: 0, target: null, tile: '1p' } },
    { ...first, event_index: 3, event: { kind: 'dahai', actor: 0, target: null, tile: '1p' } },
    { ...first, event_index: 4, event: { kind: 'tsumo', actor: 1, target: null, tile: 'C' } },
  ],
};
const decision: Decision = {
  event_index: 2,
  turn: 1,
  actual: { kind: 'taken', action: { type: 'dahai', pai: '1m' } },
  evidence: {
    event_index: 2,
    player: 0,
    discards: [],
    mortal: {
      model: { version: 4, tag: 'test', sha256: 'test-sha' },
      decision: {
        recommended: { type: 'dahai', pai: '2m' },
        candidates: [
          { action: { kind: 'discard', tile: '1m' }, q_value: 2 },
          { action: { kind: 'discard', tile: '2m' }, q_value: 1 },
        ],
        kan_candidates: [],
        shanten: 1,
        at_furiten: false,
      },
    },
  },
};

function api(): Bridge {
  return {
    importLog: vi.fn().mockResolvedValue(replay),
    analyze: vi.fn().mockResolvedValue([decision]),
    ask: vi.fn().mockResolvedValue('【计算】测试回答'),
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}

async function load(bridge: Bridge) {
  render(<App api={bridge} />);
  const file = new File(['{}'], 'test.json', { type: 'application/json' });
  Object.defineProperty(file, 'text', { value: () => Promise.resolve('{}') });
  await userEvent.upload(screen.getByLabelText('选择天凤牌谱文件'), file);
  await screen.findByLabelText('牌谱进度');
}

beforeEach(() => {
  Element.prototype.scrollIntoView = vi.fn();
});
afterEach(cleanup);

describe('完整回放与问答边界', () => {
  it('切换局面清空对话后，侧栏回到引导内容顶部', async () => {
    await load(api());
    const messages = screen.getByRole('log', { name: '复盘对话' });
    messages.scrollTop = 200;
    await userEvent.click(screen.getByLabelText('下一事件'));
    expect(messages.scrollTop).toBe(0);
    expect(screen.getByText('这一步，你在想什么？')).toBeTruthy();
  });

  it('未运行 Mortal 也能逐事件跳转，并且默认隐藏对手摸牌', async () => {
    const bridge = api();
    await load(bridge);
    expect((screen.getByLabelText('上一事件') as HTMLButtonElement).disabled).toBe(true);
    fireEvent.change(screen.getByLabelText('牌谱进度'), { target: { value: '3' } });
    expect(screen.getByTestId('event-caption').textContent).toBe('下家 · 摸牌');
    expect((screen.getByLabelText('下一事件') as HTMLButtonElement).disabled).toBe(true);
    await userEvent.click(screen.getByLabelText('显示全部手牌'));
    expect(screen.getByTestId('event-caption').textContent).toBe('下家 · 摸牌 中');
    expect(bridge.analyze).not.toHaveBeenCalled();
    expect(bridge.ask).not.toHaveBeenCalled();
  });

  it('最终推荐独立于 Q 值排序，输入框方向键不会跳转局面', async () => {
    const bridge = api();
    await load(bridge);
    await userEvent.click(screen.getAllByRole('button', { name: '分析此玩家' })[0]);
    await screen.findByRole('button', { name: '分析已完成' });
    await userEvent.click(screen.getByRole('button', { name: '下一决策 ›' }));
    expect(document.querySelector('.recommendation strong')?.textContent).toBe('切 二万');
    const input = screen.getByLabelText('复盘问题');
    await userEvent.type(input, '比较一下');
    fireEvent.keyDown(input, { code: 'ArrowRight' });
    expect(screen.getByTestId('event-caption').textContent).toBe('自己 · 摸牌 一筒');
  });

  it('切换事件后忽略旧回答，切回也使用新的问答会话', async () => {
    const bridge = api();
    const pending = deferred<string>();
    vi.mocked(bridge.ask).mockReturnValueOnce(pending.promise);
    await load(bridge);
    await userEvent.click(screen.getAllByRole('button', { name: '分析此玩家' })[0]);
    await screen.findByRole('button', { name: '分析已完成' });
    await userEvent.click(screen.getByRole('button', { name: '下一决策 ›' }));
    await userEvent.click(screen.getByText('这里的几个选择差在哪里？'));
    await waitFor(() => expect(bridge.ask).toHaveBeenCalledOnce());
    const oldConversation = vi.mocked(bridge.ask).mock.calls[0][3];
    await userEvent.click(screen.getByLabelText('下一事件'));
    await act(async () => {
      pending.resolve('不应该出现的旧回答');
    });
    expect(screen.queryByText('不应该出现的旧回答')).toBeNull();
    await userEvent.click(screen.getByLabelText('上一事件'));
    await userEvent.click(screen.getByText('这里的几个选择差在哪里？'));
    await screen.findByText('【计算】测试回答');
    expect(vi.mocked(bridge.ask).mock.calls[1][3]).not.toBe(oldConversation);
  });

  it('重新导入后，旧牌谱的分析不能污染新牌谱', async () => {
    const bridge = api();
    const pending = deferred<Decision[]>();
    vi.mocked(bridge.analyze).mockReturnValueOnce(pending.promise);
    vi.mocked(bridge.importLog)
      .mockResolvedValueOnce(replay)
      .mockResolvedValueOnce({ ...replay, id: 2 });
    await load(bridge);
    await userEvent.click(screen.getAllByRole('button', { name: '分析此玩家' })[0]);
    const file = new File(['{}'], 'second.json', { type: 'application/json' });
    Object.defineProperty(file, 'text', { value: () => Promise.resolve('{}') });
    await userEvent.upload(screen.getByLabelText('选择天凤牌谱文件'), file);
    await screen.findByText('second.json');
    await act(async () => {
      pending.resolve([decision]);
    });
    expect(screen.queryByText('分析已完成')).toBeNull();
    expect((screen.getByRole('button', { name: '下一决策 ›' }) as HTMLButtonElement).disabled).toBe(
      true,
    );
  });

  it('分析失败后可重试，原有回放局面保持不变', async () => {
    const bridge = api();
    vi.mocked(bridge.analyze).mockRejectedValueOnce({
      message: '测试：模型未准备',
      code: 'analysis',
    });
    await load(bridge);
    await userEvent.click(screen.getByLabelText('下一事件'));
    await userEvent.click(screen.getAllByRole('button', { name: '分析此玩家' })[0]);
    await screen.findByText('测试：模型未准备');
    expect(screen.getByTestId('event-caption').textContent).toBe('自己 · 摸牌 一筒');
    await userEvent.click(screen.getAllByRole('button', { name: '分析此玩家' })[0]);
    await screen.findByRole('button', { name: '分析已完成' });
  });
});
