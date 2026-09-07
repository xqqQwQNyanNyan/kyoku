import { useEffect, useRef, useState } from 'react';
import type { Bridge, SavedReplay } from './types';
import { errorMessage } from './bridge';
import { replayName } from './display';

export function RenameReplayDialog({
  api,
  gameKey,
  currentName,
  onRenamed,
  onClose,
}: {
  api: Bridge;
  gameKey: string;
  currentName: string;
  onRenamed: (replay: SavedReplay) => void;
  onClose: () => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [name, setName] = useState(() => replayName(currentName));
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState('');
  useEffect(() => {
    dialog.current?.showModal();
  }, []);

  async function save() {
    if (saving || !name.trim()) return;
    setSaving(true);
    setError('');
    try {
      onRenamed(await api.renameReplay(gameKey, name.trim()));
      onClose();
    } catch (error) {
      setError(errorMessage(error));
    } finally {
      setSaving(false);
    }
  }

  return (
    <dialog
      ref={dialog}
      className="rename-replay-dialog"
      aria-labelledby="rename-replay-title"
      onCancel={(event) => {
        event.preventDefault();
        if (!saving) onClose();
      }}
    >
      <h2 id="rename-replay-title">重命名牌谱</h2>
      <form
        className="library-name-editor"
        onSubmit={(event) => {
          event.preventDefault();
          void save();
        }}
        onKeyDown={(event) => {
          if (event.key === 'Enter' && event.nativeEvent.isComposing) event.preventDefault();
        }}
      >
        <input
          aria-label="牌谱名称"
          autoFocus
          value={name}
          disabled={saving}
          onChange={(event) => setName(Array.from(event.target.value).slice(0, 80).join(''))}
        />
        <small>{Array.from(name).length}/80</small>
        <button type="submit" disabled={saving || !name.trim()}>
          保存名称
        </button>
        <button type="button" disabled={saving} onClick={onClose}>
          取消
        </button>
      </form>
      {error && (
        <p role="alert" className="import-error">
          {error}
        </p>
      )}
    </dialog>
  );
}
