import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, cleanup } from '@testing-library/react';
import { PreflightDialog } from '../PreflightDialog';
import type { PreflightReport } from '../../../types/machine';

afterEach(cleanup);

// New i18n key is not in the locale files yet; i18next returns the raw key.
const WARNINGS_BLOCK_KEY = /Resolve the warnings above before starting the job/;

function makeReport(outcome: PreflightReport['outcome']): PreflightReport {
  return {
    outcome,
    checks: [
      { category: 'connection', description: 'Machine connected', passed: true, message: '' },
    ],
  };
}

describe('PreflightDialog', () => {
  it('explains that warnings must be resolved when outcome is pass_with_warnings', () => {
    render(<PreflightDialog report={makeReport('pass_with_warnings')} onClose={vi.fn()} />);

    const copy = screen.getByText(WARNINGS_BLOCK_KEY);
    expect(copy).toBeTruthy();
    expect(copy.className).toContain('text-bb-warning');
  });

  it('does not show the warnings copy on a clean pass', () => {
    render(<PreflightDialog report={makeReport('pass')} onClose={vi.fn()} />);
    expect(screen.queryByText(WARNINGS_BLOCK_KEY)).toBeNull();
  });

  it('does not show the warnings copy on fail', () => {
    render(<PreflightDialog report={makeReport('fail')} onClose={vi.fn()} />);
    expect(screen.queryByText(WARNINGS_BLOCK_KEY)).toBeNull();
  });
});
