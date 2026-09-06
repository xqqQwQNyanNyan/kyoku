import type { Frame, Meld } from './types';
import { Tile, tileName } from './Tile';

const eventNames: Record<string, string> = {
  tsumo: '摸牌',
  dahai: '打出',
  chi: '吃',
  pon: '碰',
  daiminkan: '大明杠',
  ankan: '暗杠',
  kakan: '加杠',
  reach: '宣告立直',
  reach_accepted: '立直成立',
  hora: '和牌',
  dora: '翻开宝牌指示牌',
  ryukyoku: '流局',
  start_kyoku: '开局',
  end_kyoku: '本局结束',
  end_game: '牌谱结束',
  none: '等待',
};

export function eventText(frame: Frame, names: string[], player: number, reveal: boolean): string {
  const { kind, actor, tile, target } = frame.event;
  const name = actor === null ? '' : `${names[actor]} · `;
  const action =
    kind === 'hora'
      ? actor === target
        ? '自摸'
        : `荣和（${names[target ?? 0]}）`
      : (eventNames[kind] ?? kind);
  const visible = kind !== 'tsumo' || actor === player || reveal;
  return `${name}${action}${tile && visible ? ` ${tileName(tile)}` : ''}`;
}

function MeldTiles({ meld, player }: { meld: Meld; player: number }) {
  const tiles = [...meld.tiles];
  let calledIndex = -1;
  if (meld.called !== null && meld.from !== null) {
    const original = tiles.indexOf(meld.called);
    if (original >= 0) {
      tiles.splice(original, 1);
      const relative = (meld.from - player + 4) % 4;
      calledIndex = relative === 3 ? 0 : relative === 2 ? 1 : tiles.length;
      tiles.splice(calledIndex, 0, meld.called);
    }
  }
  return (
    <span
      className="meld"
      title={`${eventNames[meld.kind]}${meld.from === null ? '' : ` · 来自玩家 ${meld.from + 1}`}`}
    >
      {tiles.map((tile, i) => (
        <Tile
          key={i}
          tile={meld.kind === 'ankan' && (i === 0 || i === 3) ? '?' : tile}
          small
          className={i === calledIndex ? 'sideways' : ''}
        />
      ))}
      {meld.kind === 'kakan' && <small className="meld-kind">加杠</small>}
    </span>
  );
}

export function Board({
  frame,
  names,
  player,
  reveal,
  selected,
  onSelect,
}: {
  frame: Frame;
  names: string[];
  player: number;
  reveal: boolean;
  selected: string | null;
  onSelect: (tile: string) => void;
}) {
  return (
    <div className="board" aria-label="当前牌桌">
      <div className="board-grid" />
      <div className="table-center">
        <span className="eyebrow">KYOKU</span>
        <strong>{frame.round}</strong>
        <span>
          {frame.honba} 本场 <i>·</i> 供托 {frame.riichi_sticks}
        </span>
        <div className="draw-counter">
          {frame.settled ? (
            '结算'
          ) : (
            <>
              <b>{frame.remaining_draws}</b> 次摸牌
            </>
          )}
        </div>
        <div className="dora" aria-label="宝牌指示牌">
          {frame.dora_indicators.map((tile, i) => (
            <Tile key={i} tile={tile} small />
          ))}
        </div>
        <small>宝牌指示牌</small>
      </div>
      {[0, 1, 2, 3].map((relative) => {
        const index = (player + relative) % 4;
        const seat = frame.players[index];
        const visible = relative === 0 || reveal;
        const hand = [...seat.concealed];
        const drawn = frame.drawn?.[0] === index ? frame.drawn[1] : null;
        if (drawn) {
          const at = hand.indexOf(drawn);
          if (at >= 0) {
            hand.splice(at, 1);
            hand.push(drawn);
          }
        }
        return (
          <div
            key={index}
            className={`seat seat-${relative} ${frame.active_player === index && !frame.settled ? 'active' : ''}`}
          >
            <div className="seat-label">
              <span className={`wind ${frame.dealer === index ? 'dealer' : ''}`}>
                {['東', '南', '西', '北'][(index - frame.dealer + 4) % 4]}
              </span>
              <span className="player-name">{names[index]}</span>
              <b>{seat.score.toLocaleString()}</b>
              {seat.riichi && <span className="riichi-tag">立直</span>}
            </div>
            <div className="hand" aria-label={`${names[index]}的手牌`}>
              {hand.map((tile, i) => (
                <Tile
                  key={i}
                  tile={visible ? tile : '?'}
                  small={relative !== 0}
                  className={drawn && i === hand.length - 1 ? 'drawn' : ''}
                  selected={visible && selected === tile}
                  onClick={relative === 0 ? () => onSelect(tile) : undefined}
                />
              ))}
            </div>
            <div className="melds">
              {seat.melds.map((meld, i) => (
                <MeldTiles key={i} meld={meld} player={index} />
              ))}
            </div>
            <div className="river" aria-label={`${names[index]}的牌河`}>
              {seat.discards.map((d, i) => (
                <span
                  key={i}
                  className={`discard ${d.called ? 'called' : ''}`}
                  title={`${tileName(d.tile)}${d.tsumogiri ? ' · 摸切' : ' · 手切'}${d.called ? ' · 已被鸣走' : ''}${d.riichi ? ' · 立直宣言牌' : ''}`}
                >
                  <Tile
                    tile={d.tile}
                    small
                    className={`${d.riichi ? 'sideways' : ''} ${d.tsumogiri ? 'tsumogiri' : ''}`}
                    selected={selected === d.tile}
                  />
                </span>
              ))}
            </div>
          </div>
        );
      })}
    </div>
  );
}
