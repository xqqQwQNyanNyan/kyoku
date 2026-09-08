import { useEffect, useRef } from 'react';
import type { RoundResult } from './types';

const drawReasons: Record<string, string> = {
  流局: '荒牌流局（牌山耗尽）',
  九種九牌: '九种九牌',
  四風連打: '四风连打',
  四家立直: '四家立直',
  四開槓: '四杠散了',
  三家和: '三家和了',
  流し満貫: '流局满贯',
};

function scoreText(text: string) {
  return text.replaceAll('飜', '番').replaceAll('翻', '番').replaceAll('満', '满');
}

export function RoundResultDialog({
  label,
  result,
  names,
  nextLabel,
  onClose,
}: {
  label: string;
  result: RoundResult;
  names: string[];
  nextLabel?: string;
  onClose: () => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    dialog.current?.showModal();
  }, []);
  return (
    <dialog
      ref={dialog}
      className="round-result-dialog"
      aria-labelledby="round-result-title"
      onCancel={(event) => {
        event.preventDefault();
        onClose();
      }}
    >
      <div className="settings-heading">
        <div>
          <span className="eyebrow">{label}</span>
          <h2 id="round-result-title">{result.wins.length ? '和牌结算' : '流局结算'}</h2>
        </div>
        <button aria-label="关闭结算" onClick={onClose}>
          ×
        </button>
      </div>
      <div className="round-result-content">
        {result.wins.length ? (
          result.wins.map(([actor, target], i) => {
            const detail = result.details?.kind === 'hora' ? result.details.wins[i] : undefined;
            return (
              <section className="round-win" key={actor}>
                <h3>
                  {names[actor]} · {actor === target ? '自摸' : `荣和（${names[target]} 放铳）`}
                </h3>
                {detail ? (
                  <>
                    {detail.score && <p className="round-score">{scoreText(detail.score)}</p>}
                    <table aria-label={`${names[actor]}的役种`}>
                      <thead>
                        <tr>
                          <th>役种</th>
                          <th>番数</th>
                        </tr>
                      </thead>
                      <tbody>
                        {detail.yaku.map((yaku, j) => {
                          const parts = scoreText(yaku).match(/^(.*)[(（](.*)[)）]$/);
                          return (
                            <tr key={j}>
                              <td>{parts ? parts[1] : yaku}</td>
                              <td>{parts ? parts[2] : '未提供'}</td>
                            </tr>
                          );
                        })}
                      </tbody>
                    </table>
                    {!detail.yaku.length && <p className="muted">原始牌谱未提供役种明细。</p>}
                  </>
                ) : (
                  <p className="muted">此旧牌谱未保存役种和番数，重新导入原始牌谱后可查看。</p>
                )}
              </section>
            );
          })
        ) : (
          <p className="round-draw-reason">
            {result.details?.kind === 'ryukyoku'
              ? (drawReasons[result.details.reason] ?? result.details.reason)
              : '此旧牌谱未保存流局原因，重新导入原始牌谱后可查看。'}
          </p>
        )}
        <h3>点数变化</h3>
        <table aria-label="点数变化">
          <thead>
            <tr>
              <th>玩家</th>
              <th>结算前</th>
              <th>Delta</th>
              <th>结算后</th>
            </tr>
          </thead>
          <tbody>
            {names.map((name, i) => (
              <tr key={i}>
                <td>{name}</td>
                <td>{result.scores[i] - result.deltas[i]}</td>
                <td
                  className={
                    result.deltas[i] > 0 ? 'score-gain' : result.deltas[i] < 0 ? 'score-loss' : ''
                  }
                >
                  {result.deltas[i] > 0 ? '+' : ''}
                  {result.deltas[i]}
                </td>
                <td>{result.scores[i]}</td>
              </tr>
            ))}
          </tbody>
        </table>
        <p className="muted">Delta 为本次结算点差；本局已支付的立直棒已计入结算前点数。</p>
      </div>
      <div className="round-result-footer">
        <span className="muted">{nextLabel ? `下一局：${nextLabel}` : '全场回放结束'}</span>
        <button autoFocus onClick={onClose}>
          {nextLabel ? '关闭并进入下一局' : '关闭'}
        </button>
      </div>
    </dialog>
  );
}
