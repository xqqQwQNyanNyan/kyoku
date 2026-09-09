// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { defaultModelOptions } from './ModelSettings';
import { SettingsPanel } from './Settings';
import type { Bridge, Settings } from './types';

const stored: Settings = {
  endpoint: 'https://service.example/v1/responses',
  model: 'test-model',
  has_api_key: true,
  saved: true,
};
function api(): Bridge {
  return {
    cancelQuestion: vi.fn(),
    cancelAnalysis: vi.fn(),
    importLog: vi.fn(),
    listReplays: vi.fn().mockResolvedValue({ replays: [], warnings: [], directory: '/data/kyoku' }),
    previewReplayDeletion: vi.fn(),
    deleteReplay: vi.fn(),
    deleteSession: vi.fn(),
    renameReplay: vi.fn(),
    openReplay: vi.fn().mockResolvedValue(undefined),
    openDataDirectory: vi.fn().mockResolvedValue(undefined),
    getStorage: vi.fn().mockResolvedValue({ directory: '/data/kyoku', available: true }),
    chooseDataDirectory: vi.fn().mockResolvedValue(null),
    migrateData: vi.fn(),
    cancelDataMigration: vi.fn().mockResolvedValue(undefined),
    importLink: vi.fn(),
    majsoulStatus: vi.fn().mockResolvedValue(false),
    loginMajsoul: vi.fn(),
    logoutMajsoul: vi.fn(),
    analyze: vi.fn(),
    ask: vi.fn(),
    listSessions: vi.fn(),
    getSession: vi.fn(),
    renameSession: vi.fn(),
    openSessionGame: vi.fn(),
    setSessionPosition: vi.fn().mockResolvedValue(undefined),
    continueSession: vi.fn(),
    importSession: vi.fn(),
    retrySession: vi.fn(),
    exportSession: vi.fn(),
    getSettings: vi.fn().mockResolvedValue(stored),
    saveSettings: vi.fn().mockResolvedValue(stored),
    testConnection: vi.fn().mockResolvedValue(undefined),
    runtimeStatus: vi.fn().mockImplementation((check: boolean) =>
      Promise.resolve({
        bundled: true,
        available: true,
        checked: check,
        model: 'Mortal V4 · CPU',
      }),
    ),
  };
}

beforeEach(() => {
  HTMLDialogElement.prototype.showModal = function () {
    this.open = true;
  };
});
afterEach(cleanup);

it('加载设置不回显密钥，保存时可保留原密钥', async () => {
  const bridge = api();
  render(<SettingsPanel api={bridge} onClose={vi.fn()} />);
  await screen.findByPlaceholderText('已有密钥，留空保留');
  expect((screen.getByLabelText('API Key') as HTMLInputElement).value).toBe('');
  await userEvent.clear(screen.getByLabelText('模型名'));
  await userEvent.type(screen.getByLabelText('模型名'), 'next-model');
  await userEvent.click(screen.getByRole('button', { name: '保存设置' }));
  expect(bridge.saveSettings).toHaveBeenCalledWith({
    endpoint: stored.endpoint,
    model: 'next-model',
    api_key: '',
    clear_key: false,
    options: defaultModelOptions,
  });
  await screen.findByText('设置已保存，后续提问使用新参数，历史会话会保留。');
});

it('换地址提示重新填写密钥；测试使用草稿而不会保存', async () => {
  const bridge = api();
  render(<SettingsPanel api={bridge} onClose={vi.fn()} />);
  await screen.findByPlaceholderText('已有密钥，留空保留');
  await userEvent.clear(screen.getByLabelText('服务地址'));
  await userEvent.type(screen.getByLabelText('服务地址'), 'https://another.example/v1/responses');
  expect(screen.getByPlaceholderText('填写此服务的专用密钥')).toBeTruthy();
  await userEvent.type(screen.getByLabelText('API Key'), 'new-key');
  await userEvent.click(screen.getByRole('button', { name: '测试连接' }));
  expect(bridge.testConnection).toHaveBeenCalledWith(
    {
      endpoint: 'https://another.example/v1/responses',
      model: 'test-model',
      api_key: 'new-key',
      clear_key: false,
      options: defaultModelOptions,
    },
    expect.any(Function),
  );
  await screen.findByRole('status');
  expect(bridge.saveSettings).not.toHaveBeenCalled();
});

