import { useEffect, useId, useRef, useState } from 'react';
import { errorMessage } from './bridge';

export function DeleteDialog({
  title,
  name,
  description,
  onDelete,
  onClose,
}: {
  title: string;
  name: string;
  description: string;
  onDelete: () => Promise<void>;
  onClose: () => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const titleId = useId();
  const [deleting, setDeleting] = useState(false);
  const [error, setError] = useState('');
  useEffect(() => {
    dialog.current?.showModal();
  }, []);

  async function remove() {
    if (deleting) return;
    setDeleting(true);
    setError('');
    try {
      await onDelete();
      onClose();
    } catch (error) {
      setError(errorMessage(error));
    } finally {
      setDeleting(false);
    }
  }

  return (
    <dialog
      ref={dialog}
      className="delete-dialog"
      aria-labelledby={titleId}
      onCancel={(event) => {
        event.preventDefault();
        if (!deleting) onClose();
      }}
    >
      <h2 id={titleId}>{title}</h2>
      <p className="delete-name">{name}</p>
      <p>{description}</p>
      <p>删除后无法恢复。</p>
      {error && (
        <p className="import-error" role="alert">
          {error}
        </p>
      )}
      <div className="delete-actions">
        <button autoFocus disabled={deleting} onClick={onClose}>
          取消
        </button>
        <button className="danger-button" disabled={deleting} onClick={() => void remove()}>
          {deleting ? '正在删除…' : '确认删除'}
        </button>
      </div>
    </dialog>
  );
}
