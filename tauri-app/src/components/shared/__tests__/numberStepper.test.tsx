import { describe, it, expect, vi, afterEach } from 'vitest';
import { useState } from 'react';
import { render, cleanup, fireEvent } from '@testing-library/react';
import { NumberStepper } from '../NumberStepper';

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

function getStepperButtons(container: HTMLElement) {
  const buttons = container.querySelectorAll('button');
  return { increment: buttons[0] as HTMLButtonElement, decrement: buttons[1] as HTMLButtonElement };
}

describe('NumberStepper', () => {
  it('allows an empty edit without sending zero to a numeric parent', () => {
    const onChange = vi.fn();
    function Field() {
      const [value, setValue] = useState(1000);
      return <NumberStepper value={value} min={1} onChange={(event) => {
        onChange(Number(event.target.value));
        setValue(Math.max(1, Number(event.target.value)));
      }} />;
    }
    const { getByRole } = render(<Field />);
    const input = getByRole('spinbutton') as HTMLInputElement;

    for (const value of ['100', '10', '1', '']) {
      fireEvent.change(input, { target: { value } });
      expect(input.value).toBe(value);
    }
    expect(onChange.mock.calls.map(([value]) => value)).toEqual([100, 10, 1]);
    fireEvent.change(input, { target: { value: '2500' } });
    expect(input.value).toBe('2500');
    expect(onChange).toHaveBeenLastCalledWith(2500);
  });

  it.each(['blur', 'Enter'])('restores the last numeric value on %s when left empty', (finish) => {
    const onChange = vi.fn();
    const { getByRole } = render(<NumberStepper value={1000} onChange={onChange} />);
    const input = getByRole('spinbutton') as HTMLInputElement;
    fireEvent.focus(input);
    fireEvent.change(input, { target: { value: '' } });
    expect(input.value).toBe('');
    if (finish === 'blur') fireEvent.blur(input);
    else fireEvent.keyDown(input, { key: 'Enter' });
    expect(input.value).toBe('1000');
    expect(onChange).not.toHaveBeenCalled();
  });

  it('uses an external value update when a numeric field is empty', () => {
    const onChange = vi.fn();
    const { getByRole, rerender } = render(<NumberStepper value={1000} onChange={onChange} />);
    const input = getByRole('spinbutton') as HTMLInputElement;
    fireEvent.change(input, { target: { value: '' } });
    expect(input.value).toBe('');
    rerender(<NumberStepper value={2000} onChange={onChange} />);
    expect(input.value).toBe('2000');
  });

  it.each(['0', '-2.5', '0.125'])('accepts %s as a numeric replacement after clearing', (replacement) => {
    const onChange = vi.fn();
    function Field() {
      const [value, setValue] = useState(12);
      return <NumberStepper value={value} step="any" onChange={(event) => {
        onChange(event.target.valueAsNumber);
        setValue(event.target.valueAsNumber);
      }} />;
    }
    const { getByRole } = render(<Field />);
    const input = getByRole('spinbutton') as HTMLInputElement;
    fireEvent.change(input, { target: { value: '' } });
    fireEvent.change(input, { target: { value: replacement } });
    expect(input.value).toBe(replacement);
    expect(onChange).toHaveBeenCalledExactlyOnceWith(Number(replacement));
  });

  it('steps from the last numeric value after clearing, respecting bounds', () => {
    function Field() {
      const [value, setValue] = useState(9);
      return <NumberStepper aria-label="Count" value={value} min={1} max={10}
        onChange={(event) => setValue(Number(event.target.value))} />;
    }
    const { getByRole } = render(<Field />);
    const input = getByRole('spinbutton') as HTMLInputElement;
    fireEvent.change(input, { target: { value: '' } });
    fireEvent.click(getByRole('button', { name: 'Increase Count' }));
    expect(input.value).toBe('10');
    expect((getByRole('button', { name: 'Increase Count' }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(getByRole('button', { name: 'Decrease Count' }));
    expect(input.value).toBe('9');
  });

  it('preserves empty changes and commit handlers for string-backed fields', () => {
    const onBlur = vi.fn();
    const onKeyDown = vi.fn();
    const onChange = vi.fn();
    function Field() {
      const [value, setValue] = useState('12');
      return <NumberStepper value={value} onBlur={onBlur} onKeyDown={onKeyDown}
        onChange={(event) => { onChange(event.target.value); setValue(event.target.value); }} />;
    }
    const { getByRole } = render(<Field />);
    const input = getByRole('spinbutton') as HTMLInputElement;
    fireEvent.change(input, { target: { value: '' } });
    expect(onChange).toHaveBeenLastCalledWith('');
    fireEvent.keyDown(input, { key: 'Enter' });
    fireEvent.blur(input);
    expect(input.value).toBe('');
    expect(onKeyDown).toHaveBeenCalledOnce();
    expect(onBlur).toHaveBeenCalledOnce();
  });

  it('renders both stepper buttons enabled when value is within range', () => {
    const { container } = render(
      <NumberStepper value={5} min={0} max={10} onChange={vi.fn()} />,
    );
    const { increment, decrement } = getStepperButtons(container);
    expect(increment.disabled).toBe(false);
    expect(decrement.disabled).toBe(false);
  });

  it('disables the decrement button at min', () => {
    const { container } = render(
      <NumberStepper value={0} min={0} max={10} onChange={vi.fn()} />,
    );
    const { increment, decrement } = getStepperButtons(container);
    expect(decrement.disabled).toBe(true);
    expect(decrement.className).toContain('text-bb-text-disabled');
    expect(increment.disabled).toBe(false);
  });

  it('disables the increment button at max', () => {
    const { container } = render(
      <NumberStepper value={10} min={0} max={10} onChange={vi.fn()} />,
    );
    const { increment, decrement } = getStepperButtons(container);
    expect(increment.disabled).toBe(true);
    expect(increment.className).toContain('text-bb-text-disabled');
    expect(decrement.disabled).toBe(false);
  });

  it('leaves both buttons enabled when min/max are omitted', () => {
    const { container } = render(<NumberStepper value={0} onChange={vi.fn()} />);
    const { increment, decrement } = getStepperButtons(container);
    expect(increment.disabled).toBe(false);
    expect(decrement.disabled).toBe(false);
  });

  it('pointerdown on an at-limit button does not fire onChange', () => {
    const onChange = vi.fn();
    const { container } = render(
      <NumberStepper value={0} min={0} max={10} onChange={onChange} />,
    );
    const { decrement } = getStepperButtons(container);
    fireEvent.pointerDown(decrement);
    expect(onChange).not.toHaveBeenCalled();
  });

  it('stops the repeat timer on pointercancel', () => {
    vi.useFakeTimers();
    const { container } = render(
      <NumberStepper value={5} min={0} max={10} onChange={vi.fn()} />,
    );
    const { increment } = getStepperButtons(container);

    fireEvent.pointerDown(increment);
    expect(vi.getTimerCount()).toBeGreaterThan(0);

    fireEvent.pointerCancel(increment);
    expect(vi.getTimerCount()).toBe(0);
  });

  it('clears the repeat timer on unmount so it cannot leak', () => {
    vi.useFakeTimers();
    const { container, unmount } = render(
      <NumberStepper value={5} min={0} max={10} onChange={vi.fn()} />,
    );
    const { increment } = getStepperButtons(container);

    fireEvent.pointerDown(increment);
    expect(vi.getTimerCount()).toBeGreaterThan(0);

    unmount();
    expect(vi.getTimerCount()).toBe(0);
  });
});

it('names step controls and supports assistive activation without double pointer steps', () => {
  const onChange = vi.fn();
  const { getByRole } = render(<NumberStepper aria-label="Power" value={5} onChange={onChange} />);
  const increase = getByRole('button', {name:'Increase Power'});
  expect(increase.tabIndex).toBe(0);
  fireEvent.click(increase, {detail:0});
  expect(onChange).toHaveBeenCalledTimes(1);
  onChange.mockClear();
  fireEvent.pointerDown(increase);
  fireEvent.pointerUp(increase);
  fireEvent.click(increase, {detail:1});
  expect(onChange).toHaveBeenCalledTimes(1);
});
