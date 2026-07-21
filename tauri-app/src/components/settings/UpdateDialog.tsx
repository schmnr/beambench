import { useEffect, useRef } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import { useMachineStore } from '../../stores/machineStore';
import { useUpdateStore } from '../../stores/updateStore';
import { getUpdateInstallBlocker } from '../../services/updateService';

function formatBytes(value: number): string {
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`;
  return `${(value / (1024 * 1024)).toFixed(1)} MB`;
}

export function UpdateDialog() {
  const { t, i18n } = useTranslation();
  const overlayRef = useRef<HTMLDivElement>(null);
  const update = useUpdateStore((s) => s.availableUpdate);
  const status = useUpdateStore((s) => s.status);
  const progress = useUpdateStore((s) => s.progress);
  const error = useUpdateStore((s) => s.error);
  const closeDialog = useUpdateStore((s) => s.closeDialog);
  const installAvailableUpdate = useUpdateStore((s) => s.installAvailableUpdate);
  const snoozeAvailableUpdate = useUpdateStore((s) => s.snoozeAvailableUpdate);
  const skipAvailableUpdate = useUpdateStore((s) => s.skipAvailableUpdate);

  useMachineStore((s) => s.sessionState);
  useMachineStore((s) => s.machineStatus?.run_state);
  useMachineStore((s) => s.jobProgress?.state);
  const installBlocker = getUpdateInstallBlocker();
  const isBusy = status === 'checking' || status === 'downloading' || status === 'installing' || status === 'relaunching';
  const canInstall = Boolean(update) && !installBlocker && !isBusy;
  const percent = progress?.percent ?? null;

  useEffect(() => {
    overlayRef.current?.focus();
  }, []);

  if (!update) return null;

  return createPortal(
    <div
      ref={overlayRef}
      role="dialog"
      aria-modal="true"
      aria-labelledby="update-dialog-title"
      tabIndex={-1}
      className="fixed inset-0 z-[9000] flex items-center justify-center bg-black/50"
      onKeyDown={(e) => {
        if (e.key === 'Escape' && !isBusy) closeDialog();
      }}
      onClick={(e) => {
        if (e.target === e.currentTarget && !isBusy) closeDialog();
      }}
    >
      <div className="w-[460px] max-w-[calc(100vw-2rem)] rounded-lg border border-bb-border bg-bb-panel p-6 shadow-xl">
        <h2 id="update-dialog-title" className="mb-2 text-lg font-semibold text-bb-text">
          {t('dialog.update.title_template', { version: update.version })}
        </h2>
        <div className="mb-4 text-sm text-bb-text-muted">
          {t('dialog.update.installed', { version: update.currentVersion })}
          {update.date ? <span className="ml-2">{t('dialog.update.published', { date: new Date(update.date).toLocaleDateString(i18n.language) })}</span> : null}
        </div>

        {update.body ? (
          <div className="mb-4 max-h-36 overflow-y-auto whitespace-pre-wrap rounded border border-bb-border bg-bb-surface p-3 text-sm text-bb-text">
            {update.body}
          </div>
        ) : (
          <div className="mb-4 text-sm text-bb-text-muted">{t('dialog.update.no_notes')}</div>
        )}

        {progress ? (
          <div className="mb-4 space-y-2">
            <div className="h-2 overflow-hidden rounded bg-bb-border">
              <div
                className="h-full bg-bb-accent transition-[width]"
                style={{ width: `${percent ?? 0}%` }}
              />
            </div>
            <div className="text-xs text-bb-text-muted">
              {progress.phase === 'finished'
                ? t('dialog.update.download_complete')
                : progress.totalBytes
                  ? t('dialog.update.progress_with_total', { downloaded: formatBytes(progress.downloadedBytes), total: formatBytes(progress.totalBytes) })
                  : formatBytes(progress.downloadedBytes)}
            </div>
          </div>
        ) : null}

        {installBlocker ? (
          <div role="status" className="mb-4 rounded border border-bb-warning-border bg-bb-warning-bg p-3 text-sm text-bb-warning-fg">
            {installBlocker}
          </div>
        ) : null}

        {error ? (
          <div role="alert" className="mb-4 rounded border border-bb-error-border bg-bb-error-bg p-3 text-sm text-bb-error-fg">
            {error}
          </div>
        ) : null}

        <div className="flex flex-wrap justify-end gap-2">
          <button
            onClick={() => void snoozeAvailableUpdate()}
            disabled={isBusy}
            className="rounded border border-bb-border bg-bb-surface px-3 py-1.5 text-sm text-bb-text-muted hover:bg-bb-hover disabled:opacity-50"
          >
            {t('dialog.update.not_now')}
          </button>
          <button
            onClick={() => void skipAvailableUpdate()}
            disabled={isBusy}
            className="rounded border border-bb-border bg-bb-surface px-3 py-1.5 text-sm text-bb-text-muted hover:bg-bb-hover disabled:opacity-50"
          >
            {t('dialog.update.skip_version')}
          </button>
          <button
            onClick={() => void installAvailableUpdate()}
            disabled={!canInstall}
            className="rounded bg-bb-accent px-4 py-1.5 text-sm font-medium text-bb-on-accent hover:bg-bb-accent-hover disabled:opacity-50"
          >
            {status === 'downloading' ? t('dialog.update.downloading') : status === 'installing' ? t('dialog.update.installing') : t('dialog.update.install_button')}
          </button>
        </div>
      </div>
    </div>,
    document.body,
  );
}
