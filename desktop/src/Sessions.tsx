import { useEffect, useRef, useState, Fragment } from 'react';
import ReactMarkdown from 'react-markdown';
import type {
  Bridge,
  SessionEvidence,
  SessionPosition,
  SessionSummary,
  SessionTurn,
  SessionView,
} from './types';
import { errorMessage } from './bridge';
import { Select } from './Select';
import { replayName, sessionTitle, SESSION_TITLE_LIMIT } from './display';

export interface SessionSource {
  game: number;
  gameKey: string;
  gameLabel: string;
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
  const scopeIds = useRef(new Map<string, Set<string>>());
  const selections = useRef(new Map<string, number>());
  const [summaries, setSummaries] = useState<SessionSummary[]>([]);
  const [pendingPositions, setPendingPositions] = useState<Record<string, SessionPosition>>({});
  const positionWrites = useRef(new Map<string, Promise<void>>());
  const positionVersions = useRef(new Map<string, number>());
  const busy = useRef(new Set<string>());

  function idFor(scope: string) {
    let id = scopes.current.get(scope);
    if (!id) {
      id = crypto.randomUUID();
      activate(scope, id);
    }
    return id;
  }
  function newSession(scope: string) {
    activate(scope, crypto.randomUUID());
    setRevision((r) => r + 1);
  }
  function activate(scope: string, id: string) {
    scopes.current.set(scope, id);
    selections.current.set(scope, (selections.current.get(scope) ?? 0) + 1);
    const ids = scopeIds.current.get(scope) ?? new Set<string>();
    ids.add(id);
    scopeIds.current.set(scope, ids);
  }
  async function attachGame(scope: string) {
    try {
      const result = await api.listSessions();
      setSummaries(result.sessions);
      const id = scopes.current.get(scope) ?? result.sessions.find((s) => s.game_key === scope)?.id;
      if (id && result.sessions.some((s) => s.id === id)) {
        const doc = await api.getSession(id);
        if (doc.game?.key !== scope) return;
        put(doc);
        activate(scope, doc.id);
        return doc;
      }
    } catch {
      // 历史读取失败时仍允许打开牌谱；历史面板提供明确错误与重试。
    }
  }
  async function selectSession(scope: string, id: string) {
    const version = (selections.current.get(scope) ?? 0) + 1;
    selections.current.set(scope, version);
    try {
      if (!documents[id] && summaries.some((s) => s.id === id)) {
        const doc = await api.getSession(id);
        if (doc.game?.key !== scope) throw new Error('会话与牌谱不匹配');
        put(doc);
      }
      if (selections.current.get(scope) !== version) return;
      activate(scope, id);
      setRevision((r) => r + 1);
    } catch (error) {
      if (selections.current.get(scope) !== version) return;
      setErrors((e) => ({ ...e, [idFor(scope)]: errorMessage(error) }));
    }
  }
  function choices(scope: string) {
    const ids = new Set([
      ...(scopeIds.current.get(scope) ?? []),
      ...summaries.filter((s) => s.game_key === scope).map((s) => s.id),
      ...Object.values(documents)
        .filter((doc) => doc.game?.key === scope)
        .map((doc) => doc.id),
    ]);
    return [...ids].map((id) => ({
      id,
      title: documents[id]?.title ?? summaries.find((s) => s.id === id)?.title ?? '新会话',
    }));
  }
  async function savePosition(id: string, source: SessionSource) {
    if (busy.current.has(id)) return;
    const version = (positionVersions.current.get(id) ?? 0) + 1;
    positionVersions.current.set(id, version);
    const previous = positionWrites.current.get(id) ?? Promise.resolve();
    const write = previous
      .catch(() => undefined)
      .then(async () => {
        if (positionVersions.current.get(id) !== version || busy.current.has(id)) return;
        await api.setSessionPosition(id, source.gameKey, {
          player: source.player,
          event_index: source.event,
        });
      });
    positionWrites.current.set(id, write);
    try {
      await write;
    } catch (error) {
      setErrors((e) => ({ ...e, [id]: errorMessage(error) }));
    } finally {
      if (positionWrites.current.get(id) === write) positionWrites.current.delete(id);
    }
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
  async function rename(id: string, title: string) {
    if (busy.current.has(id)) return false;
    busy.current.add(id);
    setErrors((e) => ({ ...e, [id]: '' }));
    try {
      await positionWrites.current.get(id)?.catch(() => undefined);
      put(await api.renameSession(id, title));
      return true;
    } catch (error) {
      setErrors((e) => ({ ...e, [id]: errorMessage(error) }));
      return false;
    } finally {
      busy.current.delete(id);
    }
  }
  function ask(id: string, source: SessionSource | undefined, text: string) {
    return send(id, source, text);
  }
  function retry(id: string, turn?: number) {
    const doc = documents[id];
    const text = turn === undefined ? doc?.pending_question : doc?.archive.turns[turn]?.question;
    if (text) return send(id, undefined, text, { turn });
  }
  async function send(
    id: string,
    source: SessionSource | undefined,
    text: string,
    retryTarget?: { turn?: number },
  ) {
    text = text.trim();
    if (!text || busy.current.has(id) || (!documents[id] && !source)) return;
    busy.current.add(id);
    const position = retryTarget
      ? retryTarget.turn === undefined
        ? documents[id]?.archive.evidence
        : (documents[id]?.archive.turns[retryTarget.turn]?.evidence ??
          documents[id]?.archive.evidence)
      : source
        ? { player: source.player, event_index: source.event }
        : (documents[id]?.position ?? documents[id]?.archive.evidence);
    if (position) setPendingPositions((p) => ({ ...p, [id]: position }));
    setPending((p) => ({ ...p, [id]: text }));
    setErrors((e) => ({ ...e, [id]: '' }));
    draft(id, '');
    try {
      await positionWrites.current.get(id)?.catch(() => undefined);
      const result = retryTarget
        ? await api.retrySession(id, retryTarget.turn)
        : source
          ? await api.ask(source.game, source.player, source.event, id, text, source.gameLabel)
          : await api.continueSession(id, text);
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
  return {
    documents,
    drafts,
    pending,
    pendingPositions,
    errors,
    revision,
    idFor,
    newSession,
    put,
    draft,
    rename,
    ask,
    retry,
    attachGame,
    activate,
    choices,
    selectSession,
    savePosition,
  };
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
              <summary className="trace-step">
                <span className="trace-number" aria-hidden="true">
                  {i + 1}
                </span>
                <span>
                  {names[step.kind] ?? step.kind}
                  {step.kind === 'tool' ? ` · ${String(step.name)}` : ''}
                </span>
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

function positionLabel(evidence: SessionEvidence) {
  const round = evidence.position?.round;
  const winds: Record<string, string> = { E: '东', S: '南', W: '西', N: '北' };
  const label = round
    ? `${winds[round.wind] ?? round.wind}${round.number}局 · ${evidence.position?.honba}本场 · `
    : '';
  return `${label}玩家 ${evidence.player} · G${evidence.event_index}`;
}

function Location({
  evidence,
  onLocate,
}: {
  evidence: SessionEvidence;
  onLocate?: (position: SessionPosition) => void;
}) {
  return onLocate ? (
    <button
      className="session-location"
      title="回到提问时的局面"
      onClick={() => onLocate(evidence)}
    >
      {positionLabel(evidence)}
    </button>
  ) : (
    <small className="session-location">{positionLabel(evidence)}</small>
  );
}

function SessionTitle({
  id,
  title,
  sessions,
  onSelect,
  onRename,
  editable,
}: {
  id: string;
  title: string;
  sessions?: { id: string; title: string }[];
  onSelect?: (id: string) => void;
  onRename: (title: string) => Promise<boolean>;
  editable: boolean;
}) {
  const [editing, setEditing] = useState(false);
  const [value, setValue] = useState('');
  const [saving, setSaving] = useState(false);
  if (editing)
    return (
      <form
        className="session-picker session-title-editor"
        onSubmit={async (event) => {
          event.preventDefault();
          if (saving || !editable || !value.trim()) return;
          setSaving(true);
          if (await onRename(value.trim())) setEditing(false);
          setSaving(false);
        }}
        onKeyDown={(event) => {
          if (event.key === 'Escape') {
            event.preventDefault();
            event.stopPropagation();
            if (!saving) setEditing(false);
          }
        }}
      >
        <input
          aria-label="会话标题"
          autoFocus
          value={value}
          disabled={saving}
          onChange={(event) =>
            setValue(Array.from(event.target.value).slice(0, SESSION_TITLE_LIMIT).join(''))
          }
        />
        <small>
          {Array.from(value).length}/{SESSION_TITLE_LIMIT}
        </small>
        <button type="submit" disabled={saving || !editable || !value.trim()}>
          保存
        </button>
        <button type="button" disabled={saving} onClick={() => setEditing(false)}>
          取消
        </button>
      </form>
    );
  return (
    <div className="session-picker">
      {sessions && onSelect ? (
        <Select
          label="当前会话"
          value={id}
          options={sessions.map((session) => ({
            value: session.id,
            label: sessionTitle(session.title),
          }))}
          onChange={onSelect}
        />
      ) : (
        <span className="session-title">{sessionTitle(title)}</span>
      )}
      {editable && (
        <button
          className="session-rename"
          aria-label="修改会话标题"
          title="修改会话标题"
          onClick={() => {
            setValue(sessionTitle(title));
            setEditing(true);
          }}
        >
          <svg aria-hidden="true" viewBox="0 0 24 24">
            <path d="m15 5 4 4M4 20l5-1L20 8a2.8 2.8 0 0 0-4-4L5 15l-1 5Z" />
          </svg>
        </button>
      )}
    </div>
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
  sessions,
  onSelect,
  onLocate,
}: {
  workspace: Workspace;
  id: string;
  source?: SessionSource;
  ready?: boolean;
  visible?: boolean;
  onFocus?: () => void;
  onNew?: () => void;
  sessions?: { id: string; title: string }[];
  onSelect?: (id: string) => void;
  onLocate?: (position: SessionPosition) => void;
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
        <span className="agent-icon" aria-hidden="true">
          <svg viewBox="0 0 24 24">
            <path d="M12 3C11 9 9 11 3 12c6 1 8 3 9 9 1-6 3-8 9-9-6-1-8-3-9-9Z" />
          </svg>
        </span>
        <div>
          <h2>一起复盘</h2>
          <p>
            {source?.label ??
              (doc ? positionLabel(doc.position ?? doc.archive.evidence) : '围绕这份牌谱展开讨论')}
          </p>
        </div>
        {onNew && (
          <button className="session-new" disabled={!canAsk} onClick={onNew}>
            新建会话
          </button>
        )}
      </div>
      {(sessions || doc) && (
        <SessionTitle
          key={id}
          id={id}
          title={doc?.title ?? '新会话'}
          sessions={sessions}
          onSelect={onSelect}
          onRename={(title) => w.rename(id, title)}
          editable={!!doc && !pending}
        />
      )}
      <div className="chat-context">
        <span className="status-dot" />
        {doc ? '会话已自动保存 · 每次提问使用发送时的局面' : '仅使用所选玩家当时可见的信息'}
      </div>
      {doc && (
        <details className="session-context">
          <summary>会话上下文 · {doc.archive.model}</summary>
          <p>{doc.archive.endpoint}</p>
          <details>
            <summary>最近提问的局面证据</summary>
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
            {canAsk && ready && (
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
              {!canAsk
                ? '导入牌谱后即可提问。'
                : !ready
                  ? '可以直接提问；当前局面暂无 Mortal 决策结果。'
                  : '回答会区分计算、Mortal 与推测。'}
            </small>
          </div>
        )}
        {turns.map((turn, i) => (
          <Fragment key={i}>
            <div className="message user">
              <span className="message-author">你</span>
              <Location evidence={turn.evidence ?? doc!.archive.evidence} onLocate={onLocate} />
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
                <button disabled={!!pending} onClick={() => void w.retry(id, i)}>
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
              {w.pendingPositions[id] && (
                <Location evidence={w.pendingPositions[id]} onLocate={onLocate} />
              )}
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
            <button onClick={() => void w.retry(id)}>重试未完成的问题</button>
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
          placeholder={canAsk ? '聊聊这份牌谱或当前局面…' : '导入牌谱后提问…'}
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
  onOpenGame,
}: {
  api: Bridge;
  workspace: Workspace;
  onClose: () => void;
  onOpenGame: (doc: SessionView, position?: SessionPosition) => Promise<void>;
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
  async function openGame(position?: SessionPosition) {
    if (!selected || loading) return;
    setError('');
    setLoading(true);
    try {
      await onOpenGame(w.documents[selected], position);
    } catch (error) {
      setError(errorMessage(error));
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
        {selected && w.documents[selected]?.game && (
          <button disabled={loading} onClick={() => void openGame()}>
            打开牌谱并继续
          </button>
        )}
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
              <strong title={sessionTitle(s.title)}>{sessionTitle(s.title)}</strong>
              <span>{replayName(s.context_label)}</span>
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
            <ChatPanel
              workspace={w}
              id={selected}
              onLocate={
                w.documents[selected]?.game ? (position) => void openGame(position) : undefined
              }
            />
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
