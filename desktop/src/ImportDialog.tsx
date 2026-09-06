import { useEffect, useRef, useState } from 'react';
import { examples } from './examples';
import { Select } from './Select';
import exampleLicense from '../../fixtures/tenhou/LICENSE?url';

export function ImportDialog({
  busy,
  error,
  onFile,
  onLink,
  onExample,
  onClose,
}: {
  busy: boolean;
  error: string;
  onFile: () => void;
  onLink: (link: string) => void;
  onExample: (name: string, json: string) => void;
  onClose: () => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [source, setSource] = useState<'file' | 'link' | 'example' | null>(null);
  const [link, setLink] = useState('');
  const [exampleName, setExampleName] = useState(examples[0]?.filename ?? '');
  const example = examples.find((item) => item.filename === exampleName);

  useEffect(() => {
    dialog.current?.showModal();
  }, []);

  return (
    <dialog
      ref={dialog}
      className="import-dialog"
      aria-labelledby="import-title"
      onCancel={(event) => {
        event.preventDefault();
        if (!busy) onClose();
      }}
    >
      <div className="import-heading">
        <div>
          <h2 id="import-title">导入牌谱</h2>
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
          <strong>天凤链接</strong>
          <span>粘贴链接或 log ID</span>
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
            if (!busy && link.trim()) onLink(link.trim());
          }}
        >
          <label htmlFor="log-link">天凤牌谱链接</label>
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
          <div className="import-actions">
            <small>支持四人牌谱链接或 log ID，需要联网。</small>
            <button className="primary" type="submit" disabled={busy || !link.trim()}>
              {busy ? '正在导入…' : '导入链接'}
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
              description: item.filename,
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
    </dialog>
  );
}
