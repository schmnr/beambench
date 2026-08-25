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

  it('localizes segment counts and failed bed bounds', () => {
    const report = makeReport('fail');
    report.checks = [
      { category: 'plan', description: 'Plan has segments', passed: true, message: '1827 segments' },
      {
        category: 'bounds',
        description: 'Plan fits within machine bed',
        passed: false,
        message: 'Plan bounds (-8.1,9.3 to 156.0,143.3) exceed bed (300x300mm)',
      },
    ];

    render(<PreflightDialog report={report} onClose={vi.fn()} />);

    expect(screen.getByText('1827 segments')).toBeTruthy();
    expect(screen.queryByText('dialog.preflight.messages.segment_count')).toBeNull();
    expect(screen.getByText(/planned bounds.*-8.1.*300 × 300 mm/i)).toBeTruthy();
  });

  it('gives mode-specific instructions for raster motion outside the bed', () => {
    const report = makeReport('fail');
    report.checks = [{
      category: 'bounds',
      description: 'Raster motion (overscan and scanning offset) fits within machine bed',
      passed: false,
      message: 'Raster motion spans -15.9 to 156.1mm on the 0 to 300mm X axis (8.0mm of overscan and scanning offset beyond the burn area). Reduce overscan or move the design further from the bed edge.',
    }];

    render(
      <PreflightDialog
        report={report}
        onClose={vi.fn()}
        startFrom="user_origin"
      />,
    );

    expect(screen.getByText(/move or reset User Origin farther from the bed edge/i)).toBeTruthy();
    expect(screen.queryByText(/move the design further/i)).toBeNull();
  });
});
