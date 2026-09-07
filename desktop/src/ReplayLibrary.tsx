import { useEffect, useRef, useState } from 'react';
import type { Bridge, ReplayList, SavedReplay } from './types';
import { errorMessage } from './bridge';
import { replayName } from './display';
import { Select } from './Select';

const origins = { example: '示例牌谱', file: '本地导入', link: '链接下载', session: '来自会话' };

export function ReplayLibrary({
  api,
  busy,
  error,
  onOpen,
  onClose,
}: {
  api: Bridge;
  busy: boolean;
  error: string;
  onOpen: (replay: SavedReplay) => Promise<void>;
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
  return (
    <dialog
      ref={dialog}
      className="replay-library"
      aria-labelledby="replay-library-title"
      onCancel={(event) => {
        event.preventDefault();
        if (!busy) onClose();
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
        <button aria-label="关闭牌谱库" onClick={onClose} disabled={busy}>
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
        <button disabled={loading || busy} onClick={() => setRevision((value) => value + 1)}>
          刷新
        </button>
      </div>
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
            <button
              key={replay.key}
              className="library-record"
              disabled={busy}
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