it('保存失败保留输入和现有问答，允许重试', async () => {
  const bridge = api();
  bridge.saveSettings = vi.fn().mockRejectedValue({ message: '无法保存设置' });
  render(<SettingsPanel api={bridge} onClose={vi.fn()} />);
  await screen.findByPlaceholderText('已有密钥，留空保留');
  await userEvent.type(screen.getByLabelText('API Key'), 'retry-key');
  await userEvent.click(screen.getByRole('button', { name: '保存设置' }));
  await screen.findByText('无法保存设置');
  expect((screen.getByLabelText('API Key') as HTMLInputElement).value).toBe('retry-key');
});

it('可删除密钥并独立检查本地引擎', async () => {
  const bridge = api();
  render(<SettingsPanel api={bridge} onClose={vi.fn()} />);
  await screen.findByLabelText('删除已保存的密钥');
  await userEvent.click(screen.getByLabelText('删除已保存的密钥'));
  expect((screen.getByLabelText('API Key') as HTMLInputElement).disabled).toBe(true);
  await userEvent.click(screen.getByRole('button', { name: '保存设置' }));
  expect(bridge.saveSettings).toHaveBeenCalledWith(
    expect.objectContaining({ clear_key: true, api_key: '' }),
  );
  await userEvent.click(screen.getByRole('tab', { name: '本地分析' }));
  await userEvent.click(screen.getByRole('button', { name: '检查引擎' }));
  await waitFor(() => expect(screen.getByText('应用内置 · 引擎检查通过')).toBeTruthy());
  expect(bridge.testConnection).not.toHaveBeenCalled();
});

it('允许填写 Chat Completions 地址、测试连接并保存', async () => {
  const bridge = api();
  const endpoint = 'https://service.example/v1/chat/completions';
  bridge.saveSettings = vi.fn().mockResolvedValue({ ...stored, endpoint });
  render(<SettingsPanel api={bridge} onClose={vi.fn()} />);
  await screen.findByPlaceholderText('已有密钥，留空保留');
  await userEvent.clear(screen.getByLabelText('服务地址'));
  await userEvent.type(screen.getByLabelText('服务地址'), endpoint);
  await userEvent.type(screen.getByLabelText('API Key'), 'chat-key');
  await userEvent.click(screen.getByRole('button', { name: '测试连接' }));
  await screen.findByText('连接成功，模型支持工具调用。');
  expect(bridge.testConnection).toHaveBeenCalledWith(
    expect.objectContaining({ endpoint, api_key: 'chat-key' }),
    expect.any(Function),
  );
  await userEvent.click(screen.getByRole('button', { name: '保存设置' }));
  await screen.findByText('设置已保存，后续提问使用新参数，历史会话会保留。');
  expect(bridge.saveSettings).toHaveBeenCalledWith(
    expect.objectContaining({ endpoint, api_key: 'chat-key' }),
  );
  expect((screen.getByLabelText('服务地址') as HTMLInputElement).value).toBe(endpoint);
});

