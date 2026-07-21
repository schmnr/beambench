import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { Toggle } from '../Toggle';

describe('Toggle', () => {
  it('provides a larger wrapper hit target and supports a full-width leading label', () => {
    const onChange = vi.fn();

    render(
      <Toggle
        label="Enable Dot Width Correction"
        labelFirst
        className="w-full"
        checked={false}
        onChange={onChange}
      />,
    );

    const checkbox = screen.getByRole('checkbox', { name: 'Enable Dot Width Correction' });
    const hitTarget = checkbox.closest('label');

    expect(hitTarget).not.toBeNull();
    expect(hitTarget?.classList.contains('min-h-6')).toBe(true);
    expect(hitTarget?.classList.contains('min-w-6')).toBe(true);
    expect(hitTarget?.classList.contains('w-full')).toBe(true);
    expect(hitTarget?.firstElementChild?.textContent).toBe('Enable Dot Width Correction');

    fireEvent.click(hitTarget!);

    expect(onChange).toHaveBeenCalledWith(true);
  });
});
