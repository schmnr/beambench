import { invoke } from '@tauri-apps/api/core';
import { save } from '@tauri-apps/plugin-dialog';
import i18n from '../i18n';
import type {
  ConnectionDiagnosticsSnapshot,
  DiagnosticBundleV1,
  FeedbackKind,
  FeedbackReportInput,
  SavedReport,
  SubmitFeedbackResponse,
} from '../types/feedback';

function defaultReportFilename(kind: FeedbackKind, includeProjectFile: boolean): string {
  const stamp = new Date().toISOString().replace(/[:.]/g, '-');
  return `beambench-report-${kind}-${stamp}.${includeProjectFile ? 'zip' : 'json'}`;
}

export const feedbackService = {
  getConnectionDiagnostics(): Promise<ConnectionDiagnosticsSnapshot> {
    return invoke<ConnectionDiagnosticsSnapshot>('get_connection_diagnostics');
  },

  previewReport(input: FeedbackReportInput): Promise<DiagnosticBundleV1> {
    return invoke<DiagnosticBundleV1>('preview_feedback_report', { input });
  },

  async saveReport(input: FeedbackReportInput): Promise<SavedReport> {
    const extension = input.include_project_file ? 'zip' : 'json';
    const selected = await save({
      title: i18n.t('file_dialogs.save_report_title'),
      defaultPath: defaultReportFilename(input.kind, input.include_project_file),
      filters: [{
        name: input.include_project_file
          ? i18n.t('file_dialogs.filter_report_archive')
          : i18n.t('file_dialogs.filter_report_json'),
        extensions: [extension],
      }],
    });

    if (selected === null) {
      throw new Error('Save cancelled');
    }

    return invoke<SavedReport>('save_feedback_report', { input, path: selected });
  },

  submitReport(input: FeedbackReportInput): Promise<SubmitFeedbackResponse> {
    return invoke<SubmitFeedbackResponse>('submit_feedback_report', { input });
  },

  revealReport(path: string): Promise<void> {
    return invoke<void>('reveal_feedback_report', { path });
  },
};
