import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { wrapBackendError } from '../../i18n/errors';
import type { CustomHotkeys } from '../../commands/commandRegistry';
import { useAppStore } from '../../stores/appStore';
import { useNotificationStore } from '../../stores/notificationStore';
import { MovableResizableDialogFrame } from '../shared/MovableResizableDialogFrame';
import { HotkeySettingsPanel } from './HotkeySettingsPanel';

interface HotkeyEditorDialogProps {
  onClose: () => void;
}

export function HotkeyEditorDialog({ onClose }: HotkeyEditorDialogProps) {
  const { t } = useTranslation();
  const settings = useAppStore((state) => state.settings);
  const updateSettings = useAppStore((state) => state.updateSettings);
  const push = useNotificationStore((state) => state.push);
  const [draft, setDraft] = useState<CustomHotkeys>(settings?.custom_hotkeys ?? {});
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    setDraft(settings?.custom_hotkeys ?? {});
  }, [settings?.custom_hotkeys]);

  const save = async () => {
    setBusy(true);
    try {
      await updateSettings({ custom_hotkeys: draft });
      push(t('dialog.hotkey_editor.saved'), 'success');
      onClose();
    } catch (saveError) {
      push(wrapBackendError(String(saveError)), 'error');
    } finally {
      setBusy(false);
    }
  };

  return (
    <MovableResizableDialogFrame
      title={t('dialog.hotkey_editor.title')}
      titleId="hotkey-editor-title"
      testId="hotkey-editor-dialog"
      initialWidth={820}
      initialHeight={650}
      minWidth={700}
      minHeight={460}
      onRequestClose={onClose}
      closeOnBackdropClick
      footer={(
        <div className="flex justify-end gap-2 px-5 py-3">
          <button
            type="button"
            className="rounded-md border border-bb-border px-3 py-1.5 text-sm text-bb-text hover:bg-bb-surface"
            onClick={onClose}
            disabled={busy}
          >
            {t('common.cancel')}
          </button>
          <button
            type="button"
            className="rounded-md bg-bb-accent px-3 py-1.5 text-sm font-medium text-bb-on-accent hover:bg-bb-accent-hover disabled:cursor-not-allowed disabled:opacity-50"
            onClick={() => { void save(); }}
            disabled={busy}
          >
            {busy ? t('dialog.hotkey_editor.saving') : t('common.save')}
          </button>
        </div>
      )}
    >
      <div className="flex min-h-0 flex-1 px-5 py-4">
        <HotkeySettingsPanel value={draft} onChange={setDraft} disabled={busy} />
      </div>
    </MovableResizableDialogFrame>
  );
}
