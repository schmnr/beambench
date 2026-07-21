import { machineService } from './machineService';
import type { PreflightReport, JobProgress } from '../types/machine';

export const jobService = {
  async startWithPreflight(): Promise<{ report: PreflightReport; progress?: JobProgress }> {
    const report = await machineService.runPreflightCheck();

    if (report.outcome !== 'pass') {
      return { report };
    }

    const progress = await machineService.startJob();
    return { report, progress };
  },

  async getProgress(): Promise<JobProgress | null> {
    return machineService.getJobProgress();
  },

  async pause(): Promise<void> {
    return machineService.pauseJob();
  },

  async resume(): Promise<void> {
    return machineService.resumeJob();
  },

  async cancel(): Promise<void> {
    return machineService.cancelJob();
  },
};
