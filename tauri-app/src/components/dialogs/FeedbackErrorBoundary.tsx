import React from 'react';
import { openFeedbackReport } from '../../feedbackEvents';
import i18n from '../../i18n';

interface FeedbackErrorBoundaryProps {
  children: React.ReactNode;
}

interface FeedbackErrorBoundaryState {
  error: Error | null;
  stack: string | null;
}

export class FeedbackErrorBoundary extends React.Component<FeedbackErrorBoundaryProps, FeedbackErrorBoundaryState> {
  state: FeedbackErrorBoundaryState = { error: null, stack: null };

  static getDerivedStateFromError(error: Error): FeedbackErrorBoundaryState {
    return { error, stack: error.stack ?? null };
  }

  componentDidCatch(error: Error, info: React.ErrorInfo) {
    this.setState({ error, stack: `${error.stack ?? error.message}\n${info.componentStack}` });
  }

  render() {
    const { error, stack } = this.state;
    if (!error) return this.props.children;

    return (
      <div className="flex h-screen items-center justify-center bg-bb-bg px-4 text-bb-text">
        <div className="w-[460px] max-w-full rounded-lg border border-bb-border bg-bb-panel p-5 shadow-2xl">
          <h1 className="text-base font-semibold">{i18n.t('errors.ui_error_title')}</h1>
          <p className="mt-2 text-sm text-bb-text-dim">{error.message}</p>
          <div className="mt-4 flex justify-end gap-2">
            <button
              type="button"
              className="rounded border border-bb-border px-3 py-1.5 text-xs hover:bg-bb-hover"
              onClick={() => window.location.reload()}
            >
              {i18n.t('errors.reload')}
            </button>
            <button
              type="button"
              className="rounded bg-bb-accent px-3 py-1.5 text-xs font-medium text-bb-on-accent hover:bg-bb-accent-hover"
              onClick={() => openFeedbackReport({
                kind: 'crash',
                title: i18n.t('feedback.ui_error_title'),
                description: error.message,
                sourceContext: {
                  source: 'react_error_boundary',
                  error_message: error.message,
                  stack,
                  feature: 'react',
                  correlation_ts: new Date().toISOString(),
                },
              })}
            >
              {i18n.t('notifications.report')}
            </button>
          </div>
        </div>
      </div>
    );
  }
}
