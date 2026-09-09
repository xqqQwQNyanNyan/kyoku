const honors: Record<string, string> = {
  E: '東',
  S: '南',
  W: '西',
  N: '北',
  P: '白',
  F: '發',
  C: '中',
};
const numerals = ['', '一', '二', '三', '四', '五', '六', '七', '八', '九'];
export function tileName(tile: string): string {
  if (tile === '?') return '暗牌';
  if (honors[tile]) return honors[tile];
  return `${tile.endsWith('r') ? '赤' : ''}${numerals[Number(tile[0])]}${{ m: '万', p: '饼', s: '索' }[tile[1]] ?? ''}`;
}

const faces = import.meta.glob<string>('./assets/tiles/*.svg', {
  eager: true,
  query: '?url',
  import: 'default',
});
const honorFaces: Record<string, string> = {
  E: 'Ton',
  S: 'Nan',
  W: 'Shaa',
  N: 'Pei',
  P: 'Haku',
  F: 'Hatsu',
  C: 'Chun',
};
const suits: Record<string, string> = { m: 'Man', p: 'Pin', s: 'Sou' };

function tileFace(tile: string): string {
  const name =
    honorFaces[tile] ?? `${suits[tile[1]]}${tile[0]}${tile.endsWith('r') ? '-Dora' : ''}`;
  return faces[`./assets/tiles/${name}.svg`];
}

export function Tile({
  tile,
  small = false,
  selected = false,
  onClick,
  className = '',
}: {
  tile: string;
  small?: boolean;
  selected?: boolean;
  onClick?: () => void;
  className?: string;
}) {
  const contents = (
    <span className="tile-face" aria-hidden="true">
      {tile !== '?' && <img src={tileFace(tile)} alt="" draggable={false} />}
    </span>
  );
  const classes = `tile ${small ? 'small' : ''} ${tile === '?' ? 'back' : ''} ${selected ? 'selected' : ''} ${className}`;
  return onClick ? (
    <button
      type="button"
      className={classes}
      title={tileName(tile)}
      aria-label={tileName(tile)}
      aria-pressed={selected}
      onClick={onClick}
    >
      {contents}
    </button>
  ) : (
    <span className={classes} title={tileName(tile)} role="img" aria-label={tileName(tile)}>
      {contents}
    </span>
  );
}
