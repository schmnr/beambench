import { useEffect } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import type { PreflightReport } from '../../types/machine';

interface PreflightDialogProps {
  report: PreflightReport;
  onClose: () => void;
  onApplyOverscan?: (value: number) => void;
  onReduceSpeed?: (value: number) => void;
  onContinue?: () => void;
  busy?: boolean;
}

export function PreflightDialog({
  report,
  onClose,
  onApplyOverscan,
  onReduceSpeed,
  onContinue,
  busy = false,
}: PreflightDialogProps) {
  const { t } = useTranslation();
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && !busy) {
        onClose();
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [busy, onClose]);

  const handleOverlayClick = (e: React.MouseEvent<HTMLDivElement>) => {
    if (!busy && e.target === e.currentTarget) {
      onClose();
    }
  };

  const overscanAdvisory = report.advisories?.find((item) => item.code === 'raster_overscan');
  const recommendedSpeed = report.advisories
    ?.map((item) => item.recommended_speed_mm_min)
    .filter((value): value is number => value != null)
    .reduce((minimum, value) => Math.min(minimum, value), Number.POSITIVE_INFINITY);
  const localizeCategory = (category: string) =>
    t(`dialog.preflight.categories.${category}`, { defaultValue: category });
  const localizeDescription = (description: string) => {
    const keyByDescription: Record<string, string> = {
      'Session is in Ready state': 'session_ready',
      'Machine is idle': 'machine_idle',
      'No active alarm': 'no_alarm',
      'Plan has segments': 'plan_segments',
      'Plan fits within machine bed': 'plan_fits',
      'Raster motion (overscan and scanning offset) fits within machine bed': 'raster_motion_fits',
      'Laser mode enabled ($32=1)': 'laser_mode',
      'Homing is enabled': 'homing_enabled',
      'Relative placement was framed at the current position': 'relative_framed',
    };
    const key = keyByDescription[description];
    return key ? t(`dialog.preflight.checks.${key}`, { defaultValue: description }) : description;
  };
  const localizeMessage = (message: string) => {
    const keyByMessage: Record<string, string> = {
      'Connected and ready': 'connected_ready',
      'Machine idle': 'machine_idle',
      'No alarm': 'no_alarm',
      'Raster motion within bed': 'raster_motion_within_bed',
      'Laser mode enabled': 'laser_mode_enabled',
      'Homing enabled': 'homing_enabled',
      'Frame completed for this exact project and placement': 'frame_completed',
      'This controller is not homed, so Beam Bench cannot verify physical bed edges. Frame the job after positioning the laser, then recheck before starting.':
        'frame_required',
    };
    const segmentMatch = message.match(/^(\d+) segments$/);
    if (segmentMatch) {
      return t('dialog.preflight.messages.segment_count', { count: Number(segmentMatch[1]) });
    }
    const key = keyByMessage[message];
    return key ? t(`dialog.preflight.messages.${key}`, { defaultValue: message }) : message;
  };

  const getOutcomeBadge = () => {
    switch (report.outcome) {
      case 'pass':
        return (
          <span className="rounded-full px-2 py-0.5 text-xs font-medium bg-bb-success text-bb-on-success">
            {t('dialog.preflight.outcome_pass')}
          </span>
        );
      case 'pass_with_warnings':
        return (
          <span className="rounded-full px-2 py-0.5 text-xs font-medium bg-bb-warning text-bb-on-warning">
            {t('dialog.preflight.outcome_warnings')}
          </span>
        );
      case 'fail':
        return (
          <span className="rounded-full px-2 py-0.5 text-xs font-medium bg-bb-error text-bb-on-error">
            {t('dialog.preflight.outcome_fail')}
          </span>
        );
    }
  };

  return createPortal(
    <div
      role="dialog"
      aria-modal="true"
      aria-labelledby="dialog-title"
      className="fixed inset-0 bg-black/50 flex items-center justify-center z-50"
      onClick={handleOverlayClick}
    >
      <div className="bg-bb-panel border border-bb-border rounded-lg shadow-xl p-4 min-w-[320px] max-w-[480px] max-h-[60vh] flex flex-col">
        <div className="flex items-center gap-2 mb-3">
          <h2 id="dialog-title" className="text-sm font-semibold text-bb-text">
            {t('dialog.preflight.title')}
          </h2>
          {getOutcomeBadge()}
        </div>

        <div className="overflow-y-auto flex-1 space-y-2">
          {report.checks.map((check, index) => (
            <div key={index} className="flex items-start gap-2">
              <span className={check.passed ? 'text-bb-success-fg' : 'text-bb-error-fg'}>
                {/* eslint-disable-next-line i18next/no-literal-string */}
                {check.passed ? '✓' : '✗'}
              </span>
              <div className="flex-1">
                <div className="text-xs text-bb-text-muted">{localizeCategory(check.category)}</div>
                <div className="text-sm text-bb-text">{localizeDescription(check.description)}</div>
                {check.message && (
                  <div className="text-xs text-bb-text-dim italic mt-0.5">
                    {localizeMessage(check.message)}
                  </div>
                )}
              </div>
            </div>
          ))}
          {report.advisories?.map((advisory) => (
            <div
              key={advisory.code}
              className="flex items-start gap-2 rounded border border-bb-warning-border bg-bb-warning-bg px-2 py-1.5"
            >
              <span className="text-bb-warning-fg" aria-hidden="true">
                !
              </span>
              <div className="flex-1">
                <div className="text-xs font-medium text-bb-warning-fg">
                  {t(`dialog.preflight.advisories.${advisory.code}.title`, {
                    defaultValue: advisory.description,
                  })}
                </div>
                <div className="mt-0.5 text-xs text-bb-text-muted">
                  {t(`dialog.preflight.advisories.${advisory.code}.message`, {
                    defaultValue: advisory.message,
                  })}
                </div>
              </div>
            </div>
          ))}
        </div>

        {report.outcome === 'pass_with_warnings' && (
          <div className="mt-3 text-xs text-bb-warning-fg">
            {t('dialog.preflight.advisory_choice')}
          </div>
        )}

        <div className="mt-4 flex flex-wrap justify-end gap-2">
          {report.outcome === 'pass_with_warnings' &&
            overscanAdvisory?.recommended_overscan_mm != null &&
            onApplyOverscan && (
              <button
                type="button"
                disabled={busy}
                onClick={() => onApplyOverscan(overscanAdvisory.recommended_overscan_mm!)}
                className="rounded border border-bb-border bg-bb-surface-2 px-3 py-1.5 text-xs font-medium text-bb-text hover:bg-bb-hover disabled:cursor-wait disabled:opacity-60"
              >
                {t('dialog.preflight.apply_overscan', {
                  value: overscanAdvisory.recommended_overscan_mm.toFixed(1),
                })}
              </button>
            )}
          {report.outcome === 'pass_with_warnings' &&
            recommendedSpeed != null &&
            Number.isFinite(recommendedSpeed) &&
            onReduceSpeed && (
              <button
                type="button"
                disabled={busy}
                onClick={() => onReduceSpeed(recommendedSpeed)}
                className="rounded border border-bb-border bg-bb-surface-2 px-3 py-1.5 text-xs font-medium text-bb-text hover:bg-bb-hover disabled:cursor-wait disabled:opacity-60"
              >
                {t('dialog.preflight.reduce_speed', {
                  value: Math.round(recommendedSpeed),
                })}
              </button>
            )}
          {report.outcome === 'pass_with_warnings' && onContinue && (
            <button
              type="button"
              disabled={busy}
              onClick={onContinue}
              className="rounded border border-bb-warning bg-transparent px-3 py-1.5 text-xs font-medium text-bb-warning-fg hover:bg-bb-warning-bg disabled:cursor-wait disabled:opacity-60"
            >
              {t('dialog.preflight.continue_anyway')}
            </button>
          )}
          <button
            type="button"
            disabled={busy}
            onClick={onClose}
            className="px-3 py-1.5 text-xs font-medium rounded bg-bb-accent hover:bg-bb-accent-hover text-bb-on-accent transition-colors disabled:cursor-wait disabled:opacity-60"
          >
            {t('common.close')}
          </button>
        </div>
      </div>
    </div>,
    document.body,
  );
}
