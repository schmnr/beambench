import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { MovableResizableDialogFrame } from '../shared/MovableResizableDialogFrame';
import { useProjectStore } from '../../stores/projectStore';
import { useUnsavedGuardStore } from '../../stores/unsavedGuardStore';

/**
 * Save / Don't Save / Cancel prompt shown before any action that would
 * discard unsaved project changes (quit, New, Open). The pending action is
 * parked in unsavedGuardStore and runs after a save or an explicit discard.
 */
export function UnsavedChangesDialog() {
  const { t } = useTranslation();
  const pendingAction = useUnsavedGuardStore((s) => s.pendingAction);
  const clear = useUnsavedGuardStore((s) => s.clear);
  const [busy, setBusy] = useState(false);

  if (!pendingAction) return null;

  const handleCancel = () => {
    if (busy) return;
    clear();
  };

  const handleDiscard = () => {
    if (busy) return;
    const { execute } = pendingAction;
    clear();
    void execute();
  };

  const handleSave = async () => {
    if (busy) return;
    setBusy(true);
    try {
      await useProjectStore.getState().saveProject();
      // Save As can be cancelled, in which case the project stays dirty and
      // the user keeps the dialog (their pending action must not run).
      const stillDirty = useProjectStore.getState().project?.dirty ?? false;
      if (stillDirty) return;
      const { execute } = pendingAction;
      clear();
      await execute();
    } finally {
      setBusy(false);
    }
  };

  return (
    <MovableResizableDialogFrame
      title={t('dialog.unsaved_changes.title')}
      titleId="unsaved-changes-title"
      testId="unsaved-changes-dialog"
      initialWidth={400}
      initialHeight={180}
      minWidth={340}
      minHeight={160}
      onRequestClose={handleCancel}
      footer={(
        <div className="flex justify-end gap-2 px-4 py-3">
          <button
            type="button"
            data-testid="unsaved-changes-cancel"
            onClick={handleCancel}
            disabled={busy}
            className="rounded border border-bb-border px-3 py-1.5 text-xs font-medium text-bb-text hover:bg-bb-hover disabled:opacity-50"
          >
            {t('common.cancel')}
          </button>
          <button
            type="button"
            data-testid="unsaved-changes-discard"
            onClick={handleDiscard}
            disabled={busy}
            className="rounded border border-bb-border px-3 py-1.5 text-xs font-medium text-bb-error-fg hover:bg-bb-hover disabled:opacity-50"
          >
            {t('dialog.unsaved_changes.discard')}
          </button>
          <button
            type="button"
            data-testid="unsaved-changes-save"
            onClick={() => { void handleSave(); }}
            disabled={busy}
            className="rounded bg-bb-accent px-3 py-1.5 text-xs font-semibold text-bb-on-accent hover:bg-bb-accent-hover disabled:opacity-50"
          >
            {t('common.save')}
          </button>
        </div>
      )}
    >
      <div className="flex flex-1 items-center px-5 py-4 text-sm text-bb-text">
        <p>{t('dialog.unsaved_changes.message')}</p>
      </div>
    </MovableResizableDialogFrame>
  );
}
