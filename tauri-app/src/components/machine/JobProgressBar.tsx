import { useTranslation } from 'react-i18next';
import { useMachineStore } from '../../stores/machineStore';
import { JOB_STATE_TEXT_CLASSES } from './stateColors';

function formatTime(secs: number): string {
  if (secs <= 0) return '0:00';

  const totalSeconds = Math.round(secs);

  if (totalSeconds < 3600) {
    const minutes = Math.floor(totalSeconds / 60);
    const seconds = totalSeconds % 60;
    return `${minutes}:${seconds.toString().padStart(2, '0')}`;
  }

  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  return `${hours}:${minutes.toString().padStart(2, '0')}:${seconds.toString().padStart(2, '0')}`;
}

export function JobProgressBar() {
  const { t } = useTranslation();
  const jobProgress = useMachineStore((s) => s.jobProgress);

  if (!jobProgress) return null;

  const progressPercent = jobProgress.total_lines > 0
    ? (jobProgress.acknowledged_lines / jobProgress.total_lines) * 100
    : 0;

  const stateClass = JOB_STATE_TEXT_CLASSES[jobProgress.state] ?? 'text-bb-text-muted';
  const isFinishing = jobProgress.state === 'running' && progressPercent >= 99.9;

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between">
        <span className={`text-xs font-medium ${stateClass}`}>
          {t(`panels.machine.job_progress.state.${jobProgress.state}`)}
        </span>
        <span className="text-xs text-bb-text-muted">
          {t('panels.machine.job_progress.lines', {
            acknowledged: jobProgress.acknowledged_lines,
            total: jobProgress.total_lines,
          })}
        </span>
      </div>

      <div className="h-1.5 bg-bb-surface rounded-full overflow-hidden">
        <div
          className={`h-full bg-bb-accent rounded-full transition-all ${isFinishing ? 'animate-pulse' : ''}`}
          style={{ width: `${progressPercent}%` }}
        />
      </div>

      {isFinishing && (
        <div className="text-xs text-bb-text-muted">
          {t('panels.machine.job_progress.finishing')}
        </div>
      )}

      {jobProgress.state === 'failed' && jobProgress.error_message && (
        <div className="text-xs text-bb-error-fg">
          {jobProgress.error_message}
        </div>
      )}

      <div className="flex items-center justify-between text-xs text-bb-text-muted font-mono">
        <span>{t('panels.machine.job_progress.elapsed', { time: formatTime(jobProgress.elapsed_secs) })}</span>
        <span>
          {t('panels.machine.job_progress.remaining', {
            time: formatTime(jobProgress.estimated_remaining_secs),
          })}
        </span>
      </div>
    </div>
  );
}
