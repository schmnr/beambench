import { useCallback, useEffect, useMemo, useState } from 'react';
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
import { useMacroStore } from '../../stores/macroStore';

interface HotkeySettingsPanelProps {
  value: CustomHotkeys;
  onChange: (value: CustomHotkeys) => void;
  disabled?: boolean;
}

export function HotkeySettingsPanel({ value, onChange, disabled = false }: HotkeySettingsPanelProps) {
  const { t } = useTranslation();
  const macros = useMacroStore((state) => state.macros);
  const [query, setQuery] = useState('');
  const [capturingId, setCapturingId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const translateCommandLabel = useCallback((command: CommandMetadata) => (
    command.labelKey ? t(command.labelKey, { defaultValue: command.label }) : command.label
  ), [t]);

  const translateCommandGroup = useCallback((command: CommandMetadata) => (
    command.groupKey ? t(command.groupKey, { defaultValue: command.group }) : command.group
  ), [t]);

  const commands = useMemo(() => {
    const q = query.trim().toLowerCase();
    return getCommandMetadata()
      .filter((command) => command.editable || command.defaultHotkey)
      .map((command) => ({
        ...command,
        displayLabel: translateCommandLabel(command),
        displayGroup: translateCommandGroup(command),
      }))
      .filter((command) => {
        if (!q) return true;
        return command.displayLabel.toLowerCase().includes(q)
          || command.displayGroup.toLowerCase().includes(q)
          || command.id.toLowerCase().includes(q)
          || (getEffectiveHotkey(command.id, value)?.toLowerCase().includes(q) ?? false);
      });
  }, [query, translateCommandGroup, translateCommandLabel, value]);

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
      const nextValue = { ...value, [capturingId]: hotkey };
      const commandConflict = hotkeyConflictsWithCommand(capturingId, hotkey, nextValue);
      if (commandConflict) {
        setError(t('dialog.hotkey_editor.error_command_conflict', {
          hotkey,
          command: translateCommandLabel(commandConflict),
        }));
        return;
      }
      const macroConflict = macros.find((macro) => normalizeHotkey(macro.hotkey) === hotkey);
      if (macroConflict) {
        setError(t('dialog.hotkey_editor.error_macro_conflict', { hotkey, macro: macroConflict.name }));
        return;
      }
      onChange(nextValue);
      setCapturingId(null);
      setError(null);
    };
    window.addEventListener('keydown', onKeyDown, true);
    return () => window.removeEventListener('keydown', onKeyDown, true);
  }, [capturingId, macros, onChange, t, translateCommandLabel, value]);

  const resetCommand = (commandId: string) => {
    const nextValue = { ...value };
    delete nextValue[commandId];
    onChange(nextValue);
    setError(null);
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3" data-testid="hotkey-settings-panel">
      <div className="flex gap-2">
        <input
          className="h-9 min-w-0 flex-1 rounded-md border border-bb-border bg-bb-surface px-3 text-sm text-bb-text outline-none focus:border-bb-accent"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder={t('dialog.hotkey_editor.search')}
        />
        <button
          type="button"
          className="shrink-0 rounded-md border border-bb-border px-3 py-1.5 text-sm text-bb-text hover:bg-bb-surface disabled:cursor-not-allowed disabled:opacity-50"
          disabled={disabled || Object.keys(value).length === 0}
          onClick={() => {
            onChange({});
            setCapturingId(null);
            setError(null);
          }}
        >
          {t('dialog.hotkey_editor.reset_all')}
        </button>
      </div>
      {error && (
        <div className="rounded-md border border-bb-error-border bg-bb-error-bg px-3 py-2 text-sm text-bb-error-fg" role="alert">
          {error}
        </div>
      )}
      <div className="grid grid-cols-[minmax(180px,1fr)_120px_110px_150px] gap-3 border-b border-bb-border px-3 pb-2 text-[11px] font-semibold uppercase tracking-[0.1em] text-bb-text-muted">
        <span>{t('dialog.hotkey_editor.function')}</span>
        <span>{t('dialog.hotkey_editor.current')}</span>
        <span>{t('dialog.hotkey_editor.default')}</span>
        <span className="text-right">{t('dialog.hotkey_editor.actions')}</span>
      </div>
      <div className="min-h-0 flex-1 overflow-auto rounded-md border border-bb-border">
        {commands.map((command) => {
          const effectiveHotkey = getEffectiveHotkey(command.id, value);
          const defaultHotkey = normalizeHotkey(command.defaultHotkey);
          const overridden = Object.prototype.hasOwnProperty.call(value, command.id);
          return (
            <div
              key={command.id}
              className="grid grid-cols-[minmax(180px,1fr)_120px_110px_150px] items-center gap-3 border-b border-bb-border px-3 py-2 text-sm last:border-b-0"
            >
              <div className="min-w-0">
                <div className="truncate text-bb-text">{command.displayLabel}</div>
                <div className="truncate text-xs text-bb-text-muted">{command.displayGroup}</div>
              </div>
              <div className="truncate text-xs text-bb-text">
                {effectiveHotkey ?? t('dialog.hotkey_editor.none')}
                {overridden && <span className="ml-1 text-bb-accent">{t('dialog.hotkey_editor.custom')}</span>}
              </div>
              <div className="truncate text-xs text-bb-text-muted">{defaultHotkey ?? t('dialog.hotkey_editor.none')}</div>
              <div className="flex items-center justify-end gap-2">
                <button
                  type="button"
                  className="rounded-md border border-bb-border px-2 py-1 text-xs text-bb-text hover:bg-bb-surface disabled:cursor-not-allowed disabled:opacity-50"
                  disabled={!command.editable || disabled}
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
                  disabled={!command.editable || !overridden || disabled}
                  onClick={() => resetCommand(command.id)}
                >
                  {t('dialog.hotkey_editor.reset')}
                </button>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
