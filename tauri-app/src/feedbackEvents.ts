import type { FeedbackKind, FeedbackSourceContext } from './types/feedback';

export const FEEDBACK_REPORT_OPEN_EVENT = 'beam-bench-open-feedback-report';

export interface FeedbackReportOpenDetail {
  kind: FeedbackKind;
  title?: string;
  description?: string;
  notes?: string;
  sourceContext?: FeedbackSourceContext;
}

export function openFeedbackReport(detail: FeedbackReportOpenDetail): void {
  window.dispatchEvent(new CustomEvent<FeedbackReportOpenDetail>(FEEDBACK_REPORT_OPEN_EVENT, { detail }));
}
