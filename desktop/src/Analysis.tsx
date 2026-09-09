import type { Action, Decision } from './types';
import { Tile, tileName } from './Tile';

const actions: Record<string, string> = {
  discard: '切',
  riichi: '立直',
  reach: '立直',
  chi_low: '吃（低位）',
  chi_middle: '吃（中位）',
  chi_high: '吃（高位）',
  chi: '吃',
  pon: '碰',
  kan: '杠',
  daiminkan: '大明杠',
  ankan: '暗杠',
  kakan: '加杠',
  win: '和牌',
  hora: '和牌',
  abortive_draw: '九种九牌',
  ryukyoku: '流局',
  pass: '跳过',
  none: '跳过',
  dahai: '切',
};
export function actionText(action: Action) {
  return `${actions[action.type] ?? action.type}${action.pai ? ` ${tileName(action.pai)}` : ''}`;
}

export function differsFromRecommendation(decision: Decision) {
  const recommended = decision.evidence.mortal.decision?.recommended;
  if (!recommended || decision.actual.kind === 'unresolved') return false;
  if (decision.actual.kind === 'passed') return recommended.type !== 'none';
  const actual = decision.actual.action;
  if (!actual) return false;
  // 比较动作与用牌，保留赤牌差异；同张牌的摸切、手切不算推荐分歧。
  return (
    actual.type !== recommended.type ||
    actual.pai !== recommended.pai ||
    actual.target !== recommended.target ||
    [...(actual.consumed ?? [])].sort().join(',') !==
      [...(recommended.consumed ?? [])].sort().join(',')
  );
}

export function Analysis({
  decision,
  ready,
  busy,
  supported,
  selected,
  onSelect,
  onAnalyze,
}: {
  decision: Decision | undefined;
  ready: boolean;
  busy: boolean;
  supported: boolean;
  selected: string | null;
  onSelect: (tile: string) => void;
  onAnalyze: () => void;
}) {
  const engine = decision?.evidence.mortal.decision;
  const efficiency = decision?.evidence.discards.find((d) => d.discard === selected);
  return (
    <section className="analysis-panel" aria-label="局面分析">
      <div className="panel-heading">
        <h2>决策分析</h2>
        <span className="muted">
          {decision ? `第 ${decision.turn} 手 · G${decision.event_index}` : 'Mortal × 牌效率'}
        </span>
      </div>
      {!engine ? (
        <div className="analysis-empty">
          <span className="analysis-symbol">◇</span>
          <div>
            <strong>
              {busy
                ? '正在分析整场牌谱…'
                : ready
                  ? '当前事件没有所选玩家的决策'
                  : supported
                    ? '让每一步都有依据'
                    : '当前牌谱仅支持回放'}
            </strong>
            <p>
              {busy
                ? '模型只加载一次，你可以继续浏览牌局。'
                : ready
                  ? '继续逐张播放，或使用「下一决策」跳转。'
                  : supported
                    ? '回放已就绪。分析所选玩家，查看推荐动作与切牌效率。'
                    : 'Mortal 分析目前仅支持四人半庄。'}
            </p>
          </div>
          {!ready && !busy && (
            <button className="primary" onClick={onAnalyze} disabled={!supported}>
              分析此玩家
            </button>
          )}
        </div>
      ) : (
        <>
          <div className="recommendation">
            <span className="recommend-label">Mortal 推荐</span>
            <strong>{actionText(engine.recommended)}</strong>
            <span className="actual">
              实际：
              {decision.actual.kind === 'passed'
                ? '跳过'
                : decision.actual.action
                  ? actionText(decision.actual.action)
                  : '无法确定'}
            </span>
          </div>
          <div className="candidate-scroll">
            <table>
              <thead>
                <tr>
                  <th>候选动作</th>
                  <th>Q 值</th>
                  <th>向听</th>
                  <th>未见进张枚数</th>
                </tr>
              </thead>
              <tbody>
                {[...engine.candidates]
                  .sort((a, b) => b.q_value - a.q_value)
                  .map((candidate, i) => {
                    const tile = candidate.action.tile;
                    const stats = decision.evidence.discards.find((d) => d.discard === tile);
                    return (
                      <tr key={i} className={tile && selected === tile ? 'chosen' : ''}>
                        <td>
                          {tile ? (
                            <button className="tile-action" onClick={() => onSelect(tile)}>
                              <Tile tile={tile} small />
                              <span>切 {tileName(tile)}</span>
                            </button>
                          ) : (
                            (actions[candidate.action.kind] ?? candidate.action.kind)
                          )}
                        </td>
                        <td className="numeric">{candidate.q_value.toFixed(3)}</td>
                        <td>
                          {stats
                            ? stats.shanten === 0
                              ? '听牌'
                              : stats.shanten < 0
                                ? '完成牌形'
                                : `${stats.shanten} 向听`
                            : '—'}
                        </td>
                        <td>
                          {stats ? `${stats.total_unseen} 枚 / ${stats.draws.length} 种` : '—'}
                        </td>
                      </tr>
                    );
                  })}
              </tbody>
            </table>
            {engine.kan_candidates.length > 1 && (
              <p className="detail-line">
                杠牌种独立 Q：
                {engine.kan_candidates
                  .map((c) => `${tileName(c.tile)} ${c.q_value.toFixed(3)}`)
                  .join(' · ')}
              </p>
            )}
            {efficiency && (
              <div className="effective-tiles">
                <span>{efficiency.draw_kind === 'winning_shape' ? '完成牌形' : '有效牌'}</span>
                {efficiency.draws.map((draw) => (
                  <span key={draw.tile}>
                    <Tile tile={draw.tile} small />
                    <small>×{draw.unseen}</small>
                  </span>
                ))}
              </div>
            )}
          </div>
        </>
      )}
    </section>
  );
}
