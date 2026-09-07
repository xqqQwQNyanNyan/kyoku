import { useEffect, useRef, useState, Fragment } from 'react';
import ReactMarkdown from 'react-markdown';
import type { Bridge, SessionSummary, SessionTurn, SessionView } from './types';
import { errorMessage } from './bridge';

export interface SessionSource {
  game: number;
  player: number;
  event: number;
  label: string;
}

export function useSessions(api: Bridge) {
  const [documents, setDocuments] = useState<Record<string, SessionView>>({});
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const [pending, setPending] = useState<Record<string, string>>({});
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [revision, setRevision] = useState(0);
  const scopes = useRef(new Map<string, string>());
  const busy = useRef(new Set<string>());

  function idFor(scope: string) {
    let id = scopes.current.get(scope);
    if (!id) {
      id = crypto.randomUUID();
      scopes.current.set(scope, id);
    }
    return id;
  }
  function newSession(scope: string) {
    scopes.current.set(scope, crypto.randomUUID());
    setRevision((r) => r + 1);
  }
  function put(document: SessionView) {
    setDocuments((current) => {
      if (current[document.id]?.updated_at > document.updated_at) return current;
      return { ...current, [document.id]: document };
    });
    setRevision((r) => r + 1);
  }
  function draft(id: string, text: string) {
    setDrafts((current) => ({ ...current, [id]: text }));
  }
  async function ask(id: string, source: SessionSource | undefined, text: string) {
    text = text.trim();
    if (!text || busy.current.has(id) || (!documents[id] && !source)) return;
    busy.current.add(id);
    setPending((p) => ({ ...p, [id]: text }));
    setErrors((e) => ({ ...e, [id]: '' }));
    draft(id, '');
    try {
      const result = documents[id]
        ? await api.continueSession(id, text)
        : await api.ask(source!.game, source!.player, source!.event, id, text, source!.label);
      put(result);
      if (result.archive.turns.at(-1)?.error) draft(id, text);
    } catch (error) {
      setErrors((e) => ({ ...e, [id]: errorMessage(error) }));
      draft(id, text);
      // 创建或保存已成功但后续步骤失败时，恢复磁盘上已有的会话，避免重复创建。
      try {
        put(await api.getSession(id));
      } catch {
        /* 可能尚未创建会话。 */
      }
    } finally {
      busy.current.delete(id);
      setPending((p) => {
        const next = { ...p };
        delete next[id];
        return next;
      });
    }
  }
  return { documents, drafts, pending, errors, revision, idFor, newSession, put, draft, ask };
}

type Workspace = ReturnType<typeof useSessions>;

function Markdown({ text }: { text: string }) {
  return (
    <ReactMarkdown
      components={{
        a: ({ children }) => <span>{children}</span>,
        img: ({ alt }) => <span>{alt}</span>,
      }}
    >
      {text}
    </ReactMarkdown>
  );
}

function Trace({ turn }: { turn: SessionTurn }) {
  const names: Record<string, string> = {
    request: '请求模型',
    response: '模型返回',
    tool: '执行工具',
    validation: '回答校验未通过',
    reference_repair: '本地修正证据引用',
  };
  return (
    <details className="session-trace">
      <summary>
        工作流程 · {turn.trace.filter((t) => t.kind === 'request').length} 次请求
        {turn.error ? ' · 失败' : ' · 完成'}
      </summary>
      <ol>
        {turn.trace.map((step, i) => (
          <li key={i}>
            <details>
              <summary>
                {names[step.kind] ?? step.kind}
                {step.kind === 'tool' ? ` · ${String(step.name)}` : ''}
              </summary>
              <pre>
                {JSON.stringify(
                  step,
                  (key, value) => {
                    // 推理内容由供应商管理；轨迹面板展示可观察的输入输出与工具执行。
                    if (['encrypted_content', 'reasoning_content'].includes(key))
                      return '[已保留在会话文件中]';
                    return value;
                  },
                  2,
                )}
              </pre>
            </details>
          </li>
        ))}
      </ol>
    </details>
  );
}

