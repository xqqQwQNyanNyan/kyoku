import { useEffect, useState } from 'react';
import type { AnalysisProgress } from './types';

export function AnalysisStatus({
  player,
  progress,
  startedAt,
  stopping,
  onCancel,
}: {
  player: string;
  progress: AnalysisProgress;
  startedAt: number;
  stopping: boolean;
  onCancel(): void;
}) {
  const [now, setNow] = useState(Date.now);
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, []);
  const label = stopping
    ? '正在取消分析…'
    : progress.phase === 'loading'
      ? '正在加载 Mortal 模型…'
      : progress.phase === 'analyzing'
        ? `已处理 ${progress.completed} / ${progress.total} 个事件`
        : progress.phase === 'finishing'
          ? '正在完成分析…'
          : '正在准备牌谱…';
  return (
    <div className="analysis-progress">
      <p role="status">{label}</p>
      {progress.phase === 'analyzing' && (
        <progress aria-label="Mortal 分析进度" value={progress.completed} max={progress.total} />
      )}
      <small>
        {player} · 已用 {Math.max(0, Math.floor((now - startedAt) / 1000))} 秒
      </small>
      <button className="secondary" onClick={onCancel} disabled={stopping}>
        {stopping ? '正在取消…' : '取消分析'}
      </button>
    </div>
  );
}
