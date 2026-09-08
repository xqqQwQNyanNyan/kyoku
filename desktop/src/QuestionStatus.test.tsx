// @vitest-environment jsdom
import { act, cleanup, render, screen } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { QuestionStatus } from './QuestionStatus';

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

it('阶段变化保留累计用时，停止状态优先于工具状态', () => {
  vi.useFakeTimers();
  const startedAt = Date.now();
  const { rerender } = render(
    <QuestionStatus
      progress={{ startedAt, stopping: false, stage: { phase: 'model', request: 1 } }}
    />,
  );
  act(() => vi.advanceTimersByTime(65_000));
  expect(screen.getByText('已用 1 分 5 秒')).toBeTruthy();
  rerender(
    <QuestionStatus
      progress={{ startedAt, stopping: false, stage: { phase: 'tool', name: 'analyze_hand' } }}
    />,
  );
  expect(screen.getByText('分析手牌与打点…')).toBeTruthy();
  expect(screen.getByText('已用 1 分 5 秒')).toBeTruthy();
  rerender(
    <QuestionStatus
      progress={{ startedAt, stopping: true, stage: { phase: 'tool', name: 'analyze_hand' } }}
    />,
  );
  expect(screen.getByText('正在停止…')).toBeTruthy();
  expect(screen.queryByText('分析手牌与打点…')).toBeNull();
});
