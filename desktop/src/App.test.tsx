// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import App from './App';
import type { Bridge, Decision, Frame, Replay, SessionView } from './types';
import { examples } from './examples';
import { sessionTitle } from './display';

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
  game_key: 'game-one',
  mortal_supported: true,
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

function savedSession(
  id: string,
  answer = '【计算】测试回答',
  question = '比较这里的候选切牌，说明向听、进张和 Mortal 的倾向。',
): SessionView {
  return {
    id,
    game: { key: replay.game_key },
    position: { player: 0, event_index: 2 },
    title: question,
    context_label: 'test.json · 自己 · 东一局 · 第 1 手 · G2',
    created_at: 1,
    updated_at: 1,
    busy: false,
    pending_question: null,
    archive: {
      version: 1,
      instructions: '固定局面提示词',
      tools: [],
      endpoint: 'https://example.com/responses',
      model: 'test',
      evidence: decision.evidence,
      history: [],
      turns: [
        {
          question,
          answer,
          error: null,
          evidence: decision.evidence,
          trace: [
            { kind: 'request', input: [{ role: 'user', content: question }] },
            {
              kind: 'tool',
              name: 'get_review',
              arguments: '{}',
              result: { ok: true, review: decision.evidence },
            },
          ],
        },
      ],
    },
  };
}

