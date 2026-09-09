import { useEffect, useState } from 'react';
import type { QuestionProgress } from './types';

const toolLabels: Record<string, string> = {
  get_review: '读取局面证据',
  compare_discards: '比较候选切牌',
  compare_improvements: '比较后续改良',
  compare_discard_safety: '比较切牌防守',
  analyze_discard_followup: '核验指定摸切',
  analyze_waits: '分析待牌与打点',
  analyze_action_details: '展开指定动作',
  analyze_yaku_progression: '分析役种推进牌',
  analyze_hand: '分析手牌与进张',
  analyze_yaku_route: '分析役种路线',
  analyze_defense: '检查防守依据',
  analyze_actions: '分析吃碰杠与立直',
  analyze_score_targets: '计算点差目标',
  analyze_draw_outcomes: '计算流局结果',
  analyze_win_outcome: '计算和牌结算',
};

export function QuestionStatus({
  progress,
}: {
  progress?: {
    stage: QuestionProgress;
    startedAt: number;
    stopping: boolean;
    usage?: Extract<QuestionProgress, { phase: 'usage' }>;
  };
}) {
  const [now, setNow] = useState(Date.now);
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, []);
  const stage = progress?.stage;
  const label = progress?.stopping
    ? '正在停止…'
    : stage?.phase === 'verifying'
      ? `正在核查答案 · 第 ${stage.request} 次请求`
      : stage?.phase === 'model'
        ? `正在等待模型 · 第 ${stage.request} 次请求`
        : stage?.phase === 'tool'
          ? `${toolLabels[stage.name] ?? '正在执行分析工具'}…`
          : '正在准备局面…';
  const seconds = progress ? Math.max(0, Math.floor((now - progress.startedAt) / 1000)) : 0;
  const elapsed =
    seconds < 60 ? `${seconds} 秒` : `${Math.floor(seconds / 60)} 分 ${seconds % 60} 秒`;
  return (
    <div className="thinking">
      <div className="question-stage" role="status">
        <span className="status-dot" />
        {label}
      </div>
      <small className="question-elapsed" aria-live="off">
        已用 {elapsed}
      </small>
      {progress?.stopping && <p>正在结束本轮问答；问题和执行记录会保留。</p>}
    </div>
  );
}
