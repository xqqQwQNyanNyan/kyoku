import { useEffect, useRef, useState } from 'react';
import { flushSync } from 'react-dom';
import { UsageSummary, UsageDetails } from './Usage';
import { ModelSettings, UsageSettings, defaultModelOptions } from './ModelSettings';
import { errorMessage } from './bridge';
import { StorageSettings } from './StorageSettings';
import type { Bridge, RuntimeStatus, Settings, SettingsInput, QuestionProgress } from './types';

const tabs = [
  { id: 'connection', label: '连接' },
  { id: 'model', label: '模型参数' },
  { id: 'usage', label: '用量与费用' },
  { id: 'runtime', label: '本地分析' },
  { id: 'storage', label: '数据保存' },
] as const;
type SettingsTab = (typeof tabs)[number]['id'];

export function SettingsPanel({ api, onClose }: { api: Bridge; onClose: () => void }) {
  const dialog = useRef<HTMLDialogElement>(null);
  const running = useRef(false);
  const form = useRef<HTMLFormElement>(null);
  const [tab, setTab] = useState<SettingsTab>('connection');
  const [saved, setSaved] = useState<Settings | null>(null);
  const [input, setInput] = useState<SettingsInput>({
    endpoint: '',
    model: '',
    api_key: '',
    clear_key: false,
    options: defaultModelOptions,
  });
  const [runtime, setRuntime] = useState<RuntimeStatus | null>(null);
  const [busy, setBusy] = useState('');
  const [testUsage, setTestUsage] = useState<Extract<QuestionProgress, { phase: 'usage' }> | null>(
    null,
  );
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const [runtimeError, setRuntimeError] = useState('');
  const [checking, setChecking] = useState(false);
  const [storageBusy, setStorageBusy] = useState(false);

  useEffect(() => {
    dialog.current?.showModal();
    let active = true;
    void api
      .getSettings()
      .then((value) => {
        if (!active) return;
        setSaved(value);
        setInput({
          endpoint: value.endpoint,
          model: value.model,
          api_key: '',
          clear_key: false,
          options: value.options ?? defaultModelOptions,
        });
      })
      .catch((e: unknown) => {
        if (active) setError(errorMessage(e));
      });
    void api
      .runtimeStatus(false)
      .then((value) => {
        if (active) setRuntime(value);
      })
      .catch((e: unknown) => {
        if (active) setRuntimeError(errorMessage(e));
      });
    return () => {
      active = false;
    };
  }, [api]);

  function change(patch: Partial<SettingsInput>) {
    setInput((current) => ({ ...current, ...patch }));
    setNotice('');
    setError('');
  }

  async function submit(action: 'save' | 'test') {
    if (running.current || storageBusy || !saved || !form.current) return;
    const invalid = Array.from(form.current.elements).find(
      (element): element is HTMLInputElement =>
        element instanceof HTMLInputElement && !element.validity.valid,
    );
    if (invalid) {
      // 先显示有错误的标签页，再让浏览器定位输入框，避免隐藏控件无法聚焦。
      const target = invalid.closest<HTMLElement>('[data-settings-tab]')?.dataset
        .settingsTab as SettingsTab;
      if (target) flushSync(() => setTab(target));
      invalid.reportValidity();
      invalid.focus();
      return;
    }
    running.current = true;
    setBusy(action);
    setError('');
    setNotice('');
    try {
      if (action === 'test') {
        setTestUsage(null);
        await api.testConnection(input, (progress) => {
          if (progress.phase === 'usage') setTestUsage(progress);
        });
        setNotice('连接成功，模型支持工具调用。');
      } else {
        const result = await api.saveSettings(input);
        setSaved(result);
        setInput({
          endpoint: result.endpoint,
          model: result.model,
          api_key: '',
          clear_key: false,
          options: result.options ?? defaultModelOptions,
        });
        setNotice('设置已保存，后续提问使用新参数，历史会话会保留。');
      }
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      running.current = false;
      setBusy('');
    }
  }

  async function checkRuntime() {
    if (checking) return;
    setChecking(true);
    setRuntimeError('');
    try {
      setRuntime(await api.runtimeStatus(true));
    } catch (e) {
      setRuntimeError(errorMessage(e));
    } finally {
      setChecking(false);
    }
  }

  const keepKey =
    saved?.has_api_key && saved.endpoint === input.endpoint.trim() && !input.clear_key;
  return (
    <dialog
      ref={dialog}
      className="settings-dialog"
      aria-labelledby="settings-title"
      onCancel={(event) => {
        event.preventDefault();
        if (!busy && !checking && !storageBusy) onClose();
      }}
    >
      <div className="settings-heading">
        <div>
          <span className="eyebrow">KYOKU</span>
          <h2 id="settings-title">设置</h2>
        </div>
        <button
          type="button"
          autoFocus
          onClick={onClose}
          disabled={!!busy || checking || storageBusy}
          aria-label="关闭设置"
        >
          ×
        </button>
      </div>
      <div className="settings-tabs" role="tablist" aria-label="设置分类">
        {tabs.map((item, index) => (
          <button
            key={item.id}
            type="button"
            role="tab"
            id={`settings-tab-${item.id}`}
            aria-selected={tab === item.id}
            aria-controls={`settings-panel-${item.id}`}
            tabIndex={tab === item.id ? 0 : -1}
            onClick={() => setTab(item.id)}
            onKeyDown={(event) => {
              let next: number;
              switch (event.key) {
                case 'ArrowLeft':
                  next = (index + tabs.length - 1) % tabs.length;
                  break;
                case 'ArrowRight':
                  next = (index + 1) % tabs.length;
                  break;
                case 'Home':
                  next = 0;
                  break;
                case 'End':
                  next = tabs.length - 1;
                  break;
                default:
                  return;
              }
              event.preventDefault();
              setTab(tabs[next].id);
              document.getElementById(`settings-tab-${tabs[next].id}`)?.focus();
            }}
          >
            {item.label}
          </button>
        ))}
      </div>
      <form
        ref={form}
        noValidate
        onSubmit={(event) => {
          event.preventDefault();
          if (tab !== 'storage') void submit('save');
        }}
      >
        <div className="settings-content">
          <section
            role="tabpanel"
            id="settings-panel-connection"
            aria-labelledby="settings-tab-connection"
            data-settings-tab="connection"
            hidden={tab !== 'connection'}
          >
            <fieldset disabled={!saved || !!busy}>
              <label htmlFor="llm-endpoint">服务地址</label>
              <input
                id="llm-endpoint"
                type="url"
                required
                value={input.endpoint}
                autoComplete="off"
                spellCheck={false}
                placeholder="https://api.openai.com/v1/responses"
                onChange={(e) => change({ endpoint: e.target.value })}
              />
              <small>
                填写完整地址，以 /responses 或 /chat/completions 结尾；仅本机服务允许 HTTP。
              </small>
              <div className="settings-credentials">
                <div>
                  <label htmlFor="llm-model">模型名</label>
                  <input
                    id="llm-model"
                    required
                    value={input.model}
                    autoComplete="off"
                    spellCheck={false}
                    placeholder="填写服务支持的模型名"
                    onChange={(e) => change({ model: e.target.value })}
                  />
                </div>
                <div>
                  <label htmlFor="llm-key">API Key</label>
                  <input
                    id="llm-key"
                    type="password"
                    value={input.api_key}
                    autoComplete="new-password"
                    spellCheck={false}
                    disabled={input.clear_key}
                    placeholder={keepKey ? '已有密钥，留空保留' : '填写此服务的专用密钥'}
                    onChange={(e) => change({ api_key: e.target.value })}
                  />
                </div>
              </div>
              <small>
                密钥以明文保存在本机应用配置文件中。更换地址后需重新填写；本机无认证服务可留空。
              </small>
              {saved?.has_api_key && (
                <label className="settings-clear">
                  <input
                    type="checkbox"
                    checked={input.clear_key}
                    onChange={(e) => change({ clear_key: e.target.checked, api_key: '' })}
                  />
                  删除已保存的密钥
                </label>
              )}
            </fieldset>
            {testUsage && (
              <details className="settings-test-usage">
                <summary>连接测试用量</summary>
                <UsageSummary
                  label="连接测试"
                  requests={testUsage.requests}
                  budget={testUsage.budget}
                />
                <UsageDetails requests={testUsage.requests} />
              </details>
            )}
          </section>
          <section
            role="tabpanel"
            id="settings-panel-model"
            aria-labelledby="settings-tab-model"
            data-settings-tab="model"
            hidden={tab !== 'model'}
          >
            <p className="settings-description">按所选模型的能力调整生成参数。</p>
            <fieldset disabled={!saved || !!busy}>
              <ModelSettings
                value={input.options ?? defaultModelOptions}
                disabled={!saved || !!busy}
                onChange={(options) => change({ options })}
              />
            </fieldset>
          </section>
          <section
            role="tabpanel"
            id="settings-panel-usage"
            aria-labelledby="settings-tab-usage"
            data-settings-tab="usage"
            hidden={tab !== 'usage'}
          >
            <fieldset disabled={!saved || !!busy}>
              <UsageSettings
                value={input.options ?? defaultModelOptions}
                onChange={(options) => change({ options })}
              />
            </fieldset>
          </section>
          <section
            role="tabpanel"
            id="settings-panel-runtime"
            aria-labelledby="settings-tab-runtime"
            data-settings-tab="runtime"
            hidden={tab !== 'runtime'}
          >
            <p className="settings-description">本地引擎用于离线分析牌局。</p>
            <section className="settings-runtime" aria-labelledby="runtime-title">
              <div>
                <h3 id="runtime-title">本地分析</h3>
                <p>{runtime?.model ?? '正在读取引擎信息…'}</p>
                {runtime && (
                  <small>
                    {runtime.bundled ? '应用内置' : '开发环境'} ·{' '}
                    {runtime.checked
                      ? '引擎检查通过'
                      : runtime.available
                        ? '资源齐全，尚未检查运行'
                        : '资源不完整'}
                  </small>
                )}
              </div>
              <button type="button" onClick={() => void checkRuntime()} disabled={checking}>
                {checking ? '正在检查…' : '检查引擎'}
              </button>
            </section>
            {runtimeError && (
              <p role="alert" className="settings-error">
                {runtimeError}
              </p>
            )}
          </section>
          <section
            role="tabpanel"
            id="settings-panel-storage"
            aria-labelledby="settings-tab-storage"
            data-settings-tab="storage"
            hidden={tab !== 'storage'}
          >
            <StorageSettings api={api} onBusyChange={setStorageBusy} />
          </section>
        </div>
        <footer className="settings-footer" hidden={tab === 'storage'}>
          {error && (
            <p role="alert" className="settings-error">
              {error}
            </p>
          )}
          {notice && (
            <p role="status" className="settings-notice">
              {notice}
            </p>
          )}
          <div className="settings-actions">
            <p className="settings-footnote">
              连接测试可能产生费用，不会自动保存；保存设置会保留历史会话。
            </p>
            <button
              type="button"
              disabled={
                !saved || !!busy || storageBusy || !input.endpoint.trim() || !input.model.trim()
              }
              onClick={() => void submit('test')}
            >
              {busy === 'test' ? '正在测试…' : '测试连接'}
            </button>
            <button
              className="primary"
              type="submit"
              disabled={
                !saved || !!busy || storageBusy || !input.endpoint.trim() || !input.model.trim()
              }
            >
              {busy === 'save' ? '正在保存…' : '保存设置'}
            </button>
          </div>
        </footer>
      </form>
    </dialog>
  );
}