function api(): Bridge {
  const stored = new Map<string, SessionView>();
  return {
    getSettings: vi.fn().mockResolvedValue({
      endpoint: 'https://api.openai.com/v1/responses',
      model: '',
      has_api_key: false,
      saved: false,
    }),
    saveSettings: vi.fn(),
    testConnection: vi.fn(),
    runtimeStatus: vi
      .fn()
      .mockResolvedValue({ bundled: true, available: true, checked: false, model: 'Mortal V4' }),
    importLog: vi.fn().mockResolvedValue(replay),
    listReplays: vi.fn().mockResolvedValue({ replays: [], warnings: [], directory: '/data/kyoku' }),
    renameReplay: vi.fn(),
    openReplay: vi.fn().mockResolvedValue(replay),
    openDataDirectory: vi.fn().mockResolvedValue(undefined),
    importLink: vi.fn().mockResolvedValue(replay),
    majsoulStatus: vi.fn().mockResolvedValue(false),
    loginMajsoul: vi.fn().mockResolvedValue(undefined),
    logoutMajsoul: vi.fn().mockResolvedValue(undefined),
    analyze: vi.fn().mockResolvedValue([decision]),
    ask: vi.fn().mockImplementation(async (_game, player, event, id, text) => {
      const original = stored.get(id);
      const evidence = { ...decision.evidence, player, event_index: event };
      const result = savedSession(id, original ? '追问回答' : '【计算】测试回答', text);
      result.archive.evidence = evidence;
      result.archive.turns[0].evidence = evidence;
      result.position = { player, event_index: event };
      if (original) {
        result.title = original.title;
        result.archive.turns = [...original.archive.turns, ...result.archive.turns];
        result.updated_at = original.updated_at + 1;
      }
      stored.set(id, result);
      return result;
    }),
    continueSession: vi.fn().mockImplementation(async (id, text) => {
      const original = stored.get(id) ?? savedSession(id);
      const result = {
        ...original,
        archive: {
          ...original.archive,
          turns: [
            ...original.archive.turns,
            { question: text, answer: '追问回答', error: null, trace: [] },
          ],
        },
      };
      stored.set(id, result);
      return result;
    }),
    listSessions: vi.fn().mockImplementation(async () => ({
      sessions: [...stored.values()].map((s) => ({
        id: s.id,
        game_key: s.game?.key,
        title: s.title,
        context_label: s.context_label,
        updated_at: s.updated_at,
        busy: false,
        interrupted: false,
      })),
      warnings: [],
    })),
    renameSession: vi.fn().mockImplementation(async (id, title) => {
      const doc = { ...stored.get(id)!, title, updated_at: stored.get(id)!.updated_at + 1 };
      stored.set(id, doc);
      return doc;
    }),
    getSession: vi.fn().mockImplementation(async (id) => {
      if (!stored.has(id)) throw new Error('missing');
      return stored.get(id);
    }),
    openSessionGame: vi
      .fn()
      .mockResolvedValue({ replay, name: 'test.json', position: { player: 0, event_index: 2 } }),
    setSessionPosition: vi.fn().mockResolvedValue(undefined),
    importSession: vi.fn().mockImplementation(async () => {
      const doc = savedSession('imported', '从 JSON 恢复的回答');
      stored.set(doc.id, doc);
      return doc;
    }),
    retrySession: vi.fn(),
    exportSession: vi.fn().mockResolvedValue('/Downloads/Kyoku-session-test.json'),
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

async function openChat() {
  await userEvent.click(screen.getByRole('tab', { name: '一起复盘' }));
}

async function openLink() {
  await userEvent.click(screen.getByRole('button', { name: '＋ 导入牌谱' }));
  await userEvent.click(screen.getByRole('button', { name: /天凤.*雀魂链接/ }));
}

beforeEach(() => {
  HTMLDialogElement.prototype.showModal = function () {
    this.open = true;
  };
  Element.prototype.scrollTo = vi.fn();
});
afterEach(cleanup);

describe('复盘工具标签', () => {
  it('东风场保留回放，但不能启动 Mortal 分析', async () => {
    const bridge = api();
    vi.mocked(bridge.importLog).mockResolvedValue({ ...replay, mortal_supported: false });
    await load(bridge);
    const buttons = screen.getAllByRole('button', { name: '分析此玩家' });
    for (const button of buttons) {
      expect((button as HTMLButtonElement).disabled).toBe(true);
      await userEvent.click(button);
    }
    expect(screen.getByText('东风场可回放，Mortal 分析目前仅支持半庄。')).toBeTruthy();
    expect(bridge.analyze).not.toHaveBeenCalled();
    expect(screen.getByLabelText('牌谱进度')).toBeTruthy();
  });
  it('默认只显示分析，键盘切换标签不会推进回放', async () => {
    await load(api());
    expect(screen.getAllByRole('tabpanel')).toHaveLength(1);
    expect(screen.getByRole('tabpanel', { name: '决策分析' })).toBeTruthy();
    expect(screen.queryByRole('log', { name: '复盘对话' })).toBeNull();
    const analysis = screen.getByRole('tab', { name: '决策分析' });
    analysis.focus();
    await userEvent.keyboard('{ArrowRight}');
    expect(screen.getByRole('tab', { name: '一起复盘' }).getAttribute('aria-selected')).toBe(
      'true',
    );
    expect(screen.getByRole('tabpanel', { name: '一起复盘' })).toBeTruthy();
    expect(screen.getAllByRole('tabpanel')).toHaveLength(1);
    expect((screen.getByLabelText('牌谱进度') as HTMLInputElement).value).toBe('0');
    await userEvent.keyboard('{Home}');
    expect(document.activeElement).toBe(analysis);
    expect(screen.getByRole('tabpanel', { name: '决策分析' })).toBeTruthy();
  });

  it('切换标签保留分析、回答、草稿和阅读位置', async () => {
    const bridge = api();
    await load(bridge);
    await userEvent.click(screen.getAllByRole('button', { name: '分析此玩家' })[0]);
    await screen.findByRole('button', { name: '分析已完成' });
    await userEvent.click(screen.getByRole('button', { name: '下一决策 ›' }));
    await openChat();
    await userEvent.click(screen.getByText('这里的几个选择差在哪里？'));
    await screen.findByText('【计算】测试回答');
    await userEvent.type(screen.getByLabelText('复盘问题'), '还没发送的问题');
    const messages = screen.getByRole('log', { name: '复盘对话' });
    messages.scrollTop = 120;
    const scrollTo = vi.fn();
    messages.scrollTo = scrollTo;
    await userEvent.click(screen.getByRole('tab', { name: '决策分析' }));
    expect(screen.queryByRole('log', { name: '复盘对话' })).toBeNull();
    expect(screen.getByRole('tabpanel', { name: '决策分析' }).textContent).toContain('切 二万');
    await openChat();
    expect(screen.getByText('【计算】测试回答')).toBeTruthy();
    expect((screen.getByLabelText('复盘问题') as HTMLTextAreaElement).value).toBe('还没发送的问题');
    expect(messages.scrollTop).toBe(120);
    expect(scrollTo).not.toHaveBeenCalled();
    expect(bridge.analyze).toHaveBeenCalledOnce();
    expect(bridge.ask).toHaveBeenCalledOnce();
  });

  it('后台收到回答后，打开聊天会滚动到新消息', async () => {
    const bridge = api();
    const pending = deferred<SessionView>();
    vi.mocked(bridge.ask).mockReturnValueOnce(pending.promise);
    await load(bridge);
    await userEvent.click(screen.getAllByRole('button', { name: '分析此玩家' })[0]);
    await screen.findByRole('button', { name: '分析已完成' });
    await userEvent.click(screen.getByRole('button', { name: '下一决策 ›' }));
    await openChat();
    await userEvent.click(screen.getByText('这里的几个选择差在哪里？'));
    await userEvent.click(screen.getByRole('tab', { name: '决策分析' }));
    const scrollTo = vi.mocked(Element.prototype.scrollTo);
    scrollTo.mockClear();
    await act(async () =>
      pending.resolve(savedSession(vi.mocked(bridge.ask).mock.calls[0][3], '后台收到的回答')),
    );
    expect(scrollTo).not.toHaveBeenCalled();
    await openChat();
    expect(screen.getByText('后台收到的回答')).toBeTruthy();
    expect(scrollTo).toHaveBeenCalledOnce();
  });
});

describe('复盘玩家选择', () => {
  it('键盘选择玩家不推进回放，确认后更新复盘视角', async () => {
    await load(api());
    const picker = screen.getByRole('combobox', { name: '复盘玩家' });
    await userEvent.click(picker);
    expect(screen.getByRole('option', { name: '自己' }).getAttribute('aria-selected')).toBe('true');
    await userEvent.keyboard('{ArrowDown}{Enter}');
    expect(picker.textContent).toContain('下家');
    expect(screen.queryByRole('listbox')).toBeNull();
    expect((screen.getByLabelText('牌谱进度') as HTMLInputElement).value).toBe('0');
    expect(screen.getByRole('group', { name: '下家的点况' })).toBeTruthy();
  });
});

describe('统一导入入口', () => {
  it('主页和顶栏打开同一个来源选择，关闭后保留当前牌谱', async () => {
    const bridge = api();
    render(<App api={bridge} />);
    expect(screen.queryByRole('button', { name: '链接导入' })).toBeNull();
    expect(screen.queryByRole('button', { name: '粘贴天凤 / 雀魂链接' })).toBeNull();
    await userEvent.click(screen.getByRole('button', { name: '导入牌谱 ↗' }));
    expect(screen.getByRole('dialog', { name: '导入牌谱' })).toBeTruthy();
    expect(screen.getByRole('button', { name: /本地文件/ })).toBeTruthy();
    expect(screen.getByRole('button', { name: /天凤.*雀魂链接/ })).toBeTruthy();
    await userEvent.click(screen.getByRole('button', { name: /示例牌谱/ }));
    await userEvent.click(screen.getByRole('button', { name: '导入示例' }));
    await screen.findByLabelText('牌谱进度');
    await userEvent.click(screen.getByLabelText('下一事件'));
    await userEvent.click(screen.getByRole('button', { name: '＋ 导入牌谱' }));
    const dialog = screen.getByRole('dialog', { name: '导入牌谱' });
    fireEvent.keyDown(dialog, { code: 'ArrowRight' });
    expect((screen.getByLabelText('牌谱进度') as HTMLInputElement).value).toBe('1');
    fireEvent(dialog, new Event('cancel', { bubbles: false, cancelable: true }));
    expect(screen.queryByRole('dialog')).toBeNull();
    expect((screen.getByLabelText('牌谱进度') as HTMLInputElement).value).toBe('1');
  });

  it('选择本地文件后沿用文件导入，成功时关闭弹窗', async () => {
    const bridge = api();
    render(<App api={bridge} />);
    await userEvent.click(screen.getByRole('button', { name: '＋ 导入牌谱' }));
    const fileInput = screen.getByLabelText('选择天凤牌谱文件');
    const choose = vi.spyOn(fileInput, 'click');
    await userEvent.click(screen.getByRole('button', { name: /本地文件/ }));
    expect(choose).toHaveBeenCalledOnce();
    const file = new File(['{}'], 'local.json', { type: 'application/json' });
    Object.defineProperty(file, 'text', { value: () => Promise.resolve('{}') });
    await userEvent.upload(fileInput, file);
    await screen.findByLabelText('牌谱进度');
    expect(bridge.importLog).toHaveBeenCalledExactlyOnceWith('{}', 'local.json');
    expect(screen.getByText('local')).toBeTruthy();
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('可选择不同内置样本，失败后保留选择并支持重试', async () => {
    const bridge = api();
    vi.mocked(bridge.importLog).mockRejectedValueOnce({ message: '示例导入失败' });
    render(<App api={bridge} />);
    await userEvent.click(screen.getByRole('button', { name: '＋ 导入牌谱' }));
    await userEvent.click(screen.getByRole('button', { name: /示例牌谱/ }));
    const select = screen.getByRole('combobox', { name: '选择示例牌谱' });
    await userEvent.click(select);
    expect(screen.getAllByRole('option')).toHaveLength(21);
    await userEvent.click(screen.getByRole('option', { name: '岭上摸牌' }));
    await userEvent.click(screen.getByRole('button', { name: '导入示例' }));
    await screen.findByRole('alert');
    expect(select.textContent).toContain('岭上摸牌');
    expect(screen.getByRole('dialog')).toBeTruthy();
    expect(bridge.importLink).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole('button', { name: '导入示例' }));
    await screen.findByLabelText('牌谱进度');
    expect(bridge.importLog).toHaveBeenLastCalledWith(
      examples.find((e) => e.filename === 'rinshan.json')!.json,
      'rinshan.json',
    );
    expect(screen.getByText('rinshan')).toBeTruthy();
    expect(screen.queryByRole('dialog')).toBeNull();
    await userEvent.click(screen.getByRole('button', { name: '＋ 导入牌谱' }));
    await userEvent.click(screen.getByRole('button', { name: /示例牌谱/ }));
    await userEvent.click(screen.getByRole('combobox', { name: '选择示例牌谱' }));
    await userEvent.click(screen.getByRole('option', { name: '双响' }));
    await userEvent.click(screen.getByRole('button', { name: '导入示例' }));
    await screen.findByText('double_ron');
    expect(bridge.importLog).toHaveBeenLastCalledWith(
      examples.find((e) => e.filename === 'double_ron.json')!.json,
      'double_ron.json',
    );
  });
});

describe('天凤 / 雀魂链接导入', () => {
  it.each(['雀魂牌谱:', '雀魂牌谱：', '雀魂牌譜:', '雀魂牌譜 ： '])(
    '直接粘贴带 %s 前缀的分享文本，识别登录入口并导入原牌谱编号',
    async (prefix) => {
      const bridge = api();
      const link =
        'https://game.maj-soul.com/1/?paipu=260907-d7e9a96e-3582-48c5-9858-4d01e6beb2ba_a216567045';
      render(<App api={bridge} />);
      await openLink();
      await userEvent.click(screen.getByLabelText('牌谱链接'));
      await userEvent.paste(`  ${prefix}${link}  `);
      expect(screen.getByText('账号风险提示')).toBeTruthy();
      await userEvent.type(screen.getByLabelText('雀魂账号'), 'test-user');
      await userEvent.type(screen.getByLabelText('雀魂密码'), 'test-password');
      await userEvent.click(screen.getByRole('checkbox', { name: /我已了解风险/ }));
      await userEvent.click(screen.getByRole('button', { name: '登录并导入' }));
      await screen.findByLabelText('牌谱进度');
      expect(bridge.loginMajsoul).toHaveBeenCalledOnce();
      expect(bridge.importLink).toHaveBeenCalledExactlyOnceWith(link);
    },
  );

  it('已登录的雀魂账号可直接导入分享链接，不再次索要密码', async () => {
    const bridge = api();
    vi.mocked(bridge.majsoulStatus).mockResolvedValue(true);
    const link =
      'https://game.maj-soul.com/1/?paipu=200515-cfbe0120-c92c-44ad-bdfc-ebfef3a33a10_a89702544';
    render(<App api={bridge} />);
    await openLink();
    await userEvent.type(screen.getByLabelText('牌谱链接'), link + '{Enter}');
    await screen.findByLabelText('牌谱进度');
    expect(bridge.importLink).toHaveBeenCalledExactlyOnceWith(link);
    expect(bridge.importLog).not.toHaveBeenCalled();
    expect(bridge.loginMajsoul).not.toHaveBeenCalled();
  });

  it('首次雀魂导入必须确认风险，密码只提交给登录命令且提交后清空', async () => {
    const bridge = api();
    const login = deferred<void>();
    vi.mocked(bridge.loginMajsoul).mockReturnValue(login.promise);
    const link = 'https://game.maj-soul.com/1/?paipu=200515-cfbe0120-c92c-44ad-bdfc-ebfef3a33a10';
    render(<App api={bridge} />);
    await openLink();
    await userEvent.type(screen.getByLabelText('牌谱链接'), link);
    expect(screen.getByText('账号风险提示')).toBeTruthy();
    await userEvent.type(screen.getByLabelText('雀魂账号'), 'test-user');
    await userEvent.type(screen.getByLabelText('雀魂密码'), 'secret-password');
    const button = screen.getByRole('button', { name: '登录并导入' });
    expect((button as HTMLButtonElement).disabled).toBe(true);
    fireEvent.submit(screen.getByRole('form', { name: '链接导入' }));
    expect(bridge.loginMajsoul).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole('checkbox', { name: /我已了解风险/ }));
    await userEvent.click(button);
    expect(bridge.loginMajsoul).toHaveBeenCalledExactlyOnceWith({
      username: 'test-user',
      password: 'secret-password',
      accept_risk: true,
    });
    expect((screen.getByLabelText('雀魂密码') as HTMLInputElement).value).toBe('');
    expect(bridge.importLink).not.toHaveBeenCalled();
    fireEvent.submit(screen.getByRole('form', { name: '链接导入' }));
    fireEvent.drop(screen.getByRole('dialog'), {
      dataTransfer: { files: [new File(['{}'], 'another.json')] },
    });
    expect(bridge.importLog).not.toHaveBeenCalled();
    expect(bridge.loginMajsoul).toHaveBeenCalledTimes(1);
    await act(async () => login.resolve());
    await screen.findByLabelText('牌谱进度');
    expect(bridge.importLink).toHaveBeenCalledExactlyOnceWith(link);
  });

  it('登录失败保留链接且清空密码，不开始下载', async () => {
    const bridge = api();
    vi.mocked(bridge.loginMajsoul).mockRejectedValue({ message: '雀魂登录失败，请检查账号密码' });
    render(<App api={bridge} />);
    await openLink();
    const link = 'https://game.maj-soul.com/1/?paipu=200515-cfbe0120-c92c-44ad-bdfc-ebfef3a33a10';
    await userEvent.type(screen.getByLabelText('牌谱链接'), link);
    await userEvent.type(screen.getByLabelText('雀魂账号'), 'test-user');
    await userEvent.type(screen.getByLabelText('雀魂密码'), 'wrong-password');
    await userEvent.click(screen.getByRole('checkbox', { name: /我已了解风险/ }));
    await userEvent.click(screen.getByRole('button', { name: '登录并导入' }));
    expect((await screen.findByRole('alert')).textContent).toContain('雀魂登录失败');
    expect((screen.getByLabelText('雀魂密码') as HTMLInputElement).value).toBe('');
    expect((screen.getByLabelText('牌谱链接') as HTMLInputElement).value).toBe(link);
    expect(bridge.importLink).not.toHaveBeenCalled();
  });

  it('退出雀魂后再次导入需要重新填写凭据并确认风险', async () => {
    const bridge = api();
    vi.mocked(bridge.majsoulStatus).mockResolvedValue(true);
    render(<App api={bridge} />);
    await openLink();
    await userEvent.type(
      screen.getByLabelText('牌谱链接'),
      'https://game.maj-soul.com/1/?paipu=200515-cfbe0120-c92c-44ad-bdfc-ebfef3a33a10',
    );
    await userEvent.click(screen.getByRole('button', { name: '退出登录' }));
    expect(bridge.logoutMajsoul).toHaveBeenCalledOnce();
    await screen.findByLabelText('雀魂密码');
    expect(
      (screen.getByRole('checkbox', { name: /我已了解风险/ }) as HTMLInputElement).checked,
    ).toBe(false);
    expect((screen.getByRole('button', { name: '登录并导入' }) as HTMLButtonElement).disabled).toBe(
      true,
    );
  });

  const link = 'https://tenhou.net/0/?log=2023010100gm-00a9-0000-123456ab&tw=2';

  it('空输入不能提交，粘贴链接后可按 Enter 导入并回放', async () => {
    const bridge = api();
    render(<App api={bridge} />);
    await openLink();
    expect((screen.getByRole('button', { name: '导入链接' }) as HTMLButtonElement).disabled).toBe(
      true,
    );
    await userEvent.type(screen.getByLabelText('牌谱链接'), `  ${link}  {Enter}`);
    await screen.findByLabelText('牌谱进度');
    expect(bridge.importLink).toHaveBeenCalledExactlyOnceWith(link);
    expect(bridge.importLog).not.toHaveBeenCalled();
    expect(screen.queryByRole('form', { name: '链接导入' })).toBeNull();
    expect(screen.getByText(link)).toBeTruthy();
  });

  it('下载中不能重复提交或通过拖放启动另一份导入', async () => {
    const bridge = api();
    const pending = deferred<Replay>();
    vi.mocked(bridge.importLink).mockReturnValueOnce(pending.promise);
    render(<App api={bridge} />);
    await openLink();
    await userEvent.type(screen.getByLabelText('牌谱链接'), link);
    const form = screen.getByRole('form', { name: '链接导入' });
    fireEvent.submit(form);
    fireEvent.submit(form);
    fireEvent.drop(form, { dataTransfer: { files: [new File(['{}'], 'another.json')] } });
    expect(bridge.importLink).toHaveBeenCalledOnce();
    expect(bridge.importLog).not.toHaveBeenCalled();
    expect((screen.getByLabelText('牌谱链接') as HTMLInputElement).disabled).toBe(true);
    await act(async () => pending.resolve(replay));
    await screen.findByLabelText('牌谱进度');
  });

  it('下载失败保留链接与当前局面，重试成功后切换到新牌谱', async () => {
    const bridge = api();
    vi.mocked(bridge.importLink)
      .mockRejectedValueOnce({ code: 'download_timeout', message: '下载天凤牌谱超时，请重试' })
      .mockResolvedValueOnce({ ...replay, id: 2, game_key: 'game-two' });
    await load(bridge);
    await userEvent.click(screen.getAllByRole('button', { name: '分析此玩家' })[0]);
    await screen.findByRole('button', { name: '分析已完成' });
    await userEvent.click(screen.getByRole('button', { name: '下一决策 ›' }));
    await openChat();
    await userEvent.click(screen.getByText('这里的几个选择差在哪里？'));
    await screen.findByText('【计算】测试回答');
    await openLink();
    const input = screen.getByLabelText('牌谱链接');
    await userEvent.type(input, link);
    fireEvent.keyDown(input, { code: 'ArrowRight' });
    expect(screen.getByTestId('event-caption').textContent).toBe('自己 · 摸牌 一筒');
    await userEvent.click(screen.getByRole('button', { name: '导入链接' }));
    await screen.findByText('下载天凤牌谱超时，请重试');
    expect((input as HTMLInputElement).value).toBe(link);
    expect(screen.getByText('test')).toBeTruthy();
    expect(screen.getByTestId('event-caption').textContent).toBe('自己 · 摸牌 一筒');
    expect(screen.getByText('【计算】测试回答')).toBeTruthy();
    await userEvent.click(screen.getByRole('button', { name: '导入链接' }));
    await screen.findByText(link);
    expect(screen.queryByText('分析已完成')).toBeNull();
    expect(screen.queryByText('【计算】测试回答')).toBeNull();
    expect((screen.getByLabelText('牌谱进度') as HTMLInputElement).value).toBe('0');
  });
});

describe('完整回放与问答边界', () => {
  it('长牌局列表可以翻页，回放跳转后自动显示当前牌局所在页', async () => {
    const bridge = api();
    vi.mocked(bridge.importLog).mockResolvedValueOnce({
      ...replay,
      rounds: Array.from({ length: 18 }, (_, i) => ({
        label: `第 ${i + 1} 局 · 0 本场`,
        frame_index: i,
      })),
      frames: Array.from({ length: 18 }, (_, i) => ({ ...first, event_index: i + 1 })),
    });
    await load(bridge);
    expect(screen.getByRole('button', { name: /第 1 局/ })).toBeTruthy();
    expect(screen.queryByRole('button', { name: /第 8 局/ })).toBeNull();
    await userEvent.click(screen.getByLabelText('下一页牌局'));
    await userEvent.click(screen.getByRole('button', { name: /第 8 局/ }));
    expect((screen.getByLabelText('牌谱进度') as HTMLInputElement).value).toBe('7');
    fireEvent.change(screen.getByLabelText('牌谱进度'), { target: { value: '17' } });
    expect(screen.getByRole('button', { name: /第 18 局/ }).getAttribute('aria-current')).toBe(
      'step',
    );
    expect((screen.getByLabelText('下一页牌局') as HTMLButtonElement).disabled).toBe(true);
    fireEvent.change(screen.getByLabelText('牌谱进度'), { target: { value: '0' } });
    expect(screen.getByRole('button', { name: /第 1 局/ }).getAttribute('aria-current')).toBe(
      'step',
    );
    expect((screen.getByLabelText('上一页牌局') as HTMLButtonElement).disabled).toBe(true);
  });

  it('新回答只滚动聊天记录，不调用会移动祖先容器的 scrollIntoView', async () => {
    const scrollIntoView = vi.fn();
    Element.prototype.scrollIntoView = scrollIntoView;
    await load(api());
    await openChat();
    const messages = screen.getByRole('log', { name: '复盘对话' });
    const scrollTo = vi.fn();
    messages.scrollTo = scrollTo;
    await userEvent.click(screen.getAllByRole('button', { name: '分析此玩家' })[0]);
    await screen.findByRole('button', { name: '分析已完成' });
    await userEvent.click(screen.getByRole('button', { name: '下一决策 ›' }));
    await openChat();
    await userEvent.click(screen.getByText('这里的几个选择差在哪里？'));
    await screen.findByText('【计算】测试回答');
    expect(scrollTo).toHaveBeenCalledWith({ top: messages.scrollHeight, behavior: 'smooth' });
    expect(scrollIntoView).not.toHaveBeenCalled();
  });

  it('切换局面不会重置当前会话的滚动位置', async () => {
    await load(api());
    await openChat();
    const messages = screen.getByRole('log', { name: '复盘对话' });
    messages.scrollTop = 200;
    await userEvent.click(screen.getByLabelText('下一事件'));
    expect(messages.scrollTop).toBe(200);
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

  it('未分析和非决策点都能提问，决策点额外提供分析快捷问题', async () => {
    const bridge = api();
    await load(bridge);
    await openChat();
    expect(screen.getByText('可以直接提问；当前局面暂无 Mortal 决策结果。')).toBeTruthy();
    expect((screen.getByLabelText('复盘问题') as HTMLTextAreaElement).disabled).toBe(false);
    expect(screen.queryByRole('button', { name: /这里的几个选择差在哪里/ })).toBeNull();
    await userEvent.type(screen.getByLabelText('复盘问题'), '现在是什么情况？');
    await userEvent.click(screen.getByRole('button', { name: '发送问题' }));
    await screen.findByText('【计算】测试回答');
    const id = vi.mocked(bridge.ask).mock.calls[0][3];
    expect(bridge.ask).toHaveBeenCalledWith(1, 0, 1, id, '现在是什么情况？', 'test.json');
    expect(bridge.analyze).not.toHaveBeenCalled();
    fireEvent.change(screen.getByLabelText('牌谱进度'), { target: { value: '3' } });
    await userEvent.type(screen.getByLabelText('复盘问题'), '轮到别人时我该看什么？');
    await userEvent.click(screen.getByRole('button', { name: '发送问题' }));
    await screen.findByText('追问回答');
    expect(bridge.ask).toHaveBeenLastCalledWith(1, 0, 4, id, '轮到别人时我该看什么？', 'test.json');
    expect(screen.getByText('现在是什么情况？', { selector: 'p' })).toBeTruthy();
    expect(bridge.analyze).not.toHaveBeenCalled();
    await userEvent.click(screen.getAllByRole('button', { name: '分析此玩家' })[0]);
    await screen.findByRole('button', { name: '分析已完成' });
    await userEvent.click(screen.getByRole('button', { name: '上一决策' }));
    await userEvent.click(screen.getByRole('button', { name: '新建会话' }));
    expect(screen.getByRole('button', { name: /这里的几个选择差在哪里/ })).toBeTruthy();
  });

  it('最终推荐独立于 Q 值排序，输入框方向键不会跳转局面', async () => {
    const bridge = api();
    await load(bridge);
    await userEvent.click(screen.getAllByRole('button', { name: '分析此玩家' })[0]);
    await screen.findByRole('button', { name: '分析已完成' });
    await userEvent.click(screen.getByRole('button', { name: '下一决策 ›' }));
    expect(document.querySelector('.recommendation strong')?.textContent).toBe('切 二万');
    await openChat();
    const input = screen.getByLabelText('复盘问题');
    await userEvent.type(input, '比较一下');
    fireEvent.keyDown(input, { code: 'ArrowRight' });
    expect(screen.getByTestId('event-caption').textContent).toBe('自己 · 摸牌 一筒');
  });

  it('回答期间切换事件仍显示同一会话，追问带上新位置', async () => {
    const bridge = api();
    const pending = deferred<SessionView>();
    vi.mocked(bridge.ask).mockReturnValueOnce(pending.promise);
    await load(bridge);
    await userEvent.click(screen.getAllByRole('button', { name: '分析此玩家' })[0]);
    await screen.findByRole('button', { name: '分析已完成' });
    await userEvent.click(screen.getByRole('button', { name: '下一决策 ›' }));
    await openChat();
    await userEvent.click(screen.getByText('这里的几个选择差在哪里？'));
    await waitFor(() => expect(bridge.ask).toHaveBeenCalledOnce());
    const oldConversation = vi.mocked(bridge.ask).mock.calls[0][3];
    await userEvent.click(screen.getByLabelText('下一事件'));
    await act(async () => {
      pending.resolve(savedSession(oldConversation, '原会话的后台回答'));
    });
    expect(screen.getByText('原会话的后台回答')).toBeTruthy();
    vi.mocked(bridge.ask).mockResolvedValueOnce(savedSession(oldConversation, '追问回答'));
    await userEvent.type(screen.getByLabelText('复盘问题'), '继续解释');
    await userEvent.click(screen.getByRole('button', { name: '发送问题' }));
    await screen.findByText('追问回答');
    expect(bridge.ask).toHaveBeenLastCalledWith(1, 0, 3, oldConversation, '继续解释', 'test.json');
    expect(bridge.ask).toHaveBeenCalledTimes(2);
  });

  it('重新导入后，旧牌谱的分析不能污染新牌谱', async () => {
    const bridge = api();
    const pending = deferred<Decision[]>();
    vi.mocked(bridge.analyze).mockReturnValueOnce(pending.promise);
    vi.mocked(bridge.importLog)
      .mockResolvedValueOnce(replay)
      .mockResolvedValueOnce({ ...replay, id: 2, game_key: 'game-two' });
    await load(bridge);
    await userEvent.click(screen.getAllByRole('button', { name: '分析此玩家' })[0]);
    const file = new File(['{}'], 'second.json', { type: 'application/json' });
    Object.defineProperty(file, 'text', { value: () => Promise.resolve('{}') });
    await userEvent.upload(screen.getByLabelText('选择天凤牌谱文件'), file);
    await screen.findByText('second');
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

describe('独立会话与历史上下文', () => {
  async function start(bridge: Bridge) {
    await load(bridge);
    await userEvent.click(screen.getAllByRole('button', { name: '分析此玩家' })[0]);
    await screen.findByRole('button', { name: '分析已完成' });
    await userEvent.click(screen.getByRole('button', { name: '下一决策 ›' }));
    await openChat();
    await userEvent.click(screen.getByText('这里的几个选择差在哪里？'));
    await screen.findByText('【计算】测试回答');
  }

  it('切换玩家保留同一会话和草稿，同一牌谱可新建及切换多个会话', async () => {
    const bridge = api();
    await start(bridge);
    const original = vi.mocked(bridge.ask).mock.calls[0][3];
    await userEvent.type(screen.getByLabelText('复盘问题'), '原会话草稿');
    await userEvent.click(screen.getByRole('combobox', { name: '复盘玩家' }));
    await userEvent.click(screen.getByRole('option', { name: '下家' }));
    expect(screen.getByText('【计算】测试回答')).toBeTruthy();
    expect((screen.getByLabelText('复盘问题') as HTMLTextAreaElement).value).toBe('原会话草稿');
    await userEvent.click(screen.getByRole('combobox', { name: '复盘玩家' }));
    await userEvent.click(screen.getByRole('option', { name: '自己' }));
    expect(screen.getByText('【计算】测试回答')).toBeTruthy();
    expect((screen.getByLabelText('复盘问题') as HTMLTextAreaElement).value).toBe('原会话草稿');
    await userEvent.click(screen.getByRole('button', { name: '新建会话' }));
    expect(screen.queryByText('【计算】测试回答')).toBeNull();
    expect((screen.getByLabelText('复盘问题') as HTMLTextAreaElement).value).toBe('');
    await userEvent.click(screen.getByText('这里的几个选择差在哪里？'));
    await screen.findByText('【计算】测试回答');
    expect(vi.mocked(bridge.ask).mock.calls[1][3]).not.toBe(original);
    expect((await bridge.listSessions()).sessions).toHaveLength(2);
    await userEvent.click(screen.getByRole('combobox', { name: '当前会话' }));
    await userEvent.click(
      within(screen.getByRole('listbox', { name: '当前会话' })).getAllByRole('option')[0],
    );
    expect((screen.getByLabelText('复盘问题') as HTMLTextAreaElement).value).toBe('原会话草稿');
    expect(screen.getByLabelText('当前会话').textContent).toBe(
      sessionTitle((await bridge.getSession(original)).title),
    );
  });

  it('会话标题可修改并保存，限制字符数且不改动对话', async () => {
    const bridge = api();
    await start(bridge);
    const id = vi.mocked(bridge.ask).mock.calls[0][3];
    expect(screen.getByLabelText('当前会话').tagName).toBe('BUTTON');
    expect(screen.queryByText(/test\.json/)).toBeNull();
    expect(screen.getByText('自己 · 东一局 · G2')).toBeTruthy();
    await userEvent.click(screen.getByRole('button', { name: '修改会话标题' }));
    fireEvent.change(screen.getByLabelText('会话标题'), { target: { value: '🀄'.repeat(33) } });
    expect(Array.from((screen.getByLabelText('会话标题') as HTMLInputElement).value)).toHaveLength(
      32,
    );
    fireEvent.change(screen.getByLabelText('会话标题'), { target: { value: '   ' } });
    expect((screen.getByRole('button', { name: '保存' }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.change(screen.getByLabelText('会话标题'), { target: { value: '  东一局的押引  ' } });
    await userEvent.click(screen.getByRole('button', { name: '保存' }));
    await waitFor(() => expect(screen.getByLabelText('当前会话').textContent).toBe('东一局的押引'));
    expect(bridge.renameSession).toHaveBeenCalledWith(id, '东一局的押引');
    expect(screen.getByText('【计算】测试回答')).toBeTruthy();
    await userEvent.click(screen.getByRole('button', { name: '修改会话标题' }));
    fireEvent.change(screen.getByLabelText('会话标题'), { target: { value: '未保存' } });
    await userEvent.keyboard('{Escape}');
    expect(screen.getByLabelText('当前会话').textContent).toBe('东一局的押引');
    cleanup();
    render(<App api={bridge} />);
    await userEvent.click(screen.getByRole('button', { name: '历史会话' }));
    await userEvent.click(await screen.findByRole('button', { name: /东一局的押引.*test/ }));
    expect(screen.getByText('东一局的押引', { selector: '.session-title' })).toBeTruthy();
  });

  it('重新打开应用后，不导入牌谱也能查看轨迹并继续历史会话', async () => {
    const bridge = api();
    await start(bridge);
    const id = vi.mocked(bridge.ask).mock.calls[0][3];
    cleanup();
    render(<App api={bridge} />);
    await userEvent.click(screen.getByRole('button', { name: '历史会话' }));
    await userEvent.click(await screen.findByRole('button', { name: /比较这里的候选切牌.*test/ }));
    await screen.findByText('【计算】测试回答');
    await userEvent.click(screen.getByText('工作流程 · 1 次请求 · 完成'));
    expect(screen.getByText('执行工具 · get_review')).toBeTruthy();
    await userEvent.type(screen.getByLabelText('复盘问题'), '接着说');
    await userEvent.click(screen.getByRole('button', { name: '发送问题' }));
    await screen.findByText('追问回答');
    expect(bridge.continueSession).toHaveBeenCalledWith(id, '接着说');
    expect(bridge.analyze).toHaveBeenCalledOnce();
    await userEvent.click(screen.getByRole('button', { name: '导出 JSON' }));
    await screen.findByText('已导出到：/Downloads/Kyoku-session-test.json');
    expect(bridge.exportSession).toHaveBeenCalledWith(id);
  });

  it('历史列表慢读时不积压轮询，问答完成后仍重新取得已保存的会话', async () => {
    const bridge = api();
    await start(bridge);
    const id = vi.mocked(bridge.ask).mock.calls[0][3];
    const answering = deferred<SessionView>();
    vi.mocked(bridge.ask).mockReturnValueOnce(answering.promise);
    vi.mocked(bridge.listSessions).mockClear();
    await userEvent.type(screen.getByLabelText('复盘问题'), '继续');
    await userEvent.click(screen.getByRole('button', { name: '发送问题' }));
    const listing = deferred<Awaited<ReturnType<Bridge['listSessions']>>>();
    vi.mocked(bridge.listSessions).mockReturnValueOnce(listing.promise);
    await userEvent.click(screen.getByRole('button', { name: '历史会话' }));
    expect(screen.getByText('正在读取历史会话…')).toBeTruthy();
    expect(screen.queryByText(/暂无历史会话/)).toBeNull();
    vi.useFakeTimers();
    try {
      await act(async () => {
        await vi.advanceTimersByTimeAsync(6000);
      });
      expect(bridge.listSessions).toHaveBeenCalledOnce();
      await act(async () => {
        answering.resolve(savedSession(id, '新回答'));
      });
      expect(bridge.listSessions).toHaveBeenCalledOnce();
      await act(async () => {
        listing.resolve({ sessions: [], warnings: [] });
      });
      expect(bridge.listSessions).toHaveBeenCalledTimes(2);
      expect(screen.getByRole('button', { name: /比较这里的候选切牌.*test/ })).toBeTruthy();
      await act(async () => {
        await vi.advanceTimersByTimeAsync(6000);
      });
      expect(bridge.listSessions).toHaveBeenCalledTimes(2);
    } finally {
      vi.useRealTimers();
    }
  });

  it('历史读取失败不显示为空，重试成功后清除错误', async () => {
    const bridge = api();
    vi.mocked(bridge.listSessions).mockRejectedValueOnce({ message: '无法读取测试目录' });
    render(<App api={bridge} />);
    await userEvent.click(screen.getByRole('button', { name: '历史会话' }));
    await screen.findByText('历史会话未能完整读取。');
    expect(screen.queryByText(/暂无历史会话/)).toBeNull();
    await userEvent.click(screen.getByRole('button', { name: '重新读取历史' }));
    await screen.findByText(/暂无历史会话/);
    expect(screen.queryByRole('button', { name: '重新读取历史' })).toBeNull();
  });

  it('从 JSON 加载历史会话后可以继续问答；过大文件被拒绝', async () => {
    const bridge = api();
    render(<App api={bridge} />);
    await userEvent.click(screen.getByRole('button', { name: '历史会话' }));
    const file = new File(['{"version":1}'], 'session.json', { type: 'application/json' });
    Object.defineProperty(file, 'text', { value: () => Promise.resolve('{"version":1}') });
    await userEvent.upload(screen.getByLabelText('选择会话 JSON 文件'), file);
    await screen.findByText('从 JSON 恢复的回答');
    expect(bridge.importSession).toHaveBeenCalledWith('{"version":1}');
    await userEvent.type(screen.getByLabelText('复盘问题'), '这个会话的上下文是什么');
    await userEvent.click(screen.getByRole('button', { name: '发送问题' }));
    await screen.findByText('追问回答');
    expect(bridge.continueSession).toHaveBeenCalledWith('imported', '这个会话的上下文是什么');
    Object.defineProperty(file, 'size', { value: 33 * 1024 * 1024 });
    await userEvent.upload(screen.getByLabelText('选择会话 JSON 文件'), file);
    await screen.findByText('会话文件不能超过 32 MiB');
    expect(bridge.importSession).toHaveBeenCalledOnce();
    expect(screen.getByText('追问回答')).toBeTruthy();
  });
});

describe('本地牌谱库', () => {
  it('从顶栏直接改名，保留浏览位置，取消时不保存', async () => {
    const bridge = api();
    vi.mocked(bridge.importLog).mockResolvedValue({ ...replay, name: '原名称.json' });
    vi.mocked(bridge.renameReplay)
      .mockRejectedValueOnce({ message: '名称保存失败' })
      .mockImplementation(async (key, name) => ({ key, name, origin: 'file', saved_at: 1 }));
    await load(bridge);
    await userEvent.click(screen.getByLabelText('下一事件'));
    await userEvent.click(screen.getByRole('button', { name: '重命名当前牌谱' }));
    expect((screen.getByLabelText('牌谱名称') as HTMLInputElement).value).toBe('原名称');
    fireEvent.change(screen.getByLabelText('牌谱名称'), { target: { value: '🀄'.repeat(81) } });
    expect(Array.from((screen.getByLabelText('牌谱名称') as HTMLInputElement).value)).toHaveLength(
      80,
    );
    fireEvent.change(screen.getByLabelText('牌谱名称'), { target: { value: '   ' } });
    expect((screen.getByRole('button', { name: '保存名称' }) as HTMLButtonElement).disabled).toBe(
      true,
    );
    fireEvent.change(screen.getByLabelText('牌谱名称'), { target: { value: '  新的名称  ' } });
    await userEvent.click(screen.getByRole('button', { name: '保存名称' }));
    await screen.findByText('名称保存失败');
    expect((screen.getByLabelText('牌谱名称') as HTMLInputElement).value).toBe('  新的名称  ');
    await userEvent.click(screen.getByRole('button', { name: '保存名称' }));
    await screen.findByText('新的名称', { selector: '.document-title' });
    expect(bridge.renameReplay).toHaveBeenLastCalledWith(replay.game_key, '新的名称');
    expect(screen.queryByRole('dialog', { name: '重命名牌谱' })).toBeNull();
    expect((screen.getByLabelText('牌谱进度') as HTMLInputElement).value).toBe('1');
    expect(bridge.listReplays).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole('button', { name: '重命名当前牌谱' }));
    fireEvent.change(screen.getByLabelText('牌谱名称'), { target: { value: '不保存的名称' } });
    await userEvent.click(screen.getByRole('button', { name: '取消' }));
    expect(screen.getByText('新的名称', { selector: '.document-title' })).toBeTruthy();
    expect(bridge.renameReplay).toHaveBeenCalledTimes(2);
  });

  it('改名限制字符数，失败保留输入，成功同步当前牌谱并保持浏览位置', async () => {
    const bridge = api();
    let record = { key: replay.game_key, name: '原名称', origin: 'file' as const, saved_at: 1 };
    vi.mocked(bridge.importLog).mockResolvedValue({ ...replay, name: record.name });
    vi.mocked(bridge.listReplays).mockImplementation(async () => ({
      directory: '/data/kyoku',
      warnings: [],
      replays: [record],
    }));
    vi.mocked(bridge.renameReplay)
      .mockRejectedValueOnce({ message: '保存名称失败' })
      .mockImplementation(async (_key, name) => {
        record = { ...record, name };
        return record;
      });
    await load(bridge);
    expect(screen.getByText('原名称', { selector: '.document-title' })).toBeTruthy();
    await userEvent.click(screen.getByLabelText('下一事件'));
    await userEvent.click(screen.getByRole('button', { name: '牌谱库' }));
    await userEvent.click(await screen.findByRole('button', { name: '重命名牌谱：原名称' }));
    fireEvent.change(screen.getByLabelText('牌谱名称'), { target: { value: '🀄'.repeat(81) } });
    expect(Array.from((screen.getByLabelText('牌谱名称') as HTMLInputElement).value)).toHaveLength(
      80,
    );
    fireEvent.change(screen.getByLabelText('牌谱名称'), { target: { value: '   ' } });
    expect((screen.getByRole('button', { name: '保存名称' }) as HTMLButtonElement).disabled).toBe(
      true,
    );
    fireEvent.change(screen.getByLabelText('牌谱名称'), { target: { value: '  复盘记录  ' } });
    await userEvent.click(screen.getByRole('button', { name: '保存名称' }));
    await screen.findByText('保存名称失败');
    expect((screen.getByLabelText('牌谱名称') as HTMLInputElement).value).toBe('  复盘记录  ');
    await userEvent.click(screen.getByRole('button', { name: '保存名称' }));
    await screen.findByRole('button', { name: '打开牌谱：复盘记录' });
    expect(bridge.renameReplay).toHaveBeenLastCalledWith(replay.game_key, '复盘记录');
    expect(screen.getByText('复盘记录', { selector: '.document-title' })).toBeTruthy();
    expect((screen.getByLabelText('牌谱进度') as HTMLInputElement).value).toBe('1');
    await userEvent.click(screen.getByRole('button', { name: '重命名牌谱：复盘记录' }));
    await userEvent.keyboard('{Escape}');
    expect(screen.queryByLabelText('牌谱名称')).toBeNull();
    expect(screen.getByRole('dialog', { name: '牌谱库' })).toBeTruthy();
    await userEvent.click(screen.getByRole('button', { name: '关闭牌谱库' }));
    await userEvent.click(screen.getByRole('button', { name: '牌谱库' }));
    await screen.findByRole('button', { name: '打开牌谱：复盘记录' });
    expect(bridge.openReplay).not.toHaveBeenCalled();
  });

  it('筛选并离线打开保存的牌谱，同时找回关联会话', async () => {
    const bridge = api();
    vi.mocked(bridge.listReplays).mockResolvedValue({
      directory: '/data/kyoku',
      warnings: [],
      replays: [
        { key: replay.game_key, name: '我的半庄', origin: 'link', saved_at: 1 },
        { key: 'sample', name: '示例一局', origin: 'example', saved_at: 1 },
      ],
    });
    await load(bridge);
    await openChat();
    await userEvent.type(screen.getByLabelText('复盘问题'), '先聊这份牌谱');
    await userEvent.click(screen.getByRole('button', { name: '发送问题' }));
    await screen.findByText('【计算】测试回答');
    cleanup();
    render(<App api={bridge} />);
    await userEvent.click(screen.getByRole('button', { name: '牌谱库' }));
    await screen.findByRole('button', { name: /打开牌谱：我的半庄/ });
    await userEvent.click(screen.getByRole('button', { name: '打开数据文件夹' }));
    expect(bridge.openDataDirectory).toHaveBeenCalledOnce();
    await userEvent.click(screen.getByRole('combobox', { name: '牌谱来源' }));
    await userEvent.click(screen.getByRole('option', { name: '我的牌谱' }));
    expect(screen.queryByRole('button', { name: /打开牌谱：示例一局/ })).toBeNull();
    await userEvent.type(screen.getByLabelText('搜索牌谱'), '不存在');
    expect(screen.queryByRole('button', { name: /打开牌谱：我的半庄/ })).toBeNull();
    await userEvent.clear(screen.getByLabelText('搜索牌谱'));
    await userEvent.click(screen.getByRole('button', { name: /打开牌谱：我的半庄/ }));
    await screen.findByLabelText('牌谱进度');
    expect(screen.queryByRole('dialog')).toBeNull();
    await openChat();
    expect(screen.getByText('【计算】测试回答')).toBeTruthy();
    expect(screen.getByText('我的半庄')).toBeTruthy();
    expect(bridge.openReplay).toHaveBeenCalledWith(replay.game_key);
    expect(bridge.importLink).not.toHaveBeenCalled();
    expect(bridge.importLog).toHaveBeenCalledOnce();
    expect(bridge.analyze).not.toHaveBeenCalled();
  });

  it('读取失败可重试，打开缺失牌谱失败时保留当前牌桌', async () => {
    const bridge = api();
    vi.mocked(bridge.listReplays)
      .mockRejectedValueOnce({ message: '无法读取牌谱库' })
      .mockResolvedValue({
        directory: '/data/kyoku',
        warnings: ['坏文件已保留'],
        replays: [{ key: 'missing', name: '已移走的牌谱', origin: 'file', saved_at: 1 }],
      });
    vi.mocked(bridge.openReplay).mockRejectedValue({ message: '关联的牌谱文件不存在' });
    await load(bridge);
    await userEvent.click(screen.getByLabelText('下一事件'));
    await userEvent.click(screen.getByRole('button', { name: '牌谱库' }));
    await screen.findByText('无法读取牌谱库');
    await userEvent.click(screen.getByRole('button', { name: '刷新' }));
    await screen.findByText('坏文件已保留');
    await userEvent.click(screen.getByRole('button', { name: /打开牌谱：已移走的牌谱/ }));
    await screen.findByText('关联的牌谱文件不存在');
    expect((screen.getByLabelText('牌谱进度') as HTMLInputElement).value).toBe('1');
    await userEvent.click(screen.getByRole('button', { name: '关闭牌谱库' }));
    expect(screen.getByText('test')).toBeTruthy();
  });
});

describe('牌谱会话的恢复与定位', () => {
  it('同一牌谱重新导入后恢复会话，运行期牌谱编号改变不影响关联', async () => {
    const bridge = api();
    await load(bridge);
    await openChat();
    await userEvent.type(screen.getByLabelText('复盘问题'), '从开局开始聊');
    await userEvent.click(screen.getByRole('button', { name: '发送问题' }));
    await screen.findByText('【计算】测试回答');
    const id = vi.mocked(bridge.ask).mock.calls[0][3];
    cleanup();
    vi.mocked(bridge.importLog).mockResolvedValue({ ...replay, id: 99 });
    await load(bridge);
    await openChat();
    expect(screen.getByText('【计算】测试回答')).toBeTruthy();
    expect(screen.getByLabelText('当前会话').textContent).toBe(
      sessionTitle((await bridge.getSession(id)).title),
    );
    await userEvent.click(screen.getByLabelText('下一事件'));
    await userEvent.type(screen.getByLabelText('复盘问题'), '接着看');
    await userEvent.click(screen.getByRole('button', { name: '发送问题' }));
    await screen.findByText('追问回答');
    expect(bridge.ask).toHaveBeenLastCalledWith(99, 0, 2, id, '接着看', 'test.json');
    expect(bridge.analyze).not.toHaveBeenCalled();
  });

  it('从历史恢复牌桌和浏览位置，点击消息位置能回到原玩家和事件', async () => {
    const bridge = api();
    await load(bridge);
    await openChat();
    await userEvent.type(screen.getByLabelText('复盘问题'), '先看看起手');
    await userEvent.click(screen.getByRole('button', { name: '发送问题' }));
    await screen.findByText('【计算】测试回答');
    const id = vi.mocked(bridge.ask).mock.calls[0][3];
    cleanup();
    vi.mocked(bridge.openSessionGame).mockResolvedValue({
      replay: { ...replay, id: 8 },
      name: '恢复的牌谱',
      position: { player: 1, event_index: 4 },
    });
    render(<App api={bridge} />);
    await userEvent.click(screen.getByRole('button', { name: '历史会话' }));
    await userEvent.click(await screen.findByRole('button', { name: /先看看起手.*test/ }));
    await userEvent.click(await screen.findByRole('button', { name: '打开牌谱并继续' }));
    await screen.findByLabelText('牌谱进度');
    expect((screen.getByLabelText('牌谱进度') as HTMLInputElement).value).toBe('3');
    expect(screen.getByRole('combobox', { name: '复盘玩家' }).textContent).toBe('下家');
    expect(screen.getByLabelText('当前会话').textContent).toBe(
      sessionTitle((await bridge.getSession(id)).title),
    );
    await userEvent.click(screen.getByRole('button', { name: '玩家 0 · G1' }));
    expect((screen.getByLabelText('牌谱进度') as HTMLInputElement).value).toBe('0');
    expect(screen.getByRole('combobox', { name: '复盘玩家' }).textContent).toBe('自己');
    expect(screen.getByText('【计算】测试回答')).toBeTruthy();
    expect(bridge.analyze).not.toHaveBeenCalled();
    expect(bridge.openSessionGame).toHaveBeenCalledWith(id);
  });

  it('生成期间可以另开会话，旧回答完成时不会切走当前会话', async () => {
    const bridge = api();
    const answering = deferred<SessionView>();
    vi.mocked(bridge.ask).mockReturnValueOnce(answering.promise);
    await load(bridge);
    await openChat();
    await userEvent.type(screen.getByLabelText('复盘问题'), '旧会话的问题');
    await userEvent.click(screen.getByRole('button', { name: '发送问题' }));
    const oldId = vi.mocked(bridge.ask).mock.calls[0][3];
    await userEvent.click(screen.getByRole('button', { name: '新建会话' }));
    expect(screen.getByLabelText('当前会话').textContent).toBe('新会话');
    await userEvent.type(screen.getByLabelText('复盘问题'), '新会话的问题');
    await userEvent.click(screen.getByRole('button', { name: '发送问题' }));
    await screen.findByText('【计算】测试回答');
    await act(async () => {
      answering.resolve(savedSession(oldId, '旧会话刚完成', '旧会话的问题'));
    });
    expect(screen.getByLabelText('当前会话').textContent).toBe('新会话的问题');
    expect(vi.mocked(bridge.ask).mock.calls[1][3]).not.toBe(oldId);
    expect(screen.queryByText('旧会话刚完成')).toBeNull();
    await userEvent.click(screen.getByRole('combobox', { name: '当前会话' }));
    await userEvent.click(screen.getByRole('option', { name: '旧会话的问题' }));
    expect(screen.getByText('旧会话刚完成')).toBeTruthy();
  });

  it('切换局面后重试失败问题仍指定原轮次，不以当前牌桌重新发问', async () => {
    const bridge = api();
    vi.mocked(bridge.ask).mockImplementationOnce(async (_game, _player, _event, id, text) => {
      const doc = savedSession(id, '', text);
      doc.archive.turns[0].answer = null;
      doc.archive.turns[0].error = '模拟网络失败';
      return doc;
    });
    await load(bridge);
    await openChat();
    await userEvent.click(screen.getByLabelText('下一事件'));
    await userEvent.type(screen.getByLabelText('复盘问题'), '这个问题要重试');
    await userEvent.click(screen.getByRole('button', { name: '发送问题' }));
    await screen.findByText('模拟网络失败');
    const id = vi.mocked(bridge.ask).mock.calls[0][3];
    vi.mocked(bridge.retrySession).mockResolvedValue(savedSession(id, '重试的回答'));
    await userEvent.click(screen.getByLabelText('下一事件'));
    await userEvent.click(screen.getByRole('combobox', { name: '复盘玩家' }));
    await userEvent.click(screen.getByRole('option', { name: '下家' }));
    await userEvent.click(screen.getByRole('button', { name: '重试此问题' }));
    await screen.findByText('重试的回答');
    expect(bridge.retrySession).toHaveBeenCalledWith(id, 0);
    expect(bridge.ask).toHaveBeenCalledOnce();
    expect((screen.getByLabelText('牌谱进度') as HTMLInputElement).value).toBe('2');
    expect(screen.getByRole('combobox', { name: '复盘玩家' }).textContent).toBe('下家');
  });
});
