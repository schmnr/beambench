import { useEffect } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import type { PreflightReport } from '../../types/machine';

interface PreflightDialogProps {
  report: PreflightReport;
  onClose: () => void;
}

export function PreflightDialog({ report, onClose }: PreflightDialogProps) {
  const { t } = useTranslation();
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        onClose();
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [onClose]);

  const handleOverlayClick = (e: React.MouseEvent<HTMLDivElement>) => {
    if (e.target === e.currentTarget) {
      onClose();
    }
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
          <h2 id="dialog-title" className="text-sm font-semibold text-bb-text">{t('dialog.preflight.title')}</h2>
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
                <div className="text-xs text-bb-text-muted">{check.category}</div>
                <div className="text-sm text-bb-text">{check.description}</div>
                {check.message && (
                  <div className="text-xs text-bb-text-dim italic mt-0.5">
                    {check.message}
                  </div>
                )}
              </div>
            </div>
          ))}
        </div>

        {report.outcome === 'pass_with_warnings' && (
          <div className="mt-3 text-xs text-bb-warning-fg">
            {t('dialog.preflight.warnings_block_start')}
          </div>
        )}

        <div className="flex justify-end mt-4">
          <button
            onClick={onClose}
            className="px-3 py-1.5 text-xs font-medium rounded bg-bb-accent hover:bg-bb-accent-hover text-bb-on-accent transition-colors"
          >
            {t('common.close')}
          </button>
        </div>
      </div>
    </div>,
    document.body
  );
}