it('高级参数和价格跨标签页保留，并随草稿一起保存', async () => {
  const bridge = api();
  render(<SettingsPanel api={bridge} onClose={vi.fn()} />);
  await screen.findByPlaceholderText('已有密钥，留空保留');
  await userEvent.click(screen.getByRole('tab', { name: '模型参数' }));
  await userEvent.clear(screen.getByLabelText('单次输出上限（Token）'));
  await userEvent.type(screen.getByLabelText('单次输出上限（Token）'), '16384');
  await userEvent.type(screen.getByLabelText('上下文长度（Token，可留空）'), '128000');
  await userEvent.click(screen.getByRole('combobox', { name: '思考模式' }));
  await userEvent.click(screen.getByRole('option', { name: 'high' }));
  await userEvent.click(screen.getByRole('combobox', { name: 'Chat 输出参数' }));
  await userEvent.click(screen.getByRole('option', { name: 'max_tokens' }));
  await userEvent.click(screen.getByRole('tab', { name: '用量与费用' }));
  await userEvent.type(screen.getByLabelText('每轮 Token 预算（可留空）'), '200000');
  await userEvent.click(screen.getByLabelText('配置价格，估算 API 费用'));
  await userEvent.clear(screen.getByLabelText('输入价格 / 百万 Token'));
  await userEvent.type(screen.getByLabelText('输入价格 / 百万 Token'), '2');
  await userEvent.clear(screen.getByLabelText('输出价格 / 百万 Token'));
  await userEvent.type(screen.getByLabelText('输出价格 / 百万 Token'), '8');
  await userEvent.click(screen.getByRole('button', { name: '保存设置' }));
  expect(bridge.saveSettings).toHaveBeenCalledWith(
    expect.objectContaining({
      options: {
        max_output_tokens: 16384,
        context_tokens: 128000,
        thinking: 'high',
        chat_token_limit: 'max_tokens',
        token_budget: 200000,
        prices: { currency: 'CNY', input: 2, output: 8, cached_input: null },
      },
    }),
  );
});

it('上下文小于输出上限时，保存和测试都不发送无效配置', async () => {
  const bridge = api();
  render(<SettingsPanel api={bridge} onClose={vi.fn()} />);
  await screen.findByPlaceholderText('已有密钥，留空保留');
  await userEvent.click(screen.getByRole('tab', { name: '模型参数' }));
  await userEvent.type(screen.getByLabelText('上下文长度（Token，可留空）'), '100');
  await userEvent.click(screen.getByRole('tab', { name: '连接' }));
  await userEvent.click(screen.getByRole('button', { name: '测试连接' }));
  await userEvent.click(screen.getByRole('button', { name: '保存设置' }));
  expect(bridge.testConnection).not.toHaveBeenCalled();
  expect(bridge.saveSettings).not.toHaveBeenCalled();
  expect(screen.getByRole('tab', { name: '模型参数' }).getAttribute('aria-selected')).toBe('true');
  expect(document.activeElement).toBe(screen.getByLabelText('上下文长度（Token，可留空）'));
});

it('标签页支持键盘切换，草稿和统一保存操作始终保留', async () => {
  const bridge = api();
  render(<SettingsPanel api={bridge} onClose={vi.fn()} />);
  await screen.findByPlaceholderText('已有密钥，留空保留');
  await userEvent.clear(screen.getByLabelText('模型名'));
  await userEvent.type(screen.getByLabelText('模型名'), 'draft-model');
  const connection = screen.getByRole('tab', { name: '连接' });
  connection.focus();
  await userEvent.keyboard('{ArrowRight}');
  expect(screen.getByRole('tabpanel').getAttribute('id')).toBe('settings-panel-model');
  expect(screen.getAllByRole('combobox')).toHaveLength(2);
  expect(document.querySelector('select')).toBeNull();
  expect(screen.getByRole('button', { name: '保存设置' })).toBeTruthy();
  await userEvent.keyboard('{End}');
  expect(screen.getByRole('tabpanel').getAttribute('id')).toBe('settings-panel-storage');
  await userEvent.keyboard('{Home}');
  expect((screen.getByLabelText('模型名') as HTMLInputElement).value).toBe('draft-model');
});

