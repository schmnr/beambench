import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, cleanup, fireEvent } from '@testing-library/react';
import { PreflightDialog } from '../PreflightDialog';
import type { PreflightReport } from '../../../types/machine';

afterEach(cleanup);

const ADVISORY_COPY = /correcting the advisory may improve engraving quality/i;

function makeReport(outcome: PreflightReport['outcome']): PreflightReport {
  return {
    outcome,
    checks: [
      { category: 'connection', description: 'Machine connected', passed: true, message: '' },
    ],
    advisories:
      outcome === 'pass_with_warnings'
        ? [
            {
              code: 'raster_overscan',
              description: 'Raster acceleration needs more overscan',
              message: 'Increase overscan or reduce speed.',
              recommended_overscan_mm: 16.7,
              recommended_speed_mm_min: 2300,
            },
          ]
        : [],
  };
}

describe('PreflightDialog', () => {
  it('presents warnings as a correctable quality advisory', () => {
    render(<PreflightDialog report={makeReport('pass_with_warnings')} onClose={vi.fn()} />);

    const copy = screen.getByText(ADVISORY_COPY);
    expect(copy).toBeTruthy();
    expect(copy.className).toContain('text-bb-warning');
  });

  it('does not show the warnings copy on a clean pass', () => {
    render(<PreflightDialog report={makeReport('pass')} onClose={vi.fn()} />);
    expect(screen.queryByText(ADVISORY_COPY)).toBeNull();
  });

  it('does not show the warnings copy on fail', () => {
    render(<PreflightDialog report={makeReport('fail')} onClose={vi.fn()} />);
    expect(screen.queryByText(ADVISORY_COPY)).toBeNull();
  });

  it('offers both automatic corrections and deliberate continuation', () => {
    const onApplyOverscan = vi.fn();
    const onReduceSpeed = vi.fn();
    const onContinue = vi.fn();
    render(
      <PreflightDialog
        report={makeReport('pass_with_warnings')}
        onClose={vi.fn()}
        onApplyOverscan={onApplyOverscan}
        onReduceSpeed={onReduceSpeed}
        onContinue={onContinue}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: /16.7 mm overscan/i }));
    fireEvent.click(screen.getByRole('button', { name: /2300 mm\/min/i }));
    fireEvent.click(screen.getByRole('button', { name: /continue anyway/i }));

    expect(onApplyOverscan).toHaveBeenCalledWith(16.7);
    expect(onReduceSpeed).toHaveBeenCalledWith(2300);
    expect(onContinue).toHaveBeenCalledOnce();
  });

  it('uses the safest speed when multiple advisories recommend limits', () => {
    const onReduceSpeed = vi.fn();
    const report = makeReport('pass_with_warnings');
    report.advisories?.push({
      code: 'speed_limited',
      description: 'Requested speed exceeds the machine limit',
      message: 'The active profile limits this job to 1800 mm/min.',
      recommended_speed_mm_min: 1800,
    });
    render(<PreflightDialog report={report} onClose={vi.fn()} onReduceSpeed={onReduceSpeed} />);

    fireEvent.click(screen.getByRole('button', { name: /1800 mm\/min/i }));

    expect(onReduceSpeed).toHaveBeenCalledWith(1800);
  });
});
