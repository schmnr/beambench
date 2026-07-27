import { CheckCircle2, MessageSquareWarning, X } from 'lucide-react';
import { useTranslation } from 'react-i18next';

interface PostJobCompatibilityDialogProps {
  profileName?: string;
  onCompleted: () => void;
  onProblem: () => void;
  onNotNow: () => void;
}

export function PostJobCompatibilityDialog({
  profileName,
  onCompleted,
  onProblem,
  onNotNow,
}: PostJobCompatibilityDialogProps) {
  const { t } = useTranslation();

  return (
    <div className="fixed inset-0 z-[9750] flex items-center justify-center bg-bb-bg/70 px-4">
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="post-job-compatibility-title"
        className="w-[560px] max-w-full rounded-lg border border-bb-border bg-bb-panel shadow-2xl"
      >
        <div className="flex items-center justify-between border-b border-bb-border px-5 py-3">
          <h2 id="post-job-compatibility-title" className="text-sm font-semibold text-bb-text">
            {t('feedback.post_job_title')}
          </h2>
          <button
            type="button"
            aria-label={t('feedback.post_job_not_now')}
            onClick={onNotNow}
            className="rounded p-1 text-bb-text-dim hover:bg-bb-hover"
          >
            <X size={16} />
          </button>
        </div>

        <div className="space-y-4 px-5 py-4 text-sm text-bb-text">
          <p>{t('feedback.post_job_question')}</p>
          <p className="text-bb-text-dim">
            {profileName
              ? t('feedback.post_job_profile_help', { profile: profileName })
              : t('feedback.post_job_help')}
          </p>
          <div className="rounded border border-bb-border bg-bb-bg/60 p-3 text-xs text-bb-text-dim">
            {t('feedback.post_job_privacy')}
          </div>
        </div>

        <div className="flex flex-wrap justify-end gap-2 border-t border-bb-border px-5 py-3">
          <button
            type="button"
            onClick={onNotNow}
            className="rounded border border-bb-border px-3 py-1.5 text-xs text-bb-text hover:bg-bb-hover"
          >
            {t('feedback.post_job_not_now')}
          </button>
          <button
            type="button"
            onClick={onProblem}
            className="inline-flex items-center gap-1 rounded border border-bb-border px-3 py-1.5 text-xs text-bb-text hover:bg-bb-hover"
          >
            <MessageSquareWarning size={14} /> {t('feedback.post_job_problem')}
          </button>
          <button
            type="button"
            onClick={onCompleted}
            className="inline-flex items-center gap-1 rounded bg-bb-accent px-3 py-1.5 text-xs font-medium text-bb-on-accent hover:bg-bb-accent-hover"
          >
            <CheckCircle2 size={14} /> {t('feedback.post_job_completed')}
          </button>
        </div>
      </div>
    </div>
  );
}
