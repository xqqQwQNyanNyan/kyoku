import { useEffect, useRef, useState } from 'react';
import type { Bridge, ReplayList, SavedReplay } from './types';
import { errorMessage } from './bridge';
import { replayName } from './display';
import { Select } from './Select';

const origins = { example: '示例牌谱', file: '本地导入', link: '链接下载', session: '来自会话' };
const MAX_NAME_CHARS = 80;

export function ReplayLibrary({
  api,
  busy,
  error,
  onOpen,
  onRenamed,
  onClose,
}: {
  api: Bridge;
  busy: boolean;
  error: string;
  onOpen: (replay: SavedReplay) => Promise<void>;
  onRenamed: (replay: SavedReplay) => void;
  onClose: () => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [list, setList] = useState<ReplayList | null>(null);
  const [loading, setLoading] = useState(true);
  const [failure, setFailure] = useState('');
  const [revision, setRevision] = useState(0);
  const [filter, setFilter] = useState('all');
  const [search, setSearch] = useState('');
  const [openingDirectory, setOpeningDirectory] = useState(false);
  const [editing, setEditing] = useState<SavedReplay | null>(null);
  const [name, setName] = useState('');
  const [saving, setSaving] = useState(false);
  useEffect(() => {
    dialog.current?.showModal();
  }, []);
  useEffect(() => {
    let active = true;
    setLoading(true);
    setFailure('');
    void api
      .listReplays()
      .then((result) => {
        if (active) setList(result);
      })
      .catch((error) => {
        if (active) setFailure(errorMessage(error));
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [api, revision]);
  const replays =
    list?.replays.filter(
      (replay) =>
        (filter === 'all' ||
          (filter === 'example' ? replay.origin === 'example' : replay.origin !== 'example')) &&
        replay.name.toLocaleLowerCase().includes(search.trim().toLocaleLowerCase()),
    ) ?? [];
  async function openDirectory() {
    setOpeningDirectory(true);
    setFailure('');
    try {
      await api.openDataDirectory();
    } catch (error) {
      setFailure(errorMessage(error));
    } finally {
      setOpeningDirectory(false);
    }
  }
  async function rename() {
    if (!editing || saving || busy || !name.trim()) return;
    setSaving(true);
    setFailure('');
    try {
      const renamed = await api.renameReplay(editing.key, name.trim());
      setList(
        (list) =>
          list && {
            ...list,
            replays: list.replays.map((item) => (item.key === renamed.key ? renamed : item)),
          },
      );
      onRenamed(renamed);
      setEditing(null);
    } catch (error) {
      setFailure(errorMessage(error));
    } finally {
      setSaving(false);
    }
  }
  return (
    <dialog
      ref={dialog}
      className="replay-library"
      aria-labelledby="replay-library-title"
      onCancel={(event) => {
        event.preventDefault();
        if (!busy && !saving) onClose();
      }}
    >
      <div className="library-heading">
        <div>
          <h2 id="replay-library-title">牌谱库</h2>
          <p>导入和下载的牌谱会自动保存，可离线打开。</p>
        </div>
        <button onClick={() => void openDirectory()} disabled={openingDirectory}>
          打开数据文件夹
        </button>
        <button aria-label="关闭牌谱库" onClick={onClose} disabled={busy || saving}>
          ×
        </button>
      </div>
      <div className="library-toolbar">
        <input
          aria-label="搜索牌谱"
          placeholder="搜索牌谱名称…"
          value={search}
          onChange={(event) => setSearch(event.target.value)}
        />
        <Select
          label="牌谱来源"
          value={filter}
          onChange={setFilter}
          options={[
            { value: 'all', label: '全部牌谱' },
            { value: 'imported', label: '我的牌谱' },
            { value: 'example', label: '示例牌谱' },
          ]}
        />
        <button
          disabled={loading || busy || !!editing}
          onClick={() => setRevision((value) => value + 1)}
        >
          刷新
        </button>
      </div>
      {editing && (
        <form
          className="library-name-editor"
          key={editing.key}
          onSubmit={(event) => {
            event.preventDefault();
            void rename();
          }}
          onKeyDown={(event) => {
            if (event.key === 'Escape') {
              event.preventDefault();
              event.stopPropagation();
              if (!saving) setEditing(null);
            }
            if (event.key === 'Enter' && event.nativeEvent.isComposing) event.preventDefault();
          }}
        >
          <label htmlFor="replay-name">牌谱名称</label>
          <input
            id="replay-name"
            autoFocus
            value={name}
            disabled={saving}
            onChange={(event) =>
              setName(Array.from(event.target.value).slice(0, MAX_NAME_CHARS).join(''))
            }
          />
          <small>
            {Array.from(name).length}/{MAX_NAME_CHARS}
          </small>
          <button type="submit" disabled={saving || busy || !name.trim()}>
            保存名称
          </button>
          <button type="button" disabled={saving} onClick={() => setEditing(null)}>
            取消
          </button>
        </form>
      )}
      {(error || failure) && (
        <p role="alert" className="import-error">
          {error || failure}
        </p>
      )}
      {!!list?.warnings.length && (
        <p role="alert" className="import-error">
          {list.warnings.join('；')}
        </p>
      )}
      <div className="library-records" aria-label="已保存牌谱">
        {loading ? (
          <p role="status">正在读取牌谱库…</p>
        ) : replays.length ? (
          replays.map((replay) => (
            <div className="library-entry" key={replay.key}>
              <button
                className="library-record"
                aria-label={`打开牌谱：${replayName(replay.name)}`}
                disabled={busy || saving}
                onClick={() => void onOpen(replay)}
              >
                <div>
                  <strong title={replayName(replay.name)}>{replayName(replay.name)}</strong>
                  <small>
                    {origins[replay.origin]} · {new Date(replay.saved_at).toLocaleDateString()}
                  </small>
                </div>
                <span aria-hidden="true">↗</span>
              </button>
              <button
                className="library-rename"
                aria-label={`重命名牌谱：${replayName(replay.name)}`}
                title="重命名牌谱"
                disabled={busy || saving}
                onClick={() => {
                  setEditing(replay);
                  setName(replayName(replay.name));
                  setFailure('');
                }}
              >
                <svg aria-hidden="true" viewBox="0 0 24 24">
                  <path d="m15 5 4 4M4 20l5-1L20 8a2.8 2.8 0 0 0-4-4L5 15l-1 5Z" />
                </svg>
              </button>
            </div>
          ))
        ) : (
          <p>
            {failure
              ? '牌谱库未能读取，请重试。'
              : '没有符合条件的牌谱。可以导入文件或下载链接，示例也可离线使用。'}
          </p>
        )}
      </div>
      <footer>
        <span>
          {list
            ? `${list.replays.length} 份牌谱 · 同一牌谱可有多个会话`
            : '牌谱与会话保存在同一数据目录'}
        </span>
        {list && <small title={list.directory}>{list.directory}</small>}
      </footer>
    </dialog>
  );
}
