import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { save, open } from '@tauri-apps/plugin-dialog';
import { useTranslation } from 'react-i18next';

import { useMacroStore } from '../../stores/macroStore';
import { useNotificationStore } from '../../stores/notificationStore';
import { wrapBackendError } from '../../i18n/errors';
import type { MacroDefinition } from '../../types/macro';
import { hotkeysConflict } from '../../utils/hotkeyMatch';

/** Built-in keyboard shortcuts that macros should not override. */
const BUILTIN_HOTKEYS = [
  'Ctrl+Z', 'Ctrl+Y', 'Ctrl+Shift+Z',
  'Ctrl+C', 'Ctrl+X', 'Ctrl+V',
  'Ctrl+A', 'Ctrl+D', 'Ctrl+G', 'Ctrl+Shift+G',
  'Ctrl+S', 'Ctrl+Shift+S', 'Ctrl+N', 'Ctrl+O',
  'Ctrl+E', 'Ctrl+Shift+E',
  'Delete', 'Backspace', 'Escape',
  'V', 'M', 'N', 'P', 'L', 'R', 'T', 'E', 'H', 'B',
] as const;

export function MacrosWindow(): React.ReactElement {
  const { t } = useTranslation();
  const { macros, loadMacros, saveMacro, deleteMacro, runMacro } = useMacroStore();

  const [editingId, setEditingId] = useState<string | null>(null);
  const [editingMacro, setEditingMacro] = useState<MacroDefinition | null>(null);
  const [editName, setEditName] = useState('');
  const [editDesc, setEditDesc] = useState('');
  const [editCommands, setEditCommands] = useState('');
  const [editHotkey, setEditHotkey] = useState('');
  const [editToolbar, setEditToolbar] = useState(false);
  const [pendingEditMacro, setPendingEditMacro] = useState<MacroDefinition | null>(null);

  const getHotkeyConflict = (hotkey: string): string | null => {
    if (!hotkey.trim()) return null;
    const normalized = hotkey.trim();
    const builtInConflict = BUILTIN_HOTKEYS.find((reserved) => hotkeysConflict(reserved, normalized));
    if (builtInConflict) {
      return t('panels.machine.macros.conflicts_with_builtin_shortcut', { shortcut: builtInConflict });
    }
    const otherMacro = (macros ?? []).find(
      (m) => m.id !== editingId && hotkeysConflict(m.hotkey, normalized),
    );
    if (otherMacro) return t('panels.machine.macros.conflicts_with_macro', { name: otherMacro.name });
    return null;
  };

  useEffect(() => {
    loadMacros();
  }, [loadMacros]);

  const hasUnsavedChanges = editingMacro !== null && (
    editName !== editingMacro.name ||
    editDesc !== (editingMacro.description ?? '') ||
    editCommands !== (editingMacro.commands ?? []).join('\n') ||
    editHotkey !== (editingMacro.hotkey ?? '') ||
    editToolbar !== (editingMacro.show_in_toolbar ?? false)
  );

  function handleAdd(): void {
    void saveMacro({
      id: crypto.randomUUID(),
      name: t('panels.machine.macros.new_macro'),
      commands: [],
      description: '',
    });
  }

  function beginEditing(macro: MacroDefinition): void {
    setPendingEditMacro(null);
    setEditingId(macro.id);
    setEditingMacro(macro);
    setEditName(macro.name);
    setEditDesc(macro.description ?? '');
    setEditCommands((macro.commands ?? []).join('\n'));
    setEditHotkey(macro.hotkey ?? '');
    setEditToolbar(macro.show_in_toolbar ?? false);
  }

  function handleEdit(macro: MacroDefinition): void {
    if (editingId !== null && editingId !== macro.id && hasUnsavedChanges) {
      setPendingEditMacro(macro);
      return;
    }
    beginEditing(macro);
  }

  async function handleSave() {
    if (editingId === null) return;
    const hotkeyConflict = getHotkeyConflict(editHotkey);
    if (hotkeyConflict) {
      useNotificationStore.getState().push(hotkeyConflict, 'error');
      return;
    }
    const saved = await saveMacro({
      id: editingId,
      name: editName,
      commands: editCommands.split('\n').filter(Boolean),
      description: editDesc,
      hotkey: editHotkey.trim() || undefined,
      show_in_toolbar: editToolbar,
    });
    if (saved) {
      setEditingId(null);
      setEditingMacro(null);
    }
  }

  function handleCancel(): void {
    setEditingId(null);
    setEditingMacro(null);
  }

  async function handleExportMacros() {
    try {
      const path = await save({
        filters: [{ name: 'JSON', extensions: ['json'] }],
        defaultPath: 'macros.json',
      });
      if (!path) return;
      await invoke('export_macros', { path });
      useNotificationStore.getState().push(t('panels.machine.macros.macros_exported'), 'success');
    } catch (e) {
      useNotificationStore.getState().push(wrapBackendError(String(e)), 'error');
    }
  }

  async function handleImportMacros() {
    try {
      const selected = await open({
        filters: [{ name: 'JSON', extensions: ['json'] }],
        multiple: false,
      });
      if (selected === null || Array.isArray(selected)) return;
      await invoke('import_macros', { path: selected });
      await loadMacros();
      useNotificationStore.getState().push(t('panels.machine.macros.macros_imported'), 'success');
    } catch (e) {
      useNotificationStore.getState().push(wrapBackendError(String(e)), 'error');
    }
  }

  return (
    <div className="px-2 pb-2 space-y-1">
      {pendingEditMacro && (
        <div className="rounded border border-bb-warning-border bg-bb-warning-bg p-2 text-xs text-bb-text">
          <div className="mb-2 text-bb-warning-fg">
            {t('panels.machine.macros.discard_unsaved_changes')}
          </div>
          <div className="flex justify-end gap-2">
            <button
              className="rounded border border-bb-border px-2 py-0.5 text-bb-text-muted hover:bg-bb-hover"
              onClick={() => setPendingEditMacro(null)}
            >
              {t('panels.machine.macros.keep_editing')}
            </button>
            <button
              className="rounded bg-bb-warning px-2 py-0.5 font-medium text-bb-on-warning hover:bg-bb-warning-hover"
              onClick={() => {
                const macro = pendingEditMacro;
                if (macro) beginEditing(macro);
              }}
            >
              {t('panels.machine.macros.discard')}
            </button>
          </div>
        </div>
      )}
      <button
        className="w-full px-2 py-1 text-xs bg-bb-accent text-bb-on-accent rounded hover:bg-bb-accent-hover"
        onClick={handleAdd}
      >
        {t('panels.machine.macros.add_macro')}
      </button>
      <div className="flex gap-1">
        <button
          data-testid="import-macros"
          className="flex-1 px-2 py-1 text-xs bg-bb-bg border border-bb-border text-bb-text rounded hover:bg-bb-hover"
          onClick={() => void handleImportMacros()}
        >
          {t('panels.machine.macros.import')}
        </button>
        <button
          data-testid="export-macros"
          className="flex-1 px-2 py-1 text-xs bg-bb-bg border border-bb-border text-bb-text rounded hover:bg-bb-hover"
          onClick={() => void handleExportMacros()}
        >
          {t('panels.machine.macros.export')}
        </button>
      </div>
      {(macros ?? []).map((macro) => (
        <div
          key={macro.id}
          className="flex items-center gap-1 p-1 bg-bb-bg border border-bb-border rounded text-xs"
        >
          {editingId === macro.id ? (
            <div className="flex-1 space-y-1">
              <input
                value={editName}
                onChange={(e) => setEditName(e.target.value)}
                className="w-full px-1 py-0.5 bg-bb-bg border border-bb-border rounded text-xs"
                placeholder={t('panels.machine.macros.name')}
              />
              <input
                value={editDesc}
                onChange={(e) => setEditDesc(e.target.value)}
                className="w-full px-1 py-0.5 bg-bb-bg border border-bb-border rounded text-xs"
                placeholder={t('panels.machine.macros.description')}
              />
              <textarea
                value={editCommands}
                onChange={(e) => setEditCommands(e.target.value)}
                className="w-full px-1 py-0.5 bg-bb-bg border border-bb-border rounded text-xs font-mono h-16"
                placeholder={t('panels.machine.macros.gcode_placeholder')}
              />
              <div className="flex gap-2 items-center">
                <input
                  value={editHotkey}
                  onChange={(e) => setEditHotkey(e.target.value)}
                  className="flex-1 px-1 py-0.5 bg-bb-bg border border-bb-border rounded text-xs"
                  placeholder={t('panels.machine.macros.hotkey_placeholder')}
                  data-testid="hotkey-input"
                />
                {getHotkeyConflict(editHotkey) && (
                  <span className="text-xs text-bb-error-fg" data-testid="hotkey-conflict">
                    {getHotkeyConflict(editHotkey)}
                  </span>
                )}
                <label className="flex items-center gap-1 text-xs whitespace-nowrap">
                  <input
                    type="checkbox"
                    checked={editToolbar}
                    onChange={(e) => setEditToolbar(e.target.checked)}
                    className="accent-bb-accent"
                    data-testid="toolbar-checkbox"
                  />
                  {t('panels.machine.macros.toolbar')}
                </label>
              </div>
              <div className="flex gap-1">
                <button
                  className="px-2 py-0.5 bg-bb-accent text-bb-on-accent rounded text-xs hover:bg-bb-accent-hover"
                  onClick={() => void handleSave()}
                >
                  {t('common.save')}
                </button>
                <button
                  className="px-2 py-0.5 bg-bb-bg border border-bb-border rounded text-xs hover:bg-bb-border"
                  onClick={handleCancel}
                >
                  {t('common.cancel')}
                </button>
              </div>
            </div>
          ) : (
            <>
              <button
                title={t('panels.machine.macros.run')}
                className="px-1 hover:text-bb-accent"
                onClick={() => runMacro(macro.id)}
              >
                {t('panels.machine.macros.run_short')}
              </button>
              <div className="flex-1 truncate">
                <span className="font-medium">{macro.name}</span>
                {macro.description && (
                  <span className="text-bb-text-muted ml-1">-- {macro.description}</span>
                )}
                {macro.hotkey && (
                  <span className="text-bb-text-muted ml-1">[{macro.hotkey}]</span>
                )}
              </div>
              <button
                title={t('panels.machine.macros.edit')}
                className="px-1 hover:text-bb-accent"
                onClick={() => handleEdit(macro)}
              >
                {t('panels.machine.macros.edit_short')}
              </button>
              <button
                title={t('panels.machine.macros.delete')}
                className="px-1 hover:text-bb-error-fg"
                onClick={() => deleteMacro(macro.id)}
              >
                {t('panels.machine.macros.delete_short')}
              </button>
            </>
          )}
        </div>
      ))}
    </div>
  );
}
