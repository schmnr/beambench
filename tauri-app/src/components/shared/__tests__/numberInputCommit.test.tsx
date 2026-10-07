import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { NumberInput } from '../NumberInput';

afterEach(cleanup);

describe('NumberInput commit="blur"', () => {
  it('saves once when the field is left, not on every keystroke', () => {
    const change = vi.fn();
    render(<NumberInput label="Interval" value={0.2} min={0.01} step={0.01} onChange={change} commit="blur" />);
    const input = screen.getByRole('spinbutton');
    fireEvent.change(input, { target: { value: '0' } });
    fireEvent.change(input, { target: { value: '0.1' } });
    fireEvent.change(input, { target: { value: '0.15' } });
    expect(change).not.toHaveBeenCalled();
    fireEvent.blur(input);
    expect(change).toHaveBeenCalledTimes(1);
    expect(change).toHaveBeenCalledWith(0.15);
  });

  it('clamps to the allowed range and ignores an empty field', () => {
    const change = vi.fn();
    render(<NumberInput label="Interval" value={0.2} min={0.01} max={10} onChange={change} commit="blur" />);
    const input = screen.getByRole('spinbutton');
    fireEvent.change(input, { target: { value: '0' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(change).toHaveBeenCalledWith(0.01);
    change.mockClear();
    fireEvent.change(input, { target: { value: '' } });
    fireEvent.blur(input);
    expect(change).not.toHaveBeenCalled();
  });

  it('Escape abandons the edit', () => {
    const change = vi.fn();
    render(<NumberInput label="Speed" value={100} onChange={change} commit="blur" />);
    const input = screen.getByRole('spinbutton') as HTMLInputElement;
    fireEvent.change(input, { target: { value: '5' } });
    fireEvent.keyDown(input, { key: 'Escape' });
    fireEvent.blur(input);
    expect(change).not.toHaveBeenCalled();
    expect(input.value).toBe('100');
  });
});

it('discards a draft when the external value changes', () => {
  const change = vi.fn();
  const { rerender } = render(
    <NumberInput label="Speed" value={100} onChange={change} commit="blur" />,
  );
  fireEvent.change(screen.getByRole('spinbutton'), { target: { value: '5' } });
  rerender(
    <NumberInput label="Speed" value={200} onChange={change} commit="blur" />,
  );
  expect((screen.getByRole('spinbutton') as HTMLInputElement).value).toBe(
    '200',
  );
  fireEvent.blur(screen.getByRole('spinbutton'));
  expect(change).not.toHaveBeenCalled();
});
