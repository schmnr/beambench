import { describe, expect, it } from 'vitest';

import { formatQualityTestError, formatQualityTestWarning } from './qualityTestService';

describe('formatQualityTestError', () => {
  it('formats active-job and internal quality-test errors', () => {
    expect(formatQualityTestError({ kind: 'job_in_progress' })).toMatch(/already active/);
    expect(formatQualityTestError({ kind: 'internal', message: 'planner failed' })).toBe(
      'Internal error: planner failed',
    );
    expect(formatQualityTestError({ kind: 'internal' })).toBe(
      'Internal error while running quality test.',
    );
  });

  it('falls back for unknown error shapes', () => {
    expect(formatQualityTestError({ kind: 'future_error', detail: 'new backend shape' })).toBe(
      '{"kind":"future_error","detail":"new backend shape"}',
    );
  });

  it('falls back for unknown warning shapes', () => {
    expect(formatQualityTestWarning({ kind: 'future_warning' } as never)).toBe(
      'Unknown quality-test warning.',
    );
  });
});
