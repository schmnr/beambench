import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import { useMachineStore } from '../../stores/machineStore';
import { useUpdateStore } from '../../stores/updateStore';
import { getUpdateInstallBlocker } from '../../services/updateService';
import { Download } from 'lucide-react';
import { MovableResizableDialogFrame } from '../shared/MovableResizableDialogFrame';
import { DIALOG_TONE, DialogButton, DialogFooter, DialogNotice, DialogSectionHeader } from '../shared/DialogPrimitives';

function formatBytes(value: number): string {
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`;
  return `${(value / (1024 * 1024)).toFixed(1)} MB`;
}

export function UpdateDialog() {
  const { t, i18n } = useTranslation();
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

  if (!update) return null;

  return createPortal(
    <MovableResizableDialogFrame
      title={t('dialog.update.title_template', { version: update.version })}
      titleId="update-dialog-title"
      testId="update-dialog"
      initialWidth={520}
      initialHeight={520}
      minWidth={440}
      minHeight={420}
      onRequestClose={isBusy ? undefined : closeDialog}
      closeOnBackdropClick={!isBusy}
      zIndexClassName="z-[9000]"
      headerActions={(
        <span className="rounded-full border border-bb-border bg-bb-bg/70 px-2 py-0.5 text-[11px] text-bb-text-muted">
          {t('dialog.update.installed', { version: update.currentVersion })}
        </span>
      )}
      footer={(
        <DialogFooter>
          <DialogButton tone={DIALOG_TONE.quiet} onClick={() => void snoozeAvailableUpdate()} disabled={isBusy}>{t('dialog.update.not_now')}</DialogButton>
          <DialogButton tone={DIALOG_TONE.secondary} onClick={() => void skipAvailableUpdate()} disabled={isBusy}>{t('dialog.update.skip_version')}</DialogButton>
          <DialogButton tone={DIALOG_TONE.primary} icon={<Download size={13} />} onClick={() => void installAvailableUpdate()} disabled={!canInstall}>
            {status === 'downloading' ? t('dialog.update.downloading') : status === 'installing' ? t('dialog.update.installing') : t('dialog.update.install_button')}
          </DialogButton>
        </DialogFooter>
      )}
    >
      <div className="min-h-0 flex-1 overflow-y-auto bg-bb-bg/20">
        <DialogSectionHeader
          icon={<Download size={14} />}
          title={t('dialog.update.title_template', { version: update.version })}
          description={update.date ? t('dialog.update.published', { date: new Date(update.date).toLocaleDateString(i18n.language) }) : undefined}
        />
        <div className="space-y-4 p-4">

        {update.body ? (
          <div className="max-h-48 overflow-y-auto whitespace-pre-wrap rounded-xl border border-bb-border bg-bb-panel p-3 text-sm leading-6 text-bb-text">
            {update.body}
          </div>
        ) : (
          <div className="text-sm text-bb-text-muted">{t('dialog.update.no_notes')}</div>
        )}

        {progress ? (
          <div className="space-y-2 rounded-xl border border-bb-border bg-bb-panel p-3">
            <div className="h-2 overflow-hidden rounded-full bg-bb-border">
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
          <DialogNotice tone={DIALOG_TONE.warning}>{installBlocker}</DialogNotice>
        ) : null}

        {error ? (
          <DialogNotice tone={DIALOG_TONE.error} role="alert">{error}</DialogNotice>
        ) : null}

        </div>
      </div>
    </MovableResizableDialogFrame>,
    document.body,
  );
}