export function ChatPanel({
  workspace: w,
  id,
  source,
  ready = true,
  visible = true,
  onFocus,
  onNew,
}: {
  workspace: Workspace;
  id: string;
  source?: SessionSource;
  ready?: boolean;
  visible?: boolean;
  onFocus?: () => void;
  onNew?: () => void;
}) {
  const doc = w.documents[id];
  const turns = doc?.archive.turns ?? [];
  const pending = w.pending[id] || (doc?.busy ? doc.pending_question : null);
  const canAsk = !!(doc || source);
  const question = w.drafts[id] ?? '';
  const end = useRef<HTMLDivElement>(null);
  const needsScroll = useRef(true);
  useEffect(() => {
    needsScroll.current = true;
  }, [id, doc, pending]);
  useEffect(() => {
    const container = end.current?.parentElement;
    if (!container || !visible || !needsScroll.current) return;
    needsScroll.current = false;
    if (!turns.length && !pending) container.scrollTop = 0;
    else container.scrollTo({ top: container.scrollHeight, behavior: 'smooth' });
  }, [id, doc, pending, visible, turns.length]);
  const submit = (text: string) => {
    onFocus?.();
    void w.ask(id, source, text);
  };

  return (
    <section className="chat-panel" aria-label="局面问答">
      <div className="chat-heading">
        <span className="agent-icon">✧</span>
        <div>
          <h2>一起复盘</h2>
          <p>{doc?.context_label ?? source?.label ?? '围绕当前决策展开讨论'}</p>
        </div>
        {onNew && (
          <button className="session-new" disabled={!canAsk || !!pending} onClick={onNew}>
            新建会话
          </button>
        )}
      </div>
      <div className="chat-context">
        <span className="status-dot" />
        {doc ? '会话已自动保存 · 使用此会话固定的局面证据' : '仅使用所选玩家当时可见的信息'}
      </div>
      {doc && (
        <details className="session-context">
          <summary>会话上下文 · {doc.archive.model}</summary>
          <p>{doc.archive.endpoint}</p>
          <details>
            <summary>固定局面证据</summary>
            <pre>{JSON.stringify(doc.archive.evidence, null, 2)}</pre>
          </details>
          <details>
            <summary>工具定义</summary>
            <pre>{JSON.stringify(doc.archive.tools, null, 2)}</pre>
          </details>
          <details>
            <summary>系统提示词</summary>
            <pre>{doc.archive.instructions}</pre>
          </details>
        </details>
      )}
      <div className="messages" role="log" aria-label="复盘对话" aria-live="polite">
        {!turns.length && !pending && (
          <div className="chat-welcome">
            <h3>这一步，你在想什么？</h3>
            <p>
              对比候选切牌，理解模型倾向，
              <br />
              也可以说说你当时的考虑。
            </p>
            {canAsk && (
              <>
                <button
                  onClick={() => submit('比较这里的候选切牌，说明向听、进张和 Mortal 的倾向。')}
                >
                  这里的几个选择差在哪里？ <span>↗</span>
                </button>
                <button
                  onClick={() => submit('Mortal 推荐了什么？哪些结论有计算依据，哪些只能推测？')}
                >
                  帮我读懂 Mortal 的推荐 <span>↗</span>
                </button>
              </>
            )}
            <small>
              {!ready
                ? '先分析牌谱，再选择一个决策点。'
                : !canAsk
                  ? '用「下一决策」前往可提问的局面。'
                  : '回答会区分计算、Mortal 与推测。'}
            </small>
          </div>
        )}
        {turns.map((turn, i) => (
          <Fragment key={i}>
            <div className="message user">
              <span className="message-author">你</span>
              <Markdown text={turn.question} />
            </div>
            <Trace turn={turn} />
            {turn.answer && (
              <div className="message assistant">
                <span className="message-author">KYOKU</span>
                <Markdown text={turn.answer} />
              </div>
            )}
            {turn.error && (
              <div className="chat-error" role="alert">
                {turn.error}
                <button disabled={!!pending} onClick={() => submit(turn.question)}>
                  重试此问题
                </button>
              </div>
            )}
          </Fragment>
        ))}
        {pending && (
          <>
            <div className="message user">
              <span className="message-author">你</span>
              <Markdown text={pending} />
            </div>
            <div className="thinking">
              <span className="status-dot" />
              正在生成回答…可以切换局面，结果会保留在此会话。
            </div>
          </>
        )}
        {doc?.pending_question && !pending && (
          <div className="chat-error" role="alert">
            上次请求未完成：{doc.pending_question}
            <button onClick={() => submit(doc.pending_question!)}>重试未完成的问题</button>
          </div>
        )}
        {w.errors[id] && (
          <div role="alert" className="chat-error">
            {w.errors[id]}
          </div>
        )}
        <div ref={end} />
      </div>
      <form
        className="composer"
        onSubmit={(e) => {
          e.preventDefault();
          submit(question);
        }}
      >
        <textarea
          aria-label="复盘问题"
          placeholder={canAsk ? '问问这个局面…' : '选择一个决策点后提问…'}
          value={question}
          disabled={!canAsk || !!pending}
          rows={2}
          onChange={(e) => w.draft(id, e.target.value)}
          onFocus={onFocus}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing) {
              e.preventDefault();
              submit(question);
            }
          }}
        />
        <div>
          <small>Enter 发送 · Shift Enter 换行</small>
          <button
            aria-label="发送问题"
            type="submit"
            disabled={!canAsk || !!pending || !question.trim()}
          >
            ↑
          </button>
        </div>
      </form>
      <p className="chat-footnote">切换局面保留会话，可从「历史会话」找回或导出。</p>
    </section>
  );
}

