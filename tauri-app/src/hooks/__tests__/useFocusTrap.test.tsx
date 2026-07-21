import { afterEach, describe, expect, it } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { useRef } from 'react';
import { useFocusTrap } from '../useFocusTrap';

interface HarnessProps {
  active?: boolean;
  autoFocusSecond?: boolean;
  disableThird?: boolean;
}

function Harness({ active = true, autoFocusSecond = false, disableThird = false }: HarnessProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  useFocusTrap(containerRef, active);
  return (
    <div>
      <button data-testid="outside">outside</button>
      <div ref={containerRef} data-testid="container">
        <button data-testid="first">first</button>
        <button data-testid="second" autoFocus={autoFocusSecond}>second</button>
        <button data-testid="third" disabled={disableThird}>third</button>
      </div>
    </div>
  );
}

afterEach(cleanup);

describe('useFocusTrap', () => {
  it('focuses the first focusable element on mount', () => {
    render(<Harness />);
    expect(document.activeElement).toBe(screen.getByTestId('first'));
  });

  it('does not steal focus from an autoFocus element inside the container', () => {
    render(<Harness autoFocusSecond />);
    expect(document.activeElement).toBe(screen.getByTestId('second'));
  });

  it('does nothing when inactive', () => {
    render(<Harness active={false} />);
    expect(document.activeElement).not.toBe(screen.getByTestId('first'));
    const third = screen.getByTestId('third');
    third.focus();
    fireEvent.keyDown(third, { key: 'Tab' });
    expect(document.activeElement).toBe(third);
  });

  it('wraps Tab from the last element to the first', () => {
    render(<Harness />);
    const third = screen.getByTestId('third');
    third.focus();
    fireEvent.keyDown(third, { key: 'Tab' });
    expect(document.activeElement).toBe(screen.getByTestId('first'));
  });

  it('wraps Shift+Tab from the first element to the last', () => {
    render(<Harness />);
    const first = screen.getByTestId('first');
    first.focus();
    fireEvent.keyDown(first, { key: 'Tab', shiftKey: true });
    expect(document.activeElement).toBe(screen.getByTestId('third'));
  });

  it('does not intercept Tab between middle elements', () => {
    render(<Harness />);
    const first = screen.getByTestId('first');
    first.focus();
    // jsdom has no native Tab traversal, so an uncancelled Tab leaves focus put.
    // The trap must not preventDefault here (the browser handles in-bounds moves).
    const cancelled = !fireEvent.keyDown(first, { key: 'Tab' });
    expect(cancelled).toBe(false);
    expect(document.activeElement).toBe(first);
  });

  it('skips disabled elements when wrapping', () => {
    render(<Harness disableThird />);
    const second = screen.getByTestId('second');
    second.focus();
    fireEvent.keyDown(second, { key: 'Tab' });
    expect(document.activeElement).toBe(screen.getByTestId('first'));
  });

  it('redirects Tab into the trap when focus is outside the container', () => {
    render(<Harness />);
    const container = screen.getByTestId('container');
    screen.getByTestId('outside').focus();
    fireEvent.keyDown(container, { key: 'Tab' });
    expect(document.activeElement).toBe(screen.getByTestId('first'));
  });
});
