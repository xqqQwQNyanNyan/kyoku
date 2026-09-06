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
  return `${tile.endsWith('r') ? '赤' : ''}${numerals[Number(tile[0])]}${{ m: '万', p: '筒', s: '索' }[tile[1]] ?? ''}`;
}

function Pips({ count, bamboo }: { count: number; bamboo: boolean }) {
  const positions: Record<number, [number, number][]> = {
    1: [[20, 27]],
    2: [
      [20, 17],
      [20, 37],
    ],
    3: [
      [11, 14],
      [20, 27],
      [29, 40],
    ],
    4: [
      [11, 17],
      [29, 17],
      [11, 37],
      [29, 37],
    ],
    5: [
      [11, 14],
      [29, 14],
      [20, 27],
      [11, 40],
      [29, 40],
    ],
    6: [
      [11, 12],
      [29, 12],
      [11, 27],
      [29, 27],
      [11, 42],
      [29, 42],
    ],
    7: [
      [20, 10],
      [11, 23],
      [29, 23],
      [11, 34],
      [29, 34],
      [11, 45],
      [29, 45],
    ],
    8: [
      [11, 10],
      [29, 10],
      [11, 21],
      [29, 21],
      [11, 33],
      [29, 33],
      [11, 45],
      [29, 45],
    ],
    9: [
      [10, 12],
      [20, 12],
      [30, 12],
      [10, 27],
      [20, 27],
      [30, 27],
      [10, 42],
      [20, 42],
      [30, 42],
    ],
  };
  return positions[count]?.map(([x, y], i) =>
    bamboo ? (
      <g key={i} stroke="currentColor" strokeWidth="2.8" strokeLinecap="round">
        <path d={`M${x},${y - 4}v8M${x - 2},${y}h4`} />
      </g>
    ) : (
      <g key={i}>
        <circle
          cx={x}
          cy={y}
          r={count === 1 ? 10 : 3.7}
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
        />
        <circle cx={x} cy={y} r="1.2" fill="currentColor" />
      </g>
    ),
  );
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
  const red = tile.endsWith('r') || tile === 'C';
  const color = red ? 'red' : tile[1] === 's' || tile === 'F' ? 'green' : 'ink';
  const contents =
    tile === '?' ? (
      <span className="tile-back-pattern" />
    ) : (
      <svg viewBox="0 0 40 56" aria-hidden="true">
        {honors[tile] ? (
          tile === 'P' ? (
            <rect
              x="9"
              y="10"
              width="22"
              height="36"
              rx="2"
              fill="none"
              stroke="#53758d"
              strokeWidth="2.8"
            />
          ) : (
            <text
              x="20"
              y="37"
              textAnchor="middle"
              fontSize="29"
              fill="currentColor"
              fontFamily="serif"
              fontWeight="700"
            >
              {honors[tile]}
            </text>
          )
        ) : tile[1] === 'm' ? (
          <>
            <text
              x="20"
              y="24"
              textAnchor="middle"
              fontSize="22"
              fill="currentColor"
              fontFamily="serif"
              fontWeight="700"
            >
              {numerals[Number(tile[0])]}
            </text>
            <text
              x="20"
              y="46"
              textAnchor="middle"
              fontSize="22"
              fill="#b43b34"
              fontFamily="serif"
              fontWeight="700"
            >
              萬
            </text>
          </>
        ) : (
          <Pips count={Number(tile[0])} bamboo={tile[1] === 's'} />
        )}
        {tile.endsWith('r') && <circle cx="35" cy="5" r="2" fill="currentColor" />}
      </svg>
    );
  const classes = `tile ${color} ${small ? 'small' : ''} ${tile === '?' ? 'back' : ''} ${selected ? 'selected' : ''} ${className}`;
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
