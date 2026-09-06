// @vitest-environment jsdom
import { useState } from 'react';
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it } from 'vitest';
import { Select } from './Select';

afterEach(cleanup);
const options = [
  { value: 'a', label: '自己' },
  { value: 'b', label: '下家' },
  { value: 'c', label: '对家' },
  { value: 'd', label: '上家' },
];
function Picker() {
  const [value, setValue] = useState('b');
  return (
    <>
      <Select label="玩家" value={value} options={options} onChange={setValue} />
      <button>外部按钮</button>
    </>
  );
}

it('Escape 取消未确认的选择，点击外部关闭菜单', async () => {
  render(<Picker />);
  const picker = screen.getByRole('combobox');
  await userEvent.click(picker);
  await userEvent.keyboard('{ArrowDown}{Escape}');
  expect(picker.textContent).toContain('下家');
  expect(screen.queryByRole('listbox')).toBeNull();
  expect(document.activeElement).toBe(picker);
  await userEvent.click(picker);
  await userEvent.click(screen.getByRole('button', { name: '外部按钮' }));
  expect(screen.queryByRole('listbox')).toBeNull();
});

it('Home、End 和方向键浏览选项，Enter 确认，Tab 离开', async () => {
  render(<Picker />);
  const picker = screen.getByRole('combobox');
  picker.focus();
  await userEvent.keyboard('{ArrowDown}{End}{Enter}');
  expect(picker.textContent).toContain('上家');
  await userEvent.keyboard('{Home}{Enter}');
  expect(picker.textContent).toContain('自己');
  await userEvent.keyboard('{ArrowDown}{ArrowDown}{Tab}');
  expect(picker.textContent).toContain('自己');
  expect(screen.queryByRole('listbox')).toBeNull();
  expect(document.activeElement).toBe(screen.getByRole('button', { name: '外部按钮' }));
});

it('鼠标选择后保留触发器焦点，禁用时无法展开', async () => {
  const { rerender } = render(<Picker />);
  const picker = screen.getByRole('combobox');
  await userEvent.click(picker);
  await userEvent.click(screen.getByRole('option', { name: '对家' }));
  expect(picker.textContent).toContain('对家');
  expect(document.activeElement).toBe(picker);
  expect(screen.queryByRole('listbox')).toBeNull();
  rerender(<Select label="玩家" value="a" options={options} disabled onChange={() => {}} />);
  await userEvent.click(screen.getByRole('combobox'));
  expect(screen.queryByRole('listbox')).toBeNull();
});
