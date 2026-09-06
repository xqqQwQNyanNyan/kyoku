import { useEffect, useRef, useState } from 'react';
import ReactMarkdown from 'react-markdown';
import type { Bridge, Decision, Replay } from './types';
import { bridge, errorMessage } from './bridge';
import { Board, eventText } from './Board';
import { Analysis } from './Analysis';
import { Tile } from './Tile';

type Message = { role: 'user' | 'assistant'; text: string };

export default function App({ api = bridge }: { api?: Bridge }) {
  const [replay, setReplay] = useState<Replay | null>(null);
  const [filename, setFilename] = useState('');
  const [index, setIndex] = useState(0);
  const [player, setPlayer] = useState(0);
  const [reveal, setReveal] = useState(false);
  const [playing, setPlaying] = useState(false);
  const [speed, setSpeed] = useState(1);
  const [selected, setSelected] = useState<string | null>(null);
  const [decisions, setDecisions] = useState<Record<number, Decision[]>>({});
  const [analyzing, setAnalyzing] = useState<number | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const [messages, setMessages] = useState<Message[]>([]);
  const [question, setQuestion] = useState('');
  const [answerContext, setAnswerContext] = useState<string | null>(null);
  const asking = answerContext !== null;
  const [chatError, setChatError] = useState('');
  const fileInput = useRef<HTMLInputElement>(null);
  const chatEnd = useRef<HTMLDivElement>(null);
  const conversation = useRef(crypto.randomUUID());
  const documentId = useRef<number | null>(null);
  const analysisJob = useRef(0);
  const importBusy = useRef(false);
  const questionBusy = useRef(false);
  const frame = replay?.frames[index];
  const points = decisions[player];
  const decision = frame ? points?.find((d) => d.event_index === frame.event_index) : undefined;
  const activeRound = replay?.rounds.findLastIndex((round) => round.frame_index <= index) ?? -1;

  function resetConversation() {
    conversation.current = crypto.randomUUID();
    setMessages([]);
    setQuestion('');
    setChatError('');
    setSelected(null);
  }

  function selectFrame(next: number) {
    if (!replay) return;
    next = Math.max(0, Math.min(replay.frames.length - 1, next));
    if (next !== index) {
      resetConversation();
      setIndex(next);
    }
  }

  function jump(next: number) {
    setPlaying(false);
    selectFrame(next);
  }

  function changePlayer(next: number) {
    if (next === player) return;
    setPlaying(false);
    resetConversation();
    setPlayer(next);
  }

  async function importFile(file: File | undefined) {
    if (!file || importBusy.current) return;
    if (file.size > 16 * 1024 * 1024) {
      setError('牌谱文件不能超过 16 MiB');
      return;
    }
    importBusy.current = true;
    setLoading(true);
    setPlaying(false);
    setError('');
    try {
      const loaded = await api.importLog(await file.text());
      documentId.current = loaded.id;
      analysisJob.current += 1;
      resetConversation();
      setReplay(loaded);
      setFilename(file.name);
      setIndex(0);
      setPlayer(0);
      setReveal(false);
      setDecisions({});
      setAnalyzing(null);
    } catch (error) {
      setError(errorMessage(error));
    } finally {
      setLoading(false);
      importBusy.current = false;
    }
  }

  async function analyze() {
    if (!replay || analyzing !== null) return;
    const id = replay.id;
    const perspective = player;
    const job = ++analysisJob.current;
    setAnalyzing(perspective);
    setError('');
    try {
      const result = await api.analyze(id, perspective);
      if (documentId.current === id)
        setDecisions((current) => ({ ...current, [perspective]: result }));
    } catch (error) {
      if (documentId.current === id) setError(errorMessage(error));
    } finally {
      if (analysisJob.current === job) setAnalyzing(null);
    }
  }

  async function ask(text: string) {
    text = text.trim();
    if (!replay || !frame || !decision || !text || questionBusy.current) return;
    const key = conversation.current;
    questionBusy.current = true;
    setAnswerContext(key);
    setPlaying(false);
    setChatError('');
    setQuestion('');
    setMessages((current) => [...current, { role: 'user', text }]);
    try {
      const answer = await api.ask(replay.id, player, frame.event_index, key, text);
      if (conversation.current === key)
        setMessages((current) => [...current, { role: 'assistant', text: answer }]);
    } catch (error) {
      if (conversation.current === key) {
        setChatError(errorMessage(error));
        setQuestion(text);
        setMessages((current) => current.slice(0, -1));
      }
    } finally {
      questionBusy.current = false;
      setAnswerContext(null);
    }
  }

  function nextDecision(direction: -1 | 1) {
    if (!replay || !frame || !points) return;
    const point =
      direction === 1
        ? points.find((d) => d.event_index > frame.event_index)
        : points.findLast((d) => d.event_index < frame.event_index);
    if (point) jump(replay.frames.findIndex((f) => f.event_index === point.event_index));
  }

  useEffect(() => {
    if (!playing || !replay) return;
    if (index >= replay.frames.length - 1) {
      setPlaying(false);
      return;
    }
    const timer = window.setTimeout(() => selectFrame(index + 1), 850 / speed);
    return () => window.clearTimeout(timer);
  });

  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      if (!replay || event.altKey || event.ctrlKey || event.metaKey || event.isComposing) return;
      if (
        (event.target as HTMLElement).closest(
          'input, textarea, select, button, [contenteditable=true]',
        )
      )
        return;
      if (event.code === 'Space') {
        event.preventDefault();
        setPlaying((current) => !current);
      }
      if (event.code === 'ArrowLeft') {
        event.preventDefault();
        jump(index - 1);
      }
      if (event.code === 'ArrowRight') {
        event.preventDefault();
        jump(index + 1);
      }
    }
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });

  useEffect(() => {
    chatEnd.current?.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
  }, [messages, asking]);

  const openFile = () => fileInput.current?.click();

  return (
    <div
      className="app"
      onDragOver={(event) => {
        event.preventDefault();
      }}
      onDrop={(event) => {
        event.preventDefault();
        void importFile(event.dataTransfer.files[0]);
      }}
    >
      <input
        ref={fileInput}
        className="file-input"
        type="file"
        accept=".json,application/json"
        aria-label="选择天凤牌谱文件"
        disabled={loading}
        onChange={(event) => {
          void importFile(event.target.files?.[0]);
          event.target.value = '';
        }}
      />
      <header className="app-header">
        <div className="brand">
          <span className="brand-mark">局</span>
          <div>
            <strong>Kyoku</strong>
            <span>日麻牌谱复盘</span>
          </div>
        </div>
        <div className="document-title">
          {replay ? (
            <>
              <span className="status-dot" />
              {filename}
            </>
          ) : (
            '从一份牌谱，重新看懂每一步。'
          )}
        </div>
        {replay && (
          <label className="perspective">
            复盘玩家{' '}
            <select
              aria-label="复盘玩家"
              value={player}
              onChange={(e) => changePlayer(Number(e.target.value))}
            >
              {replay.names.map((name, i) => (
                <option key={i} value={i}>
                  {name}
                </option>
              ))}
            </select>
          </label>
        )}
        <button className="import-button" disabled={loading} onClick={openFile}>
          {loading ? '正在读取…' : '＋ 导入牌谱'}
        </button>
      </header>
      {error && (
        <div role="alert" className="error-banner">
          <span>{error}</span>
          <button onClick={() => setError('')} aria-label="关闭错误提示">
            ×
          </button>
        </div>
      )}
      {!replay || !frame ? (
        <main className="welcome">
          <div className="welcome-art">
            <Tile tile="1m" />
            <Tile tile="9p" />
            <Tile tile="F" />
          </div>
          <span className="eyebrow">REPLAY · COMPARE · UNDERSTAND</span>
          <h1>
            再看一局，
            <br />
            <em>多懂一步。</em>
          </h1>
          <p>
            回到每一次摸打，比较当时的选择，
            <br />和 Agent 一起梳理你的判断。
          </p>
          <button className="primary large" onClick={openFile} disabled={loading}>
            {loading ? '正在读取牌谱…' : '选择牌谱文件'} <span>↗</span>
          </button>
          <span className="welcome-hint">支持本地天凤 JSON · 也可以拖入文件</span>
          <div className="welcome-footer">
            <span>01 完整牌局回放</span>
            <span>02 Mortal 动作对比</span>
            <span>03 局面问答</span>
          </div>
        </main>
      ) : (
        <main className="workspace">
          <aside className="navigation">
            <div className="panel-heading">
              <h2>牌局</h2>
              <span className="muted">{replay.rounds.length} 局</span>
            </div>
            <div className="round-list">
              {replay.rounds.map((round, i) => (
                <button
                  key={i}
                  className={activeRound === i ? 'active' : ''}
                  aria-current={activeRound === i ? 'step' : undefined}
                  onClick={() => jump(round.frame_index)}
                >
                  <span>{round.label.split(' · ')[0]}</span>
                  <small>{round.label.split(' · ')[1]}</small>
                  <span className="round-arrow">›</span>
                </button>
              ))}
            </div>
            <div className="nav-footer">
              <span className="eyebrow">REVIEW</span>
              <strong>{replay.names[player]}</strong>
              <p>
                {points
                  ? `${points.length} 个决策点已就绪`
                  : analyzing === player
                    ? '正在分析整场…'
                    : '分析后可按决策跳转'}
              </p>
              <button
                className="secondary"
                onClick={() => void analyze()}
                disabled={analyzing !== null || !!points}
              >
                {points ? '分析已完成' : analyzing !== null ? '分析中…' : '分析此玩家'}
              </button>
            </div>
          </aside>
          <div className="replay-column">
            <section className="table-stage">
              <div className="table-toolbar">
                <span className="eyebrow">
                  {frame.round} / {frame.honba} 本场
                </span>
                <label>
                  <input
                    type="checkbox"
                    checked={reveal}
                    onChange={(e) => setReveal(e.target.checked)}
                  />
                  显示全部手牌
                </label>
              </div>
              <Board
                frame={frame}
                names={replay.names}
                player={player}
                reveal={reveal}
                selected={selected}
                onSelect={(tile) => setSelected(tile === selected ? null : tile)}
              />
              <div className="table-caption">
                <span className="event-caption" data-testid="event-caption">
                  {eventText(frame, replay.names, player, reveal)}
                </span>
                <span className="muted">半透明牌：摸切 · 虚线牌：已被鸣走</span>
              </div>
            </section>
            <section className="transport" aria-label="回放控制">
              <input
                type="range"
                min="0"
                max={replay.frames.length - 1}
                value={index}
                aria-label="牌谱进度"
                onChange={(e) => jump(Number(e.target.value))}
              />
              <div className="transport-buttons">
                <span className="event-counter">
                  G{String(frame.event_index).padStart(3, '0')}{' '}
                  <small>/ {replay.frames.at(-1)?.event_index}</small>
                </span>
                <div className="playback-buttons">
                  <button
                    aria-label="上一事件"
                    title="上一事件 ←"
                    onClick={() => jump(index - 1)}
                    disabled={index === 0}
                  >
                    ‹
                  </button>
                  <button
                    className="play"
                    aria-label={playing ? '暂停' : '播放'}
                    onClick={() => setPlaying(!playing)}
                    disabled={index === replay.frames.length - 1}
                  >
                    {playing ? 'Ⅱ' : '▶'}
                  </button>
                  <button
                    aria-label="下一事件"
                    title="下一事件 →"
                    onClick={() => jump(index + 1)}
                    disabled={index === replay.frames.length - 1}
                  >
                    ›
                  </button>
                  <select
                    aria-label="播放速度"
                    value={speed}
                    onChange={(e) => setSpeed(Number(e.target.value))}
                  >
                    <option value="0.5">0.5×</option>
                    <option value="1">1×</option>
                    <option value="2">2×</option>
                    <option value="4">4×</option>
                  </select>
                </div>
                <div className="decision-buttons">
                  <button
                    onClick={() => nextDecision(-1)}
                    disabled={!points?.some((d) => d.event_index < frame.event_index)}
                  >
                    上一决策
                  </button>
                  <button
                    onClick={() => nextDecision(1)}
                    disabled={!points?.some((d) => d.event_index > frame.event_index)}
                  >
                    下一决策 ›
                  </button>
                </div>
              </div>
            </section>
            <Analysis
              decision={decision}
              ready={!!points}
              busy={analyzing !== null}
              selected={selected}
              onSelect={setSelected}
              onAnalyze={() => void analyze()}
            />
          </div>
          <aside className="chat-panel">
            <div className="chat-heading">
              <span className="agent-icon">✧</span>
              <div>
                <h2>一起复盘</h2>
                <p>
                  {decision ? `${frame.round} · 第 ${decision.turn} 手` : '围绕当前决策展开讨论'}
                </p>
              </div>
              <span className="local-badge">Agent</span>
            </div>
            <div className="chat-context">
              <span className="status-dot" />
              <span>仅使用所选玩家当时可见的信息</span>
            </div>
            <div className="messages" aria-live="polite">
              {messages.length === 0 && (
                <div className="chat-welcome">
                  <div className="chat-orbit">✧</div>
                  <h3>这一步，你在想什么？</h3>
                  <p>
                    对比候选切牌，理解模型倾向，
                    <br />
                    也可以说说你当时的考虑。
                  </p>
                  <button
                    disabled={!decision || asking}
                    onClick={() => void ask('比较这里的候选切牌，说明向听、进张和 Mortal 的倾向。')}
                  >
                    这里的几个选择差在哪里？ <span>↗</span>
                  </button>
                  <button
                    disabled={!decision || asking}
                    onClick={() =>
                      void ask('Mortal 推荐了什么？哪些结论有计算依据，哪些只能推测？')
                    }
                  >
                    帮我读懂 Mortal 的推荐 <span>↗</span>
                  </button>
                  <small>
                    {!points
                      ? '先分析牌谱，再选择一个决策点。'
                      : !decision
                        ? '用「下一决策」前往可提问的局面。'
                        : '回答会区分计算、Mortal 与推测。'}
                  </small>
                </div>
              )}
              {messages.map((message, i) => (
                <div className={`message ${message.role}`} key={i}>
                  <span className="message-author">{message.role === 'user' ? '你' : 'KYOKU'}</span>
                  <ReactMarkdown
                    components={{
                      a: ({ children }) => <span>{children}</span>,
                      img: ({ alt }) => <span>{alt}</span>,
                    }}
                  >
                    {message.text}
                  </ReactMarkdown>
                </div>
              ))}
              {asking && (
                <div className="thinking">
                  <span className="status-dot" />
                  {answerContext === conversation.current
                    ? '正在生成回答…'
                    : '上一局面的请求仍在结束，可以继续浏览。'}
                </div>
              )}
              {chatError && (
                <div role="alert" className="chat-error">
                  {chatError}
                </div>
              )}
              <div ref={chatEnd} />
            </div>
            <form
              className="composer"
              onSubmit={(e) => {
                e.preventDefault();
                void ask(question);
              }}
            >
              <textarea
                aria-label="复盘问题"
                placeholder={decision ? '问问这个局面…' : '选择一个决策点后提问…'}
                value={question}
                disabled={!decision || asking}
                onChange={(e) => setQuestion(e.target.value)}
                rows={3}
                onFocus={() => setPlaying(false)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing) {
                    e.preventDefault();
                    void ask(question);
                  }
                }}
              />
              <div>
                <small>Enter 发送 · Shift Enter 换行</small>
                <button
                  aria-label="发送问题"
                  type="submit"
                  disabled={!decision || asking || !question.trim()}
                >
                  ↑
                </button>
              </div>
            </form>
            <p className="chat-footnote">切换局面会清空问答。模型解释可通过原始证据核对。</p>
          </aside>
        </main>
      )}
    </div>
  );
}
