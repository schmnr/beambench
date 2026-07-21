import { invoke } from '@tauri-apps/api/core';
import { save } from '@tauri-apps/plugin-dialog';
import i18n from '../i18n';
import type { PreviewData } from '../types/preview';
import type { ExecutionPlan, PlanStats } from '../types/plan';
import type { SessionJobOptions } from '../types/jobOptions';
import { measureAsyncPerf } from './perfMarks';

export const previewService = {
  async generatePlan(jobOptions?: SessionJobOptions): Promise<ExecutionPlan> {
    return measureAsyncPerf('generate_plan', () =>
      invoke<ExecutionPlan>('generate_plan', { jobOptions })
    );
  },

  async getPlanStats(): Promise<PlanStats> {
    return invoke<PlanStats>('get_plan_stats');
  },

  async generatePreview(jobOptions?: SessionJobOptions): Promise<PreviewData> {
    return measureAsyncPerf('generate_preview', () =>
      invoke<PreviewData>('generate_preview', { jobOptions })
    );
  },

  async cancelPlanning(): Promise<void> {
    return invoke<void>('cancel_planning');
  },

  async exportGcode(jobOptions?: SessionJobOptions): Promise<string> {
    const selected = await save({
      title: i18n.t('file_dialogs.export_gcode_title'),
      defaultPath: 'output.gcode',
      filters: [{
        name: i18n.t('file_dialogs.filter_gcode'),
        extensions: ['gcode', 'nc', 'ngc'],
      }],
    });

    if (selected === null) {
      throw new Error('Export cancelled');
    }

    return invoke<string>('export_gcode', { path: selected, jobOptions });
  },

  // Optimization is part of the persisted project (`Project.optimization`);
  // mutate it through `projectService.setOptimization` and the corresponding
  // project-store action.
};
