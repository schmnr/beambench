import { invoke } from '@tauri-apps/api/core';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { DEFAULT_QUALITY_TEST_SETTINGS, type QualityTestRequest } from '../types/machine';
import { formatQualityTestError, formatQualityTestWarning, qualityTestService } from './qualityTestService';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn(), save: vi.fn() }));

const invokeMock = vi.mocked(invoke);

beforeEach(() => {
  invokeMock.mockReset();
});

describe('qualityTestService live placement contract', () => {
  it('forwards the identical request shape to frame and start', async () => {
    const request: QualityTestRequest = {
      kind: 'material',
      ...DEFAULT_QUALITY_TEST_SETTINGS.material,
    };
    invokeMock.mockResolvedValue({ state: 'running' });

    await qualityTestService.frame(request);
    await qualityTestService.start(request);

    expect(invokeMock).toHaveBeenNthCalledWith(1, 'quality_test_frame', { request });
    expect(invokeMock).toHaveBeenNthCalledWith(2, 'quality_test_start', { request });
  });
});

describe('formatQualityTestError', () => {
  it('formats active-job and internal quality-test errors', () => {
    expect(formatQualityTestError({ kind: 'job_in_progress' })).toMatch(/already active/);
    expect(formatQualityTestError({ kind: 'internal', message: 'planner failed' })).toBe(
      'Internal error: planner failed',
    );
    expect(formatQualityTestError({ kind: 'internal' })).toBe(
      'Internal error while running quality test.',
    );
    expect(formatQualityTestError({ kind: 'current_position_unavailable' })).toMatch(
      /connected machine/,
    );
    expect(formatQualityTestError({ kind: 'user_origin_not_set' })).toMatch(/Set Here/);
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
