// @vitest-environment jsdom
import { cleanup, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it, vi } from 'vitest';
import { Board } from './Board';
import type { Frame } from './types';

const frame: Frame = {
  event_index: 20,
  round: '东一局',
  honba: 0,
  dealer: 0,
  riichi_sticks: 1,
  remaining_draws: 40,
  dora_indicators: ['3p'],
  active_player: 0,
  settled: false,
  drawn: null,
  players: Array.from({ length: 4 }, () => ({
    score: 25000,
    riichi: false,
    concealed: ['1m', '2m', '3m', '4p', '5p', '6p', '7s', '8s', '9s', 'E'],
    melds: [{ kind: 'pon', tiles: ['C', 'C', 'C'], called: 'C', from: 3 }],
    discards: [
      { tile: '1p', tsumogiri: false, called: false, riichi: false },
      { tile: '2p', tsumogiri: true, called: false, riichi: false },
      { tile: '3p', tsumogiri: false, called: true, riichi: false },
      { tile: '4p', tsumogiri: true, called: true, riichi: true },
    ],
  })),
  event: { kind: 'dahai', actor: 0, target: null, tile: '4p' },
};
const props = {
  frame,
  names: ['自己', '下家', '对家', '上家'],
  player: 0,
  reveal: false,
  selected: null,
  onSelect: vi.fn(),
};

afterEach(cleanup);

it('四家四杠、长牌河与普通局面切换时，不再设置随局面变化的尺寸', () => {
  const { rerender } = render(<Board {...props} />);
  const board = screen.getByLabelText('当前牌桌');
  const ordinaryStyle = board.getAttribute('style');
  rerender(
    <Board
      {...props}
      frame={{
        ...frame,
        drawn: [0, 'P'],
        players: frame.players.map((p, i) => ({
          ...p,
          concealed: ['P', 'P'],
          melds: ['1m', '3p', '7s', 'C'].map((tile, j) => ({
            kind: 'daiminkan',
            tiles: [tile, tile, tile, tile],
            called: tile,
            from: (i + (j % 3) + 1) % 4,
          })),
          discards: Array.from({ length: 36 }, (_, j) => ({
            tile: '1p',
            tsumogiri: false,
            called: false,
            riichi: j === 8,
          })),
        })),
      }}
    />,
  );
  expect(board.getAttribute('style')).toBe(ordinaryStyle);
  for (const name of props.names) {
    expect(within(screen.getByLabelText(`${name}的副露`)).getAllByRole('img')).toHaveLength(16);
    const river = screen.getByLabelText(`${name}的牌河`);
    expect(within(river).getAllByRole('img')).toHaveLength(36);
    expect(river.getAttribute('style')).toBeNull();
  }
});

it('切换视角后点况仍对应原玩家，且不随牌面容器旋转', () => {
  render(<Board {...props} player={1} />);
  const status = screen.getByRole('group', { name: '自己的点况' });
  expect(status.closest('.seat')).toBeNull();
  expect(within(status).getByText('自己')).toBeTruthy();
  expect(within(status).getByText('25,000')).toBeTruthy();
  expect(screen.getAllByRole('group', { name: /的点况$/ })).toHaveLength(4);
});

it('牌背保持暗牌语义，隐藏时不加载真实牌面，自己的手牌仍可选择', async () => {
  const onSelect = vi.fn();
  const { rerender } = render(<Board {...props} onSelect={onSelect} />);
  const opponent = screen.getByLabelText('下家的手牌');
  expect(within(opponent).getAllByRole('img', { name: '暗牌' })).toHaveLength(10);
  expect(opponent.querySelector('img')).toBeNull();
  await userEvent.click(within(screen.getByLabelText('自己的手牌')).getByLabelText('一万'));
  expect(onSelect).toHaveBeenCalledExactlyOnceWith('1m');
  rerender(<Board {...props} reveal />);
  expect(within(opponent).queryByRole('img', { name: '暗牌' })).toBeNull();
  expect(within(opponent).getByRole('img', { name: '一万' })).toBeTruthy();
});

it('被鸣走的牌保留原位，摸切与立直标记同时保留', () => {
  render(<Board {...props} />);
  const river = within(screen.getByLabelText('自己的牌河'));
  expect(river.getAllByRole('img')).toHaveLength(4);
  expect(river.getByTitle('一饼 · 手切')).toBeTruthy();
  expect(river.getByTitle('二饼 · 摸切')).toBeTruthy();
  expect(river.getByTitle('三饼 · 手切 · 已被鸣走')).toBeTruthy();
  expect(river.getByTitle('四饼 · 摸切 · 已被鸣走 · 立直宣言牌')).toBeTruthy();
});

it('副露保留全部牌和来源，暗杠仍只展示中间两张', () => {
  render(
    <Board
      {...props}
      frame={{
        ...frame,
        players: frame.players.map((p, i) =>
          i === 0
            ? {
                ...p,
                melds: [
                  { kind: 'daiminkan', tiles: ['5m', '5m', '5m', '5mr'], called: '5mr', from: 1 },
                  { kind: 'ankan', tiles: ['1s', '1s', '1s', '1s'], called: null, from: null },
                ],
              }
            : p,
        ),
      }}
    />,
  );
  const melds = within(screen.getByLabelText('自己的副露'));
  const kan = melds.getByRole('group', { name: '大明杠' });
  expect(within(kan).getAllByRole('img')).toHaveLength(4);
  expect(within(kan).getByRole('img', { name: '赤五万' })).toBeTruthy();
  expect(kan.title).toBe('大明杠 · 来自玩家 2');
  const concealedKan = within(melds.getByRole('group', { name: '暗杠' }));
  expect(concealedKan.getAllByRole('img', { name: '暗牌' })).toHaveLength(2);
  expect(concealedKan.getAllByRole('img', { name: '一索' })).toHaveLength(2);
});
