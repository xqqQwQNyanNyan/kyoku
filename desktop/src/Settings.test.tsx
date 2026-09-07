// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
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
    importLog: vi.fn(),
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
  });
  await screen.findByText('设置已保存，新会话使用新配置，历史会话会保留。');
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
  expect(bridge.testConnection).toHaveBeenCalledWith({
    endpoint: 'https://another.example/v1/responses',
    model: 'test-model',
    api_key: 'new-key',
    clear_key: false,
  });
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
  );
  await userEvent.click(screen.getByRole('button', { name: '保存设置' }));
  await screen.findByText('设置已保存，新会话使用新配置，历史会话会保留。');
  expect(bridge.saveSettings).toHaveBeenCalledWith(
    expect.objectContaining({ endpoint, api_key: 'chat-key' }),
  );
  expect((screen.getByLabelText('服务地址') as HTMLInputElement).value).toBe(endpoint);
});
