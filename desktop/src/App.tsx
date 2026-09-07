import { useEffect, useRef, useState, type CSSProperties } from 'react';
import type { Bridge, Decision, Replay, SessionPosition, SessionView } from './types';
import { bridge, errorMessage } from './bridge';
import { Board, eventText } from './Board';
import { Analysis } from './Analysis';
import { Tile } from './Tile';
import { SettingsPanel } from './Settings';
import { useWindowScale } from './useWindowScale';
import { ImportDialog } from './ImportDialog';
import { Select } from './Select';
import { replayName } from './display';
import { ChatPanel, HistoryDialog, useSessions } from './Sessions';
import { ReplayLibrary } from './ReplayLibrary';
import { RenameReplayDialog } from './RenameReplayDialog';

const roundsPerPage = 7;
const reviewTabs = [
  { id: 'analysis', label: '决策分析' },
  { id: 'chat', label: '一起复盘' },
] as const;

export default function App({ api = bridge }: { api?: Bridge }) {
  const scale = useWindowScale();
  const [reviewTab, setReviewTab] = useState<'analysis' | 'chat'>('analysis');
  const [roundPage, setRoundPage] = useState(0);
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
  const [showImport, setShowImport] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [error, setError] = useState('');
  const workspace = useSessions(api);
  const [showHistory, setShowHistory] = useState(false);
  const [showLibrary, setShowLibrary] = useState(false);
  const [showRename, setShowRename] = useState(false);
  const asking = Object.keys(workspace.pending).length > 0;
  const fileInput = useRef<HTMLInputElement>(null);
  const documentId = useRef<number | null>(null);
  const analysisJob = useRef(0);
  const importBusy = useRef(false);
  const frame = replay?.frames[index];
  const points = decisions[player];
  const decision = frame ? points?.find((d) => d.event_index === frame.event_index) : undefined;
  const activeRound = replay?.rounds.findLastIndex((round) => round.frame_index <= index) ?? -1;

  useEffect(() => {
    setRoundPage(Math.floor(Math.max(0, activeRound) / roundsPerPage));
  }, [activeRound, replay]);

  function selectFrame(next: number) {
    if (!replay) return;
    next = Math.max(0, Math.min(replay.frames.length - 1, next));
    if (next !== index) {
      setSelected(null);
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
    setSelected(null);
    setPlayer(next);
  }

  async function importReplay(name: string, read: () => Promise<Replay>) {
    if (importBusy.current) return;
    importBusy.current = true;
    setLoading(true);
    setPlaying(false);
    setError('');
    try {
      const loaded = await read();
      const attached = await workspace.attachGame(loaded.game_key);
      documentId.current = loaded.id;
      analysisJob.current += 1;
      setSelected(null);
      setReplay(loaded);
      setFilename(loaded.name ?? name);
      setIndex(
        Math.max(
          0,
          loaded.frames.findIndex((f) => f.event_index === attached?.position?.event_index),
        ),
      );
      setPlayer(attached?.position?.player ?? 0);
      setReveal(false);
      setDecisions({});
      setAnalyzing(null);
      setShowImport(false);
      setShowLibrary(false);
    } catch (error) {
      setError(errorMessage(error));
    } finally {
      setLoading(false);
      importBusy.current = false;
    }
  }

  async function importFile(file: File | undefined) {
    if (!file || importBusy.current) return;
    if (file.size > 32 * 1024 * 1024) {
      setError('牌谱文件不能超过 32 MiB；天凤 JSON 上限为 16 MiB');
      return;
    }
    await importReplay(file.name, async () => api.importLog(await file.text(), file.name));
  }

  function openImport() {
    setPlaying(false);
    setError('');
    setShowImport(true);
  }

  async function analyze() {
    if (!replay || !replay.mortal_supported || analyzing !== null) return;
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
      if (showSettings || showImport || showHistory || showLibrary || showRename) return;
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

  const scope = replay?.game_key ?? '';
  const chatId = workspace.idFor(scope);
  const chatSource =
    replay && frame
      ? {
          game: replay.id,
          gameKey: replay.game_key,
          gameLabel: filename,
          player,
          event: frame.event_index,
          label: `${replay.names[player]} · ${frame.round} · G${frame.event_index}`,
        }
      : undefined;

  function locate(position: SessionPosition) {
    if (!replay) return;
    const frameIndex = replay.frames.findIndex((f) => f.event_index === position.event_index);
    if (frameIndex < 0) return;
    changePlayer(position.player);
    jump(frameIndex);
  }

  async function openSessionGame(doc: SessionView, position?: SessionPosition) {
    if (importBusy.current) return;
    importBusy.current = true;
    setLoading(true);
    setPlaying(false);
    try {
      const opened = await api.openSessionGame(doc.id);
      const loaded = opened.replay;
      await workspace.attachGame(loaded.game_key);
      if (documentId.current !== loaded.id) {
        analysisJob.current += 1;
        setDecisions({});
        setAnalyzing(null);
      }
      documentId.current = loaded.id;
      workspace.activate(loaded.game_key, doc.id);
      setReplay(loaded);
      setFilename(opened.name);
      setSelected(null);
      setReveal(false);
      const cursor = position ?? opened.position;
      setIndex(
        Math.max(
          0,
          loaded.frames.findIndex((f) => f.event_index === cursor.event_index),
        ),
      );
      setPlayer(cursor.player);
      setReviewTab('chat');
      setShowHistory(false);
    } finally {
      importBusy.current = false;
      setLoading(false);
    }
  }

  const hasSavedGame = workspace.documents[chatId]?.game?.key === scope;
  const chatPending = !!workspace.pending[chatId];
  useEffect(() => {
    if (!chatSource || !hasSavedGame || chatPending || playing) return;
    const timer = window.setTimeout(() => void workspace.savePosition(chatId, chatSource), 400);
    return () => window.clearTimeout(timer);
  }, [chatId, scope, frame?.event_index, player, hasSavedGame, chatPending, playing]);

  const openFile = () => fileInput.current?.click();

  return (
    <div
      className="app"
      style={{ '--app-scale': scale } as CSSProperties}
      onDragOver={(event) => {
        event.preventDefault();
      }}
      onDrop={(event) => {
        event.preventDefault();
        if (!showSettings && !showHistory && !showLibrary && !showRename)
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
          <div>
            <strong>Kyoku</strong>
            <span>日麻牌谱复盘</span>
          </div>
        </div>
        <div className="document-heading">
          <div className="document-title" title={replay ? replayName(filename) : undefined}>
            {replay ? (
              <>
                <span className="status-dot" />
                {replayName(filename)}
              </>
            ) : (
              '从一份牌谱，重新看懂每一步。'
            )}
          </div>
          {replay && (
            <button
              className="document-rename"
              aria-label="重命名当前牌谱"
              title="重命名牌谱"
              disabled={loading}
              onClick={() => {
                setPlaying(false);
                setShowRename(true);
              }}
            >
              <svg aria-hidden="true" viewBox="0 0 24 24">
                <path d="m15 5 4 4M4 20l5-1L20 8a2.8 2.8 0 0 0-4-4L5 15l-1 5Z" />
              </svg>
            </button>
          )}
        </div>
        {replay && (
          <div className="perspective">
            <span>复盘玩家</span>
            <Select
              label="复盘玩家"
              value={String(player)}
              options={replay.names.map((name, i) => ({ value: String(i), label: name }))}
              onChange={(value) => changePlayer(Number(value))}
            />
          </div>
        )}
        <button className="import-button" disabled={loading} onClick={openImport}>
          {loading ? '正在读取…' : '＋ 导入牌谱'}
        </button>
        <button
          onClick={() => {
            setPlaying(false);
            setError('');
            setShowLibrary(true);
          }}
          disabled={loading}
        >
          牌谱库
        </button>
        <button
          onClick={() => {
            setPlaying(false);
            setShowHistory(true);
          }}
        >
          历史会话
        </button>
        <button
          onClick={() => {
            setPlaying(false);
            setShowSettings(true);
          }}
          disabled={asking}
        >
          设置
        </button>
      </header>
      {showHistory && (
        <HistoryDialog
          api={api}
          workspace={workspace}
          onClose={() => setShowHistory(false)}
          onOpenGame={openSessionGame}
        />
      )}
      {showSettings && <SettingsPanel api={api} onClose={() => setShowSettings(false)} />}
      {showRename && replay && (
        <RenameReplayDialog
          api={api}
          gameKey={replay.game_key}
          currentName={filename}
          onRenamed={(record) => {
            if (record.key === replay.game_key) setFilename(record.name);
          }}
          onClose={() => setShowRename(false)}
        />
      )}
      {showLibrary && (
        <ReplayLibrary
          api={api}
          busy={loading}
          error={error}
          onOpen={(record) => importReplay(record.name, () => api.openReplay(record.key))}
          onRenamed={(record) => {
            if (record.key === replay?.game_key) setFilename(record.name);
          }}
          onClose={() => setShowLibrary(false)}
        />
      )}
      {error && !showImport && !showLibrary && (
        <div role="alert" className="error-banner">
          <span>{error}</span>
          <button onClick={() => setError('')} aria-label="关闭错误提示">
            ×
          </button>
        </div>
      )}
      {showImport && (
        <ImportDialog
          api={api}
          onAccountBusyChange={(busy) => {
            importBusy.current = busy;
            setLoading(busy);
          }}
          busy={loading}
          error={error}
          onFile={openFile}
          onLink={(value) => importReplay(value, () => api.importLink(value))}
          onExample={(name, json) => void importReplay(name, () => api.importLog(json, name))}
          onClose={() => setShowImport(false)}
        />
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
          <button className="primary large" onClick={openImport} disabled={loading}>
            {loading ? '正在读取牌谱…' : '导入牌谱'} <span>↗</span>
          </button>
          <span className="welcome-hint">本地文件 · 天凤 / 雀魂链接 · 示例牌谱</span>
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
              {replay.rounds
                .slice(roundPage * roundsPerPage, (roundPage + 1) * roundsPerPage)
                .map((round, offset) => {
                  const i = roundPage * roundsPerPage + offset;
                  return (
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
                  );
                })}
            </div>
            {replay.rounds.length > roundsPerPage && (
              <div className="round-pages" aria-label="牌局列表翻页">
                <button
                  aria-label="上一页牌局"
                  disabled={roundPage === 0}
                  onClick={() => setRoundPage(roundPage - 1)}
                >
                  ‹
                </button>
                <span>
                  {roundPage + 1} / {Math.ceil(replay.rounds.length / roundsPerPage)}
                </span>
                <button
                  aria-label="下一页牌局"
                  disabled={(roundPage + 1) * roundsPerPage >= replay.rounds.length}
                  onClick={() => setRoundPage(roundPage + 1)}
                >
                  ›
                </button>
              </div>
            )}
            <div className="nav-footer">
              <span className="eyebrow">REVIEW</span>
              <strong>{replay.names[player]}</strong>
              <p>
                {!replay.mortal_supported
                  ? '东风场可回放，Mortal 分析目前仅支持半庄。'
                  : points
                    ? `${points.length} 个决策点已就绪`
                    : analyzing === player
                      ? '正在分析整场…'
                      : '分析后可按决策跳转'}
              </p>
              <button
                className="secondary"
                onClick={() => void analyze()}
                disabled={!replay.mortal_supported || analyzing !== null || !!points}
              >
                {points ? '分析已完成' : analyzing !== null ? '分析中…' : '分析此玩家'}
              </button>
            </div>
          </aside>
          <div className="replay-column">
            <div className="table-viewport">
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
                  <span className="table-legend">
                    <span>
                      <i className="legend-tsumogiri" />
                      摸切
                    </span>
                    <span>
                      <i className="legend-called" />
                      已被鸣走
                    </span>
                  </span>
                </div>
              </section>
            </div>
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
          </div>
          <aside className="review-sidebar" aria-label="复盘工具">
            <div className="review-tabs" role="tablist" aria-label="复盘工具切换">
              {reviewTabs.map((tab, i) => (
                <button
                  key={tab.id}
                  id={`review-tab-${tab.id}`}
                  type="button"
                  role="tab"
                  aria-selected={reviewTab === tab.id}
                  aria-controls={`review-panel-${tab.id}`}
                  tabIndex={reviewTab === tab.id ? 0 : -1}
                  onClick={() => setReviewTab(tab.id)}
                  onKeyDown={(event) => {
                    let next: number;
                    switch (event.key) {
                      case 'ArrowLeft':
                        next = (i + reviewTabs.length - 1) % reviewTabs.length;
                        break;
                      case 'ArrowRight':
                        next = (i + 1) % reviewTabs.length;
                        break;
                      case 'Home':
                        next = 0;
                        break;
                      case 'End':
                        next = reviewTabs.length - 1;
                        break;
                      default:
                        return;
                    }
                    event.preventDefault();
                    setReviewTab(reviewTabs[next].id);
                    document.getElementById(`review-tab-${reviewTabs[next].id}`)?.focus();
                  }}
                >
                  {tab.label}
                </button>
              ))}
            </div>
            <div
              className="review-tabpanel"
              role="tabpanel"
              id="review-panel-analysis"
              aria-labelledby="review-tab-analysis"
              hidden={reviewTab !== 'analysis'}
            >
              <Analysis
                decision={decision}
                ready={!!points}
                busy={analyzing !== null}
                selected={selected}
                onSelect={setSelected}
                onAnalyze={() => void analyze()}
                supported={replay.mortal_supported}
              />
            </div>
            <div
              className="review-tabpanel"
              role="tabpanel"
              id="review-panel-chat"
              aria-labelledby="review-tab-chat"
              hidden={reviewTab !== 'chat'}
            >
              <ChatPanel
                workspace={workspace}
                id={chatId}
                source={chatSource}
                ready={!!decision}
                visible={reviewTab === 'chat' && !showHistory}
                onFocus={() => setPlaying(false)}
                onNew={() => workspace.newSession(scope)}
                sessions={workspace.choices(scope)}
                onSelect={(id) => void workspace.selectSession(scope, id)}
                onLocate={locate}
              />
            </div>
          </aside>
        </main>
      )}
    </div>
  );
}
