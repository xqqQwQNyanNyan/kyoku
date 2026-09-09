import { useEffect, useRef, useState } from 'react';
import { examples } from './examples';
import { Select } from './Select';
import exampleLicense from '../../fixtures/tenhou/LICENSE?url';
import type { Bridge } from './types';
import { errorMessage } from './bridge';

function shareLink(text: string): string {
  return text.trim().replace(/^雀魂牌[谱譜]\s*[:：]\s*/u, '');
}

function isMajsoulLink(link: string): boolean {
  try {
    return [
      'game.maj-soul.com',
      'game.maj-soul.net',
      'game.majsoul.com',
      'game.mahjongsoul.com',
      'mahjongsoul.game.yo-star.com',
      'mahjongsoul.game.yo-star.net',
    ].includes(new URL(shareLink(link)).hostname);
  } catch {
    return false;
  }
}

export function ImportDialog({
  api,
  busy: importing,
  error,
  onFile,
  onLink,
  onExample,
  onClose,
  onAccountBusyChange,
}: {
  api: Pick<Bridge, 'majsoulStatus' | 'loginMajsoul' | 'logoutMajsoul'>;
  busy: boolean;
  error: string;
  onFile: () => void;
  onLink: (link: string) => Promise<void>;
  onExample: (name: string, json: string) => void;
  onClose: () => void;
  onAccountBusyChange: (busy: boolean) => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [source, setSource] = useState<'file' | 'link' | 'example' | null>(null);
  const [link, setLink] = useState('');
  const [loggedIn, setLoggedIn] = useState(false);
  const [checkingAccount, setCheckingAccount] = useState(true);
  const [accountBusy, setAccountBusy] = useState(false);
  const [accountError, setAccountError] = useState('');
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [acceptRisk, setAcceptRisk] = useState(false);
  const submitting = useRef(false);
  const majsoul = isMajsoulLink(link);
  const busy = importing || accountBusy;
  const needsLogin = majsoul && !loggedIn;
  const canImport =
    Boolean(link.trim()) &&
    (!majsoul || !checkingAccount) &&
    (!needsLogin || Boolean(username.trim() && password && acceptRisk));
  const [exampleName, setExampleName] = useState(examples[0]?.filename ?? '');
  const example = examples.find((item) => item.filename === exampleName);

  useEffect(() => {
    dialog.current?.showModal();
  }, []);

  useEffect(() => {
    let active = true;
    setCheckingAccount(true);
    void api
      .majsoulStatus()
      .then((status) => {
        if (active) {
          setLoggedIn(status);
          setAccountError('');
        }
      })
      .catch((failure: unknown) => {
        if (active) {
          setLoggedIn(false);
          setAccountError(errorMessage(failure));
        }
      })
      .finally(() => {
        if (active) setCheckingAccount(false);
      });
    return () => {
      active = false;
    };
  }, [api, error]);

  useEffect(() => {
    if (source !== 'link' || !majsoul) {
      setPassword('');
      setAcceptRisk(false);
    }
  }, [source, majsoul]);

  async function submitLink() {
    if (busy || submitting.current || !canImport) return;
    submitting.current = true;
    setAccountError('');
    try {
      if (needsLogin) {
        setAccountBusy(true);
        onAccountBusyChange(true);
        const pending = api.loginMajsoul({
          username: username.trim(),
          password,
          accept_risk: acceptRisk,
        });
        setPassword('');
        await pending;
        setLoggedIn(true);
        setAccountBusy(false);
        onAccountBusyChange(false);
      }
      await onLink(shareLink(link));
    } catch (failure) {
      setAccountError(errorMessage(failure));
    } finally {
      setAccountBusy(false);
      onAccountBusyChange(false);
      submitting.current = false;
    }
  }

  async function logout() {
    if (busy || submitting.current) return;
    setAccountBusy(true);
    onAccountBusyChange(true);
    setAccountError('');
    try {
      await api.logoutMajsoul();
      setLoggedIn(false);
      setPassword('');
      setAcceptRisk(false);
    } catch (failure) {
      setAccountError(errorMessage(failure));
    } finally {
      setAccountBusy(false);
      onAccountBusyChange(false);
    }
  }

  return (
    <dialog
      ref={dialog}
      className={`import-dialog${source === 'link' && majsoul ? ' majsoul-import' : ''}`}
      aria-labelledby="import-title"
      onCancel={(event) => {
        event.preventDefault();
        if (!busy) onClose();
      }}
    >
      <div className="import-heading">
        <div>
          <h2 id="import-title">选择牌谱</h2>
          <p>选择一种方式，开始复盘。</p>
        </div>
        <button type="button" onClick={onClose} disabled={busy} aria-label="关闭导入">
          ×
        </button>
      </div>
      <div className="import-sources">
        <button
          type="button"
          autoFocus
          aria-pressed={source === 'file'}
          disabled={busy}
          onClick={() => {
            setSource('file');
            onFile();
          }}
        >
          <strong>本地文件</strong>
          <span>选择天凤 JSON 文件</span>
        </button>
        <button
          type="button"
          aria-pressed={source === 'link'}
          disabled={busy}
          onClick={() => setSource('link')}
        >
          <strong>天凤 / 雀魂链接</strong>
          <span>粘贴天凤或雀魂分享链接</span>
        </button>
        <button
          type="button"
          aria-pressed={source === 'example'}
          disabled={busy}
          onClick={() => setSource('example')}
        >
          <strong>示例牌谱</strong>
          <span>内置样本，离线体验</span>
        </button>
      </div>
      {source === 'link' && (
        <form
          className="import-detail"
          aria-label="链接导入"
          onSubmit={(event) => {
            event.preventDefault();
            void submitLink();
          }}
        >
          <label htmlFor="log-link">牌谱链接</label>
          <input
            id="log-link"
            autoFocus
            autoComplete="off"
            spellCheck={false}
            placeholder="https://tenhou.net/0/?log=…"
            value={link}
            disabled={busy}
            onChange={(event) => setLink(event.target.value)}
          />
          {majsoul && (
            <section className="majsoul-account" aria-label="雀魂登录">
              {checkingAccount ? (
                <p>正在检查登录状态…</p>
              ) : loggedIn ? (
                <div className="majsoul-session">
                  <span>已登录国际中文服 · 仅本次运行有效</span>
                  <button type="button" disabled={busy} onClick={() => void logout()}>
                    退出登录
                  </button>
                </div>
              ) : (
                <>
                  <p>国际中文服（繁体中文）账号密码登录，暂不支持其他服、第三方账号或短信登录。</p>
                  <div className="majsoul-credentials">
                    <label>
                      雀魂账号
                      <input
                        value={username}
                        onChange={(event) => setUsername(event.target.value)}
                        autoComplete="off"
                        spellCheck={false}
                        maxLength={256}
                        disabled={busy}
                      />
                    </label>
                    <label>
                      雀魂密码
                      <input
                        type="password"
                        value={password}
                        onChange={(event) => setPassword(event.target.value)}
                        autoComplete="off"
                        maxLength={1024}
                        disabled={busy}
                      />
                    </label>
                  </div>
                  <div className="majsoul-risk" id="majsoul-risk">
                    <strong>账号风险提示</strong>
                    <p>
                      本功能使用非官方接口，可能触发安全验证、账号限制甚至封禁，建议使用小号。
                      登录可能使其他客户端掉线，请勿在对局中使用。
                    </p>
                    <p>
                      账号密码仅用于本机直接登录雀魂，不发送给 Kyoku 服务，也不写入文件。
                      退出登录或关闭应用后，会话结束。
                    </p>
                  </div>
                  <label className="majsoul-consent">
                    <input
                      type="checkbox"
                      checked={acceptRisk}
                      disabled={busy}
                      aria-describedby="majsoul-risk"
                      onChange={(event) => setAcceptRisk(event.target.checked)}
                    />
                    我已了解风险，同意使用此账号登录并下载牌谱
                  </label>
                </>
              )}
            </section>
          )}
          <div className="import-actions">
            <small>
              {majsoul
                ? '支持普通四人段位场；东风场仅用于回放，Mortal 分析请使用半庄。'
                : '支持四人天凤牌谱及天凤 log ID。'}
            </small>
            <button className="primary" type="submit" disabled={busy || !canImport}>
              {accountBusy
                ? '正在连接…'
                : importing
                  ? '正在导入…'
                  : needsLogin
                    ? '登录并导入'
                    : '导入链接'}
            </button>
          </div>
        </form>
      )}
      {source === 'example' && (
        <form
          className="import-detail"
          aria-label="示例导入"
          onSubmit={(event) => {
            event.preventDefault();
            if (!busy && example) onExample(example.filename, example.json);
          }}
        >
          <label htmlFor="example-log">选择示例牌谱</label>
          <Select
            id="example-log"
            label="选择示例牌谱"
            autoFocus
            value={exampleName}
            disabled={busy}
            options={examples.map((item) => ({
              value: item.filename,
              label: item.title,
              description: item.filename.replace(/\.json$/i, ''),
            }))}
            onChange={setExampleName}
          />
          <div className="import-actions">
            <small>共 {examples.length} 份样本，部分仅含一局或数局。</small>
            <button className="primary" type="submit" disabled={busy || !example}>
              {busy ? '正在导入…' : '导入示例'}
            </button>
          </div>
          <small className="example-credit">
            样本来自 Equim / mjai-reviewer ·{' '}
            <a href={exampleLicense} download="tenhou-examples-LICENSE.txt">
              Apache-2.0 许可
            </a>
          </small>
        </form>
      )}
      {(source === null || source === 'file') && (
        <p className="import-hint">
          {busy ? '正在读取牌谱…' : '也可以直接拖入本地天凤 JSON 文件，最大 16 MiB。'}
        </p>
      )}
      {error && (
        <p role="alert" className="import-error">
          {error}
        </p>
      )}
      {source === 'link' && majsoul && accountError && (
        <p role="alert" className="import-error">
          {accountError}
        </p>
      )}
    </dialog>
  );
}
