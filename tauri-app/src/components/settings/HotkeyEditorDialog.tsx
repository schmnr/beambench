import { useCallback, useEffect, useMemo, useState } from 'react';
import { wrapBackendError } from '../../i18n/errors';
import { useTranslation } from 'react-i18next';
import {
  getCommandMetadata,
  getEffectiveHotkey,
  hotkeyConflictsWithCommand,
  isReservedHotkey,
  type CommandMetadata,
  type CustomHotkeys,
} from '../../commands/commandRegistry';
import { hotkeyFromKeyboardEvent, normalizeHotkey } from '../../utils/hotkeyMatch';
import { useAppStore } from '../../stores/appStore';
import { useMacroStore } from '../../stores/macroStore';
import { useNotificationStore } from '../../stores/notificationStore';
import { MovableResizableDialogFrame } from '../shared/MovableResizableDialogFrame';

interface HotkeyEditorDialogProps {
  onClose: () => void;
}

export function HotkeyEditorDialog({ onClose }: HotkeyEditorDialogProps) {
  const { t } = useTranslation();
  const settings = useAppStore((s) => s.settings);
  const updateSettings = useAppStore((s) => s.updateSettings);
  const macros = useMacroStore((s) => s.macros);
  const push = useNotificationStore((s) => s.push);
  const [draft, setDraft] = useState<CustomHotkeys>(settings?.custom_hotkeys ?? {});
  const [query, setQuery] = useState('');
  const [capturingId, setCapturingId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const translateCommandLabel = useCallback((command: CommandMetadata) => (
    command.labelKey ? t(command.labelKey, { defaultValue: command.label }) : command.label
  ), [t]);

  const translateCommandGroup = useCallback((command: CommandMetadata) => (
    command.groupKey ? t(command.groupKey, { defaultValue: command.group }) : command.group
  ), [t]);

  useEffect(() => {
    setDraft(settings?.custom_hotkeys ?? {});
  }, [settings?.custom_hotkeys]);

  const commands = useMemo(() => {
    const q = query.trim().toLowerCase();
    return getCommandMetadata().map((command) => ({
      ...command,
      displayLabel: translateCommandLabel(command),
      displayGroup: translateCommandGroup(command),
    })).filter((command) => {
      if (!q) return true;
      return command.displayLabel.toLowerCase().includes(q)
        || command.displayGroup.toLowerCase().includes(q)
        || command.id.toLowerCase().includes(q);
    });
  }, [query, translateCommandGroup, translateCommandLabel]);

  useEffect(() => {
    if (!capturingId) return undefined;
    const onKeyDown = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopPropagation();
      const hotkey = hotkeyFromKeyboardEvent(event);
      if (!hotkey) {
        setError(t('dialog.hotkey_editor.error_incomplete'));
        return;
      }
      if (isReservedHotkey(hotkey)) {
        setError(t('dialog.hotkey_editor.error_reserved', { hotkey }));
        return;
      }
      const nextDraft = { ...draft, [capturingId]: hotkey };
      const commandConflict = hotkeyConflictsWithCommand(capturingId, hotkey, nextDraft);
      if (commandConflict) {
        setError(t('dialog.hotkey_editor.error_command_conflict', { hotkey, command: translateCommandLabel(commandConflict) }));
        return;
      }
      const macroConflict = macros.find((macro) => normalizeHotkey(macro.hotkey) === hotkey);
      if (macroConflict) {
        setError(t('dialog.hotkey_editor.error_macro_conflict', { hotkey, macro: macroConflict.name }));
        return;
      }
      setDraft(nextDraft);
      setCapturingId(null);
      setError(null);
    };
    window.addEventListener('keydown', onKeyDown, true);
    return () => window.removeEventListener('keydown', onKeyDown, true);
  }, [capturingId, draft, macros, t, translateCommandLabel]);

  const clearHotkey = (commandId: string) => {
    setDraft((current) => {
      const next = { ...current };
      delete next[commandId];
      return next;
    });
    setError(null);
  };

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

  const footer = (
    <div className="flex justify-between gap-2 px-5 py-3">
      <button
        type="button"
        className="rounded-md border border-bb-border px-3 py-1.5 text-sm text-bb-text hover:bg-bb-surface disabled:cursor-not-allowed disabled:opacity-50"
        disabled={busy || Object.keys(draft).length === 0}
        onClick={() => {
          setDraft({});
          setError(null);
        }}
      >
        {t('dialog.hotkey_editor.reset_all')}
      </button>
      <div className="flex gap-2">
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
    </div>
  );

  return (
    <MovableResizableDialogFrame
      title={t('dialog.hotkey_editor.title')}
      titleId="hotkey-editor-title"
      testId="hotkey-editor-dialog"
      initialWidth={760}
      initialHeight={620}
      minWidth={620}
      minHeight={420}
      onRequestClose={onClose}
      closeOnBackdropClick
      footer={footer}
    >
        <div className="flex min-h-0 flex-1 flex-col gap-3 px-5 py-4">
          <input
            className="h-9 rounded-md border border-bb-border bg-bb-surface px-3 text-sm text-bb-text outline-none focus:border-bb-accent"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder={t('dialog.hotkey_editor.search')}
            autoFocus
          />
          {error && (
            <div className="rounded-md border border-bb-error-border bg-bb-error-bg px-3 py-2 text-sm text-bb-error-fg">
              {error}
            </div>
          )}
          <div className="min-h-0 flex-1 overflow-auto rounded-md border border-bb-border">
            {commands.map((command) => {
              const effectiveHotkey = getEffectiveHotkey(command.id, draft);
              const defaultHotkey = normalizeHotkey(command.defaultHotkey);
              const overridden = Boolean(draft[command.id]);
              return (
                <div
                  key={command.id}
                  className="grid grid-cols-[1fr_130px_210px] items-center gap-3 border-b border-bb-border px-3 py-2 text-sm last:border-b-0"
                >
                  <div className="min-w-0">
                    <div className="truncate text-bb-text">{command.displayLabel}</div>
                    <div className="truncate text-xs text-bb-text-muted">{command.displayGroup}</div>
                  </div>
                  <div className="text-xs text-bb-text-muted">
                    {effectiveHotkey ?? t('dialog.hotkey_editor.none')}
                    {overridden && <span className="ml-1 text-bb-accent">{t('dialog.hotkey_editor.custom')}</span>}
                  </div>
                  <div className="flex items-center justify-end gap-2">
                    <button
                      type="button"
                      className="rounded-md border border-bb-border px-2 py-1 text-xs text-bb-text hover:bg-bb-surface disabled:cursor-not-allowed disabled:opacity-50"
                      disabled={!command.editable || busy}
                      onClick={() => {
                        setCapturingId(command.id);
                        setError(t('dialog.hotkey_editor.prompt', { command: command.displayLabel }));
                      }}
                    >
                      {capturingId === command.id ? t('dialog.hotkey_editor.press_keys') : t('dialog.hotkey_editor.assign')}
                    </button>
                    <button
                      type="button"
                      className="rounded-md border border-bb-border px-2 py-1 text-xs text-bb-text hover:bg-bb-surface disabled:cursor-not-allowed disabled:opacity-50"
                      disabled={!command.editable || !overridden || busy}
                      onClick={() => clearHotkey(command.id)}
                    >
                      {t('dialog.hotkey_editor.clear')}
                    </button>
                    <span className="w-16 text-right text-xs text-bb-text-muted">
                      {defaultHotkey ?? ''}
                    </span>
                  </div>
                </div>
              );
            })}
          </div>
        </div>
    </MovableResizableDialogFrame>
  );
}