export function HistoryDialog({
  api,
  workspace: w,
  onClose,
}: {
  api: Bridge;
  workspace: Workspace;
  onClose: () => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const input = useRef<HTMLInputElement>(null);
  const [sessions, setSessions] = useState<SessionSummary[]>([]);
  const [listing, setListing] = useState(true);
  const [listError, setListError] = useState('');
  const [listRevision, setListRevision] = useState(0);
  const [selected, setSelected] = useState<string | null>(null);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const [loading, setLoading] = useState(false);
  const selection = useRef(0);
  const listRequest = useRef<ReturnType<Bridge['listSessions']> | null>(null);
  useEffect(() => {
    dialog.current?.showModal();
  }, []);
  useEffect(() => {
    let active = true;
    let timer: number | undefined;
    setListing(true);
    const refresh = async () => {
      // 状态变化重启 effect 时，也要等上一次读取完成，不能积压后台请求。
      if (listRequest.current) await listRequest.current.catch(() => undefined);
      if (!active) return;
      const request = api.listSessions();
      listRequest.current = request;
      try {
        const result = await request;
        if (active) {
          setSessions(result.sessions);
          setListError(result.warnings.join('；'));
        }
      } catch (e) {
        if (active) setListError(errorMessage(e));
      } finally {
        if (listRequest.current === request) listRequest.current = null;
        if (active) setListing(false);
        if (active && Object.keys(w.pending).length) {
          timer = window.setTimeout(() => void refresh(), 1500);
        }
      }
    };
    void refresh();
    return () => {
      active = false;
      window.clearTimeout(timer);
    };
  }, [api, w.revision, w.pending, listRevision]);
  async function open(id: string) {
    const job = ++selection.current;
    setError('');
    setLoading(true);
    try {
      const doc = await api.getSession(id);
      if (selection.current === job) {
        w.put(doc);
        setSelected(id);
      }
    } catch (e) {
      if (selection.current === job) setError(errorMessage(e));
    } finally {
      if (selection.current === job) setLoading(false);
    }
  }
  // 从磁盘打开的进行中任务在完成后刷新；不会重新发起模型请求。
  useEffect(() => {
    if (!selected || !w.documents[selected]?.busy || w.pending[selected]) return;
    const timer = window.setTimeout(() => void open(selected), 1500);
    return () => window.clearTimeout(timer);
  }, [selected, w.documents, w.pending]);
  async function importFile(file: File | undefined) {
    if (!file) return;
    if (file.size > 32 * 1024 * 1024) {
      setError('会话文件不能超过 32 MiB');
      return;
    }
    ++selection.current;
    setLoading(true);
    setError('');
    try {
      const doc = await api.importSession(await file.text());
      w.put(doc);
      setSelected(doc.id);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setLoading(false);
    }
  }
  async function exportFile() {
    if (!selected) return;
    setError('');
    try {
      setNotice(`已导出到：${await api.exportSession(selected)}`);
    } catch (e) {
      setError(errorMessage(e));
    }
  }
  return (
    <dialog
      ref={dialog}
      className="history-dialog"
      aria-labelledby="history-title"
      onCancel={(e) => {
        e.preventDefault();
        onClose();
      }}
    >
      <div className="history-heading">
        <div>
          <h2 id="history-title">历史会话</h2>
          <p>独立保存每次复盘的上下文与工作流程</p>
        </div>
        <button onClick={() => input.current?.click()} disabled={loading}>
          加载 JSON
        </button>
        <button onClick={() => void exportFile()} disabled={!selected}>
          导出 JSON
        </button>
        <button aria-label="关闭历史会话" onClick={onClose}>
          ×
        </button>
      </div>
      <input
        ref={input}
        className="file-input"
        aria-label="选择会话 JSON 文件"
        type="file"
        accept=".json,application/json"
        onChange={(e) => {
          void importFile(e.target.files?.[0]);
          e.target.value = '';
        }}
      />
      {error && (
        <p className="chat-error" role="alert">
          {error}
        </p>
      )}
      {notice && (
        <p className="session-notice" role="status">
          {notice}
        </p>
      )}
      {listError && (
        <p className="chat-error" role="alert">
          {listError}
          <button disabled={listing} onClick={() => setListRevision((r) => r + 1)}>
            重新读取历史
          </button>
        </p>
      )}
      <div className="history-body">
        <nav aria-label="历史会话列表">
          {!sessions.length && (
            <p className="muted">
              {listing
                ? '正在读取历史会话…'
                : listError
                  ? '历史会话未能完整读取。'
                  : '暂无历史会话。开始一次问答，或加载已有 JSON。'}
            </p>
          )}
          {sessions.map((s) => (
            <button
              key={s.id}
              aria-current={s.id === selected ? 'true' : undefined}
              onClick={() => void open(s.id)}
            >
              <strong>{s.title}</strong>
              <span>{s.context_label}</span>
              <small>
                {new Date(s.updated_at).toLocaleString()} ·{' '}
                {s.busy
                  ? '进行中'
                  : s.interrupted
                    ? '未完成'
                    : s.failed
                      ? '失败 · 可重试'
                      : '已保存'}
              </small>
            </button>
          ))}
        </nav>
        <div className="history-content">
          {loading && <p role="status">正在读取会话…</p>}
          {selected ? (
            <ChatPanel workspace={w} id={selected} />
          ) : (
            <div className="history-empty">
              选择一个会话，查看记录、工作流程或继续追问。
              <small>无需重新导入牌谱或运行 Mortal。</small>
            </div>
          )}
        </div>
      </div>
    </dialog>
  );
}
