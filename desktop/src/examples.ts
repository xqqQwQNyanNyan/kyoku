// 构建时内置仓库样本，安装包也能离线导入，无需依赖源码目录。
const logs = import.meta.glob<string>('../../fixtures/tenhou/*.json', {
  query: '?raw',
  import: 'default',
  eager: true,
});

const titles: Record<string, string> = {
  ranked_game: '对局回放',
  chankan: '抢杠和牌',
  complex_nakis: '复杂鸣牌',
  double_kakan_then_chankan: '连续加杠与抢杠',
  double_ron: '双响',
  four_reach: '四家立直',
  kyushukyuhai: '九种九牌',
  rinshan: '岭上摸牌',
  ryukyoku: '流局',
};

function title(name: string) {
  if (titles[name]) return titles[name];
  const match = /^(complex_nakis|confusing_nakis|suukantsu)_(\d+)$/.exec(name);
  if (!match) return name;
  const group = { complex_nakis: '复杂鸣牌', confusing_nakis: '鸣牌辨析', suukantsu: '连续杠' };
  return `${group[match[1] as keyof typeof group]} · ${Number(match[2]) + 1}`;
}

export const examples = Object.entries(logs)
  .map(([path, json]) => {
    const filename = path.slice(path.lastIndexOf('/') + 1);
    return { filename, title: title(filename.replace(/\.json$/, '')), json };
  })
  .sort((a, b) => {
    if (a.filename === 'ranked_game.json') return -1;
    if (b.filename === 'ranked_game.json') return 1;
    return a.filename.localeCompare(b.filename);
  });
