import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { PostJobCompatibilityDialog } from '../PostJobCompatibilityDialog';

afterEach(cleanup);

describe('PostJobCompatibilityDialog', () => {
  it('states that nothing is automatic and exposes all three outcomes', () => {
    render(
      <PostJobCompatibilityDialog
        profileName="K40"
        onCompleted={vi.fn()}
        onProblem={vi.fn()}
        onNotNow={vi.fn()}
      />,
    );

    expect(screen.getByText('Did this job finish correctly?')).toBeDefined();
    expect(screen.getByText(/Nothing is sent automatically/)).toBeDefined();
    expect(screen.getByText(/compatibility with K40/)).toBeDefined();
    expect(screen.getByRole('button', { name: 'Yes, it completed' })).toBeDefined();
    expect(screen.getByRole('button', { name: 'Report a problem' })).toBeDefined();
    expect(screen.getAllByRole('button', { name: 'Not now' }).length).toBeGreaterThan(0);
  });

  it('reports the selected outcome without sending anything itself', () => {
    const onCompleted = vi.fn();
    const onProblem = vi.fn();
    render(
      <PostJobCompatibilityDialog
        onCompleted={onCompleted}
        onProblem={onProblem}
        onNotNow={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'Yes, it completed' }));
    expect(onCompleted).toHaveBeenCalledOnce();
    expect(onProblem).not.toHaveBeenCalled();
  });
});
