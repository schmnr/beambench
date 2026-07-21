import { describe, it, expect, vi, afterEach } from 'vitest';
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
