import { createPortal } from 'react-dom';
import { useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import type { RecoveryInfo } from '../../services/persistenceService';

interface RecoveryDialogProps {
  recoveries: RecoveryInfo[];
  onRestore: (path: string) => void;
  onDiscard: (path: string) => void;
  onDiscardAll: () => void;
  onClose: () => void;
}

export function RecoveryDialog({
  recoveries,
  onRestore,
  onDiscard,
  onDiscardAll,
  onClose,
}: RecoveryDialogProps) {
  const { t } = useTranslation();
  useEffect(() => {
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', handleKey);
    return () => window.removeEventListener('keydown', handleKey);
  }, [onClose]);

  return createPortal(
    <div
      className="fixed inset-0 z-[9999] flex items-center justify-center bg-black/50"
      onClick={onClose}
    >
      <div
        className="bg-bb-panel rounded-lg border border-bb-border p-4 w-[400px] max-h-[60vh] overflow-y-auto"
        onClick={(e) => e.stopPropagation()}
      >
        <h2 className="text-sm font-semibold text-bb-text mb-2">
          {t('dialog.recovery.title')}
        </h2>
        <p className="text-xs text-bb-text-muted mb-3">
          {t('dialog.recovery.description')}
        </p>

        <div className="flex flex-col gap-2 mb-3">
          {recoveries.map((r) => (
            <div
              key={r.path}
              className="flex items-center justify-between bg-bb-hover rounded px-2 py-1.5"
            >
              <span className="text-xs text-bb-text truncate mr-2">
                {r.project_name}
              </span>
              <div className="flex gap-1 shrink-0">
                <button
                  className="text-xs px-2 py-0.5 rounded bg-bb-accent text-bb-on-accent hover:bg-bb-accent-hover"
                  onClick={() => onRestore(r.path)}
                >
                  {t('dialog.recovery.restore')}
                </button>
                <button
                  className="text-xs px-2 py-0.5 rounded bg-bb-hover text-bb-text-muted hover:bg-bb-border hover:text-bb-text"
                  onClick={() => onDiscard(r.path)}
                >
                  {t('dialog.recovery.discard')}
                </button>
              </div>
            </div>
          ))}
        </div>

        <div className="flex justify-between">
          <button
            className="text-xs px-3 py-1 rounded bg-bb-hover text-bb-text-muted hover:bg-bb-border hover:text-bb-text"
            onClick={onDiscardAll}
          >
            {t('dialog.recovery.discard_all')}
          </button>
          <button
            className="text-xs px-3 py-1 rounded bg-bb-hover text-bb-text hover:bg-bb-border"
            onClick={onClose}
          >
            {t('common.close')}
          </button>
        </div>
      </div>
    </div>,
    document.body,
  );
}