it('数据目录独立于问答配置，选择取消不迁移，完成后显示新位置', async () => {
  const bridge = api();
  vi.mocked(bridge.getSettings).mockRejectedValue(new Error('问答配置损坏'));
  render(<SettingsPanel api={bridge} onClose={vi.fn()} />);
  await userEvent.click(screen.getByRole('tab', { name: '数据保存' }));
  await waitFor(() =>
    expect((screen.getByLabelText('当前保存位置') as HTMLInputElement).value).toBe('/data/kyoku'),
  );
  expect(screen.queryByRole('button', { name: '保存设置' })).toBeNull();
  await userEvent.click(screen.getByRole('button', { name: '选择新位置…' }));
  expect(bridge.migrateData).not.toHaveBeenCalled();
  vi.mocked(bridge.chooseDataDirectory).mockResolvedValue('D:\\复盘资料\\Kyoku');
  vi.mocked(bridge.migrateData).mockResolvedValue({
    directory: 'D:\\复盘资料\\Kyoku',
    available: true,
  });
  await userEvent.click(screen.getByRole('button', { name: '选择新位置…' }));
  expect((screen.getByLabelText('新的保存位置') as HTMLInputElement).value).toBe(
    'D:\\复盘资料\\Kyoku',
  );
  expect(bridge.migrateData).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole('button', { name: '迁移并使用此位置' }));
  await screen.findByText('保存位置已切换，后续牌谱和对话会写入新目录。原目录副本已保留。');
  expect((screen.getByLabelText('当前保存位置') as HTMLInputElement).value).toBe(
    'D:\\复盘资料\\Kyoku',
  );
  expect(bridge.saveSettings).not.toHaveBeenCalled();
});

it('迁移期间显示进度并可取消，失败时保留原位置和待选目录', async () => {
  const bridge = api();
  const close = vi.fn();
  vi.mocked(bridge.chooseDataDirectory).mockResolvedValue('/new/location');
  let rejectMove: (e: Error) => void = () => {};
  vi.mocked(bridge.migrateData).mockImplementation((_directory, _id, progress) => {
    progress({ copied_files: 1, total_files: 5, copied_bytes: 100, total_bytes: 500 });
    return new Promise((_resolve, reject) => {
      rejectMove = reject;
    });
  });
  render(<SettingsPanel api={bridge} onClose={close} />);
  await userEvent.click(screen.getByRole('tab', { name: '数据保存' }));
  await userEvent.click(screen.getByRole('button', { name: '选择新位置…' }));
  await userEvent.click(screen.getByRole('button', { name: '迁移并使用此位置' }));
  await screen.findByText('已复制 1 / 5 个文件');
  expect(screen.getByRole('progressbar').getAttribute('max')).toBe('500');
  expect((screen.getByRole('button', { name: '关闭设置' }) as HTMLButtonElement).disabled).toBe(
    true,
  );
  await userEvent.click(screen.getByRole('button', { name: '关闭设置' }));
  expect(close).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole('button', { name: '取消迁移' }));
  expect(bridge.cancelDataMigration).toHaveBeenCalledWith(
    vi.mocked(bridge.migrateData).mock.calls[0][1],
  );
  rejectMove(new Error('迁移已取消，仍使用原数据目录'));
  await screen.findByText('迁移已取消，仍使用原数据目录');
  expect((screen.getByLabelText('当前保存位置') as HTMLInputElement).value).toBe('/data/kyoku');
  expect((screen.getByLabelText('新的保存位置') as HTMLInputElement).value).toBe('/new/location');
  expect(
    (screen.getByRole('button', { name: '迁移并使用此位置' }) as HTMLButtonElement).disabled,
  ).toBe(false);
});

it('自选磁盘不可用时说明原路径，不允许用空库替代', async () => {
  const bridge = api();
  vi.mocked(bridge.getStorage).mockResolvedValue({
    directory: '/Volumes/离线盘/资料',
    available: false,
  });
  render(<SettingsPanel api={bridge} onClose={vi.fn()} />);
  await userEvent.click(screen.getByRole('tab', { name: '数据保存' }));
  await screen.findByText(/数据目录不可用。请重新连接/);
  expect((screen.getByRole('button', { name: '选择新位置…' }) as HTMLButtonElement).disabled).toBe(
    true,
  );
  expect((screen.getByLabelText('当前保存位置') as HTMLInputElement).value).toBe(
    '/Volumes/离线盘/资料',
  );
});
