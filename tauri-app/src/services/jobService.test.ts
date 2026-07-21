import { describe, it, expect, vi, beforeEach } from 'vitest';

// Mock machineService
vi.mock('./machineService', () => ({
  machineService: {
    runPreflightCheck: vi.fn(),
    startJob: vi.fn(),
    getJobProgress: vi.fn(),
    pauseJob: vi.fn(),
    resumeJob: vi.fn(),
    cancelJob: vi.fn(),
  },
}));

import { jobService } from './jobService';
import { machineService } from './machineService';
import type { PreflightReport, JobProgress } from '../types/machine';

const passReport: PreflightReport = {
  outcome: 'pass',
  checks: [{ category: 'connection', description: 'Machine connected', passed: true, message: '' }],
};

const failReport: PreflightReport = {
  outcome: 'fail',
  checks: [{ category: 'connection', description: 'Machine connected', passed: false, message: 'Not connected' }],
};

const warningReport: PreflightReport = {
  outcome: 'pass_with_warnings',
  checks: [{ category: 'settings', description: 'Laser mode enabled ($32=1)', passed: false, message: 'Laser mode disabled' }],
};

const mockProgress: JobProgress = {
  state: 'running',
  total_lines: 100,
  queued_lines: 0,
  sent_lines: 50,
  acknowledged_lines: 45,
  elapsed_secs: 30,
  estimated_remaining_secs: 30,
  buffer_fill_bytes: 64,
};

describe('jobService', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('startWithPreflight runs preflight first, starts job on pass', async () => {
    vi.mocked(machineService.runPreflightCheck).mockResolvedValue(passReport);
    vi.mocked(machineService.startJob).mockResolvedValue(mockProgress);

    const result = await jobService.startWithPreflight();

    expect(machineService.runPreflightCheck).toHaveBeenCalled();
    expect(machineService.startJob).toHaveBeenCalled();
    expect(result.report.outcome).toBe('pass');
    expect(result.progress).toEqual(mockProgress);
  });

  it('startWithPreflight does not start job on fail', async () => {
    vi.mocked(machineService.runPreflightCheck).mockResolvedValue(failReport);

    const result = await jobService.startWithPreflight();

    expect(machineService.runPreflightCheck).toHaveBeenCalled();
    expect(machineService.startJob).not.toHaveBeenCalled();
    expect(result.report.outcome).toBe('fail');
    expect(result.progress).toBeUndefined();
  });

  it('startWithPreflight does not start job on warning-tier preflight', async () => {
    vi.mocked(machineService.runPreflightCheck).mockResolvedValue(warningReport);

    const result = await jobService.startWithPreflight();

    expect(machineService.runPreflightCheck).toHaveBeenCalled();
    expect(machineService.startJob).not.toHaveBeenCalled();
    expect(result.report.outcome).toBe('pass_with_warnings');
    expect(result.progress).toBeUndefined();
  });

  it('getProgress delegates to machineService', async () => {
    vi.mocked(machineService.getJobProgress).mockResolvedValue(mockProgress);

    const result = await jobService.getProgress();

    expect(machineService.getJobProgress).toHaveBeenCalled();
    expect(result).toEqual(mockProgress);
  });

  it('pause/resume/cancel delegate to machineService', async () => {
    vi.mocked(machineService.pauseJob).mockResolvedValue();
    vi.mocked(machineService.resumeJob).mockResolvedValue();
    vi.mocked(machineService.cancelJob).mockResolvedValue();

    await jobService.pause();
    expect(machineService.pauseJob).toHaveBeenCalled();

    await jobService.resume();
    expect(machineService.resumeJob).toHaveBeenCalled();

    await jobService.cancel();
    expect(machineService.cancelJob).toHaveBeenCalled();
  });
});
