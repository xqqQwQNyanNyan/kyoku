import { useEffect, useRef, useState } from 'react';
import { errorMessage } from './bridge';
import type { Bridge, MigrationProgress, StorageLocation } from './types';

export function StorageSettings({
  api,
  onBusyChange,
}: {
  api: Bridge;
  onBusyChange: (busy: boolean) => void;
}) {
  const [location, setLocation] = useState<StorageLocation | null>(null);
  const [destination, setDestination] = useState('');
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<MigrationProgress | null>(null);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const [cancelling, setCancelling] = useState(false);
  const request = useRef<string | null>(null);
  const working = useRef(false);

  useEffect(() => {
    let active = true;
    void api
      .getStorage()
      .then((value) => {
        if (active) setLocation(value);
      })
      .catch((e: unknown) => {
        if (active) setError(errorMessage(e));
      });
    return () => {
      active = false;
    };
  }, [api]);

  function running(value: boolean) {
    working.current = value;
    setBusy(value);
    onBusyChange(value);
  }

  async function choose() {
    if (working.current) return;
    running(true);
    setError('');
    try {
      const path = await api.chooseDataDirectory();
      if (path) {
        setDestination(path);
        setNotice('');
      }
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      running(false);
    }
  }

  async function migrate() {
    if (working.current || !destination) return;
    running(true);
    setError('');
    setNotice('');
    setProgress(null);
    setCancelling(false);
    const id = crypto.randomUUID();
    request.current = id;
    try {
      const result = await api.migrateData(destination, id, (value) => {
        if (request.current === id) setProgress(value);
      });
      setLocation(result);
      setDestination('');
      setNotice('保存位置已切换，后续牌谱和对话会写入新目录。原目录副本已保留。');
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      request.current = null;
      setProgress(null);
      running(false);
    }
  }

  async function cancel() {
    if (!request.current) return;
    setCancelling(true);
    try {
      await api.cancelDataMigration(request.current);
    } catch (e) {
      setError(errorMessage(e));
      setCancelling(false);
    }
  }

  async function open() {
    try {
      await api.openDataDirectory();
    } catch (e) {
      setError(errorMessage(e));
    }
  }

  return (
    <div className="storage-settings">
      <p className="settings-description">牌谱和对话一起保存，切换位置后无需重启。</p>
      <label htmlFor="data-location">当前保存位置</label>
      <input id="data-location" readOnly value={location?.directory ?? '正在读取…'} />
      {location && !location.available && (
        <p role="alert" className="settings-error">
          数据目录不可用。请重新连接存储设备或恢复原目录，再重新打开应用；不会自动改用空目录。
        </p>
      )}
      <div className="storage-actions">
        <button type="button" disabled={busy || !location?.available} onClick={() => void open()}>
          打开数据文件夹
        </button>
        <button type="button" disabled={busy || !location?.available} onClick={() => void choose()}>
          选择新位置…
        </button>
      </div>
      {destination && (
        <div className="storage-destination">
          <label htmlFor="data-destination">新的保存位置</label>
          <input id="data-destination" readOnly value={destination} />
          <small>
            将复制 replays 和 sessions。目标位置不能已有这两个文件夹，原目录会保留一份副本。
          </small>
          <button type="button" className="primary" disabled={busy} onClick={() => void migrate()}>
            迁移并使用此位置
          </button>
        </div>
      )}
      {busy && request.current && (
        <div role="status" className="storage-progress">
          <p>
            {cancelling
              ? '正在取消…'
              : progress?.total_files
                ? `已复制 ${progress.copied_files} / ${progress.total_files} 个文件`
                : '正在检查数据目录…'}
          </p>
          {progress && progress.total_bytes > 0 && (
            <progress
              aria-label="数据迁移进度"
              value={progress.copied_bytes}
              max={progress.total_bytes}
            />
          )}
          <button type="button" disabled={!progress || cancelling} onClick={() => void cancel()}>
            取消迁移
          </button>
        </div>
      )}
      <small className="storage-help">
        请先等待问答、导入和保存结束。迁移失败或取消时继续使用原目录。API
        设置和内置模型的位置不随之改变。
      </small>
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
    </div>
  );
}
