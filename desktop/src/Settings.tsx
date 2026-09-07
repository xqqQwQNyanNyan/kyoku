import { useEffect, useRef, useState } from 'react';
import { errorMessage } from './bridge';
import type { Bridge, RuntimeStatus, Settings, SettingsInput } from './types';

export function SettingsPanel({ api, onClose }: { api: Bridge; onClose: () => void }) {
  const dialog = useRef<HTMLDialogElement>(null);
  const running = useRef(false);
  const [saved, setSaved] = useState<Settings | null>(null);
  const [input, setInput] = useState<SettingsInput>({
    endpoint: '',
    model: '',
    api_key: '',
    clear_key: false,
  });
  const [runtime, setRuntime] = useState<RuntimeStatus | null>(null);
  const [busy, setBusy] = useState('');
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const [runtimeError, setRuntimeError] = useState('');
  const [checking, setChecking] = useState(false);

  useEffect(() => {
    dialog.current?.showModal();
    let active = true;
    void api
      .getSettings()
      .then((value) => {
        if (!active) return;
        setSaved(value);
        setInput({ endpoint: value.endpoint, model: value.model, api_key: '', clear_key: false });
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
    if (running.current) return;
    running.current = true;
    setBusy(action);
    setError('');
    setNotice('');
    try {
      if (action === 'test') {
        await api.testConnection(input);
        setNotice('连接成功，模型支持工具调用。');
      } else {
        const result = await api.saveSettings(input);
        setSaved(result);
        setInput({ endpoint: result.endpoint, model: result.model, api_key: '', clear_key: false });
        setNotice('设置已保存，新会话使用新配置，历史会话会保留。');
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
        if (!busy && !checking) onClose();
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
          disabled={!!busy || checking}
          aria-label="关闭设置"
        >
          ×
        </button>
      </div>
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
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void submit('save');
        }}
      >
        <h3>Agent 问答</h3>
        <p className="settings-description">仅问答需要联网配置。回放和本地分析无需 API Key。</p>
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
          <div className="settings-actions">
            <button
              type="button"
              disabled={!input.endpoint.trim() || !input.model.trim()}
              onClick={() => void submit('test')}
            >
              {busy === 'test' ? '正在测试…' : '测试连接'}
            </button>
            <button
              className="primary"
              type="submit"
              disabled={!input.endpoint.trim() || !input.model.trim()}
            >
              {busy === 'save' ? '正在保存…' : '保存设置'}
            </button>
          </div>
        </fieldset>
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
        <p className="settings-footnote">
          测试连接会发送一次不含牌谱的请求，可能产生少量调用费用。保存配置会清空当前问答历史。
        </p>
      </form>
    </dialog>
  );
}
