// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, expect, it } from 'vitest';
import { UsageDetails, UsageSummary, turnUsage } from './Usage';
import type { RequestUsage } from './types';

afterEach(cleanup);
const known: RequestUsage = {
  input_tokens: 1000,
  output_tokens: 200,
  cached_input_tokens: 400,
  reasoning_tokens: 150,
  cost: 0.003,
  prices: { currency: 'CNY', input: 2, output: 8, cached_input: 0.5 },
};
const unknown: RequestUsage = {
  input_tokens: null,
  output_tokens: null,
  cached_input_tokens: null,
  reasoning_tokens: null,
  cost: null,
  prices: null,
};

it('显示逐次 Token、缓存、思考和当时的计费价格', () => {
  render(<UsageDetails requests={[known]} />);
  expect(screen.getByText('1,000 / 200')).toBeTruthy();
  expect(screen.getByText('其中思考 150')).toBeTruthy();
  expect(screen.getByText('0.003000 CNY')).toBeTruthy();
  expect(screen.getByText('每百万：输入 2 / 输出 8 / 缓存 0.5')).toBeTruthy();
});

it('未知用量不会显示为零，部分费用明确标为已知小计', () => {
  const view = render(<UsageSummary requests={[unknown]} budget={200000} />);
  expect(screen.getByText(/输入 未知 \/ 输出 未知/)).toBeTruthy();
  view.rerender(<UsageSummary requests={[known, unknown]} budget={200000} />);
  expect(screen.getByText('已知估算费用 0.003000 CNY')).toBeTruthy();
  expect(screen.getByText('部分费用未知，未计入小计。')).toBeTruthy();
});

it('不同币种分别累计，不把思考 Token 再加到预算中', () => {
  render(
    <UsageSummary
      requests={[known, { ...known, prices: { ...known.prices!, currency: 'USD' } }]}
      budget={10000}
    />,
  );
  expect(screen.getByText('估算费用 0.003000 CNY + 0.003000 USD')).toBeTruthy();
  expect(screen.getByText('本轮预算：2,400 / 10,000 Token')).toBeTruthy();
});

it('旧会话从原始轨迹读取用量，失败请求保留未知且不套用价格', () => {
  const requests = turnUsage({
    question: '问题',
    answer: null,
    error: '失败',
    usage: [],
    options: null,
    trace: [
      { kind: 'request' },
      { kind: 'response', output: { usage: { prompt_tokens: 100, completion_tokens: 20 } } },
      { kind: 'request' },
    ],
  });
  expect(requests).toHaveLength(2);
  expect(requests[0].input_tokens).toBe(100);
  expect(requests[0].cost).toBeNull();
  expect(requests[1].input_tokens).toBeNull();
});
