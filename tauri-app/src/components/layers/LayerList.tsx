import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useProjectStore } from '../../stores/projectStore';
import { projectService } from '../../services/projectService';
import { SubLayerStack } from '../properties/SubLayerStack';
import { CheckSquare, Lock, ClipboardCopy, ClipboardPaste, Trash2 } from 'lucide-react';
import { useUiStore } from '../../stores/uiStore';
import { TextInput } from '../shared/TextInput';
import { ToggleSwitch } from '../shared/ToggleSwitch';
import { PALETTE_COLORS } from '../../constants/palette';
import { normColor } from '../../stores/layerFamilyResolver';
import { useNotificationStore } from '../../stores/notificationStore';
import { INSPECTOR_CARD_CLASS, INSPECTOR_SECTION_HEADER_CLASS } from '../shared/panelAppearance';

const SHOW_TOGGLE_ACTIVE_COLOR = 'bg-bb-accent';

export function LayerList() {
  const { t } = useTranslation();
  const layers = useProjectStore((s) => s.project?.layers ?? []);
  const selectedLayerId = useProjectStore((s) => s.selectedLayerId);
  const updateLayer = useProjectStore((s) => s.updateLayer);
  const reorderLayer = useProjectStore((s) => s.reorderLayer);
  const removeLayer = useProjectStore((s) => s.removeLayer);
  const objects = useProjectStore((s) => s.project?.objects ?? []);
  const selectObjects = useProjectStore((s) => s.selectObjects);
  const lockObjects = useProjectStore((s) => s.lockObjects);
  const unlockObjects = useProjectStore((s) => s.unlockObjects);
  const copyLayerSettings = useProjectStore((s) => s.copyLayerSettings);
  const pasteLayerSettings = useProjectStore((s) => s.pasteLayerSettings);
  const layerSettingsClipboard = useUiStore((s) => s.layerSettingsClipboard);
  const loadProject = useProjectStore((s) => s.loadProject);
  const [colorPicker, setColorPicker] = useState<{ x: number; y: number } | null>(null);
  const [layerNameDraft, setLayerNameDraft] = useState('');

  const selectedLayer = layers.find((l) => l.id === selectedLayerId) ?? null;
  const activeLayer = selectedLayer ?? layers[0] ?? null;
  const activeLayerObjs = activeLayer ? objects.filter((o) => o.layer_id === activeLayer.id) : [];
  const layerAllLocked = activeLayerObjs.length > 0 && activeLayerObjs.every((o) => o.locked);

  useEffect(() => {
    setLayerNameDraft(activeLayer?.name ?? '');
  }, [activeLayer?.id, activeLayer?.name]);

  const commitLayerName = async () => {
    if (!activeLayer || layerNameDraft === activeLayer.name) return;
    const updated = await updateLayer(activeLayer.id, { name: layerNameDraft });
    if (!updated) {
      const currentLayer = useProjectStore.getState().project?.layers.find((layer) => layer.id === activeLayer.id);
      setLayerNameDraft(currentLayer?.name ?? activeLayer.name);
    }
  };

  const notifyLayerError = (messageKey: string, error: unknown) => {
    useNotificationStore.getState().push(t(messageKey, { detail: String(error) }), 'error');
  };

  const reloadLayers = async (invalidatePreview = false) => {
    try {
      await loadProject(invalidatePreview ? { invalidatePreview: true } : undefined);
    } catch (error) {
      notifyLayerError('panels.layers.errors.reload_layers', error);
    }
  };

  const handleToggleVisible = async (layerId: string, visible: boolean) => {
    try {
      await projectService.setLayerVisible(layerId, visible);
      await reloadLayers(true);
    } catch (error) {
      notifyLayerError('panels.layers.errors.update_visibility', error);
    }
  };
  // M4: full-stack copy/paste via app-scoped clipboard. Backend mints fresh entry IDs and
  // replaces the target layer's entries[] in one atomic op (one undo snapshot).


  if (layers.length === 0) {
    return (
      <div className="px-2 py-3 text-xs text-bb-text-dim text-center" data-testid="empty-layer-row">
        {t('panels.layers.empty')}
      </div>
    );
  }

  return (
    <div className="flex flex-col">
      {/* ── LAYER ─────────────────────────────────────────────── */}
      {activeLayer && (
        <div className={INSPECTOR_CARD_CLASS} data-testid="layer-block">
          <div className="px-3 py-2.5">
          <div className="mb-2 flex items-center justify-between gap-2">
            <span className={INSPECTOR_SECTION_HEADER_CLASS}>
              {t('panels.properties.layer')}
            </span>
            <div className="flex shrink-0 items-center gap-0.5" data-testid="layer-header-actions">
              <button
                type="button"
                data-testid="select-all-on-layer"
                className="rounded p-1 text-bb-text-muted hover:bg-bb-hover hover:text-bb-text disabled:cursor-not-allowed disabled:opacity-40"
                title={t('panels.layers.select_all_objects_title')}
                disabled={!objects.some((o) => o.layer_id === activeLayer.id)}
                onClick={() => {
                  const layerObjIds = objects
                    .filter((o) => o.layer_id === activeLayer.id)
                    .map((o) => o.id)
                    .reverse();
                  selectObjects(layerObjIds);
                }}
              >
                <CheckSquare size={16} />
              </button>
              <button
                type="button"
                data-testid="lock-layer"
                className={`rounded p-1 disabled:cursor-not-allowed disabled:opacity-40 ${
                  layerAllLocked
                    ? 'bg-bb-accent/15 text-bb-accent hover:bg-bb-accent/25'
                    : 'text-bb-text-muted hover:bg-bb-hover hover:text-bb-text'
                }`}
                title={t('panels.layers.toggle_lock_title')}
                disabled={!objects.some((o) => o.layer_id === activeLayer.id)}
                onClick={() => {
                  const layerObjs = objects.filter((o) => o.layer_id === activeLayer.id);
                  const layerObjectIds = layerObjs.map((o) => o.id);
                  if (layerObjectIds.length === 0) return;
                  if (layerAllLocked) void unlockObjects(layerObjectIds);
                  else void lockObjects(layerObjectIds);
                }}
              >
                <Lock size={16} />
              </button>
              <button
                type="button"
                data-testid="copy-layer-settings"
                className="rounded p-1 text-bb-text-muted hover:bg-bb-hover hover:text-bb-text disabled:cursor-not-allowed disabled:opacity-40"
                title={t('panels.layers.copy_settings_title')}
                onClick={() => {
                  if (activeLayer.is_tool_layer) return;
                  copyLayerSettings(activeLayer.id);
                  useNotificationStore.getState().push(t('panels.layers.settings_copied'), 'success');
                }}
                disabled={activeLayer.is_tool_layer}
              >
                <ClipboardCopy size={16} />
              </button>
              <button
                type="button"
                data-testid="paste-layer-settings"
                className="rounded p-1 text-bb-text-muted hover:bg-bb-hover hover:text-bb-text disabled:cursor-not-allowed disabled:opacity-40"
                title={
                  layerSettingsClipboard && layerSettingsClipboard.length > 0
                    ? t('panels.layers.paste_settings_title')
                    : t('panels.layers.no_layer_settings_on_clipboard')
                }
                disabled={activeLayer.is_tool_layer || !layerSettingsClipboard || layerSettingsClipboard.length === 0}
                onClick={() => void pasteLayerSettings(activeLayer.id)}
              >
                <ClipboardPaste size={16} />
              </button>
              <span aria-hidden="true" className="mx-0.5 h-4 w-px bg-bb-border" />
              {/* Move the layer earlier/later in the run order (tab order) */}
              <button
                type="button"
                className="rounded p-1 text-bb-text-muted hover:bg-bb-hover hover:text-bb-text disabled:opacity-30"
                title={t('panels.layers.move_up')}
                disabled={layers.findIndex((l) => l.id === activeLayer.id) === 0}
                onClick={() => void reorderLayer(activeLayer.id, layers.findIndex((l) => l.id === activeLayer.id) - 1)}
                data-testid="layer-move-earlier"
              >
                ◀
              </button>
              <button
                type="button"
                className="rounded p-1 text-bb-text-muted hover:bg-bb-hover hover:text-bb-text disabled:opacity-30"
                title={t('panels.layers.move_down')}
                disabled={layers.findIndex((l) => l.id === activeLayer.id) === layers.length - 1}
                onClick={() => void reorderLayer(activeLayer.id, layers.findIndex((l) => l.id === activeLayer.id) + 1)}
                data-testid="layer-move-later"
              >
                ▶
              </button>
              <button
                type="button"
                className="ml-1 rounded p-1 text-bb-text-muted hover:bg-bb-error-bg hover:text-bb-error-fg"
                title={t('panels.layers.delete_layer')}
                onClick={() => void removeLayer(activeLayer.id)}
                data-testid="delete-layer"
              >
                <Trash2 size={14} />
              </button>
            </div>
          </div>

          <TextInput
            label={t('panels.properties.name')}
            value={layerNameDraft}
            onChange={setLayerNameDraft}
            onBlur={() => void commitLayerName()}
            onKeyDown={(event) => {
              if (event.key === 'Enter') event.currentTarget.blur();
            }}
            data-testid="layer-quick-name-input"
          />

          <div className="mt-2 flex items-center justify-between gap-4">
            <div className="flex items-center gap-1.5 text-xs" data-testid="quick-edit">
              <span className="text-bb-text-muted shrink-0">{t('panels.layers.quick_edit.color')}</span>
              <button
                className="h-6 w-8 rounded-md border border-bb-border shrink-0 hover:ring-1 hover:ring-bb-accent"
                data-testid="quick-edit-color"
                style={{ backgroundColor: activeLayer.color_tag }}
                onClick={(e) => {
                  const rect = e.currentTarget.getBoundingClientRect();
                  setColorPicker((cur) => (cur ? null : { x: rect.left, y: rect.bottom + 4 }));
                }}
                aria-label={t('panels.layers.quick_edit.color')}
              />
            </div>
            <div className="flex items-center gap-5">
              {!activeLayer.is_tool_layer && (
                <label className="flex items-center gap-1.5 text-xs text-bb-text-muted">
                  {t('panels.layers.header.output')}
                  <ToggleSwitch
                    active={activeLayer.enabled}
                    onClick={() => updateLayer(activeLayer.id, { enabled: !activeLayer.enabled })}
                    testId="output-toggle"
                  />
                </label>
              )}
              <label className="flex items-center gap-1.5 text-xs text-bb-text-muted">
                {t('panels.layers.header.show')}
                <ToggleSwitch
                  active={activeLayer.visible !== false}
                  activeColor={SHOW_TOGGLE_ACTIVE_COLOR}
                  onClick={() => void handleToggleVisible(activeLayer.id, activeLayer.visible === false)}
                  testId="show-toggle"
                />
              </label>
            </div>
          </div>

          </div>

          {/* Cut settings — flat for a single sub-layer; the stacked
              sub-layer UI only appears once a second one exists. */}
          {!activeLayer.is_tool_layer && (
            <div className="border-t border-bb-border px-3 py-2.5">
              {activeLayer.entries.length > 1 && (
                <div className={`${INSPECTOR_SECTION_HEADER_CLASS} pb-2`}>
                  {t('panels.sub_layer_stack.title')}
                </div>
              )}
              <SubLayerStack layerId={activeLayer.id} />
            </div>
          )}
        </div>
      )}

      {/* Layer color picker — fixed position so nothing clips it */}
      {colorPicker && activeLayer && (
        <>
          <div className="fixed inset-0 z-40" onClick={() => setColorPicker(null)} />
          <div
            className="fixed z-50 w-56 rounded-xl border border-bb-border bg-bb-panel p-3 shadow-xl"
            style={{ left: colorPicker.x, top: colorPicker.y }}
            data-testid="layer-color-picker"
          >
            <div className="grid grid-cols-6 gap-2">
              {PALETTE_COLORS.filter((c) => {
                if (normColor(c.hex) === normColor(activeLayer.color_tag)) return true;
                return !layers.some(
                  (l) => l.id !== activeLayer.id && normColor(l.color_tag) === normColor(c.hex),
                );
              }).map((c) => {
                const current = normColor(c.hex) === normColor(activeLayer.color_tag);
                return (
                  <button
                    key={c.hex}
                    className={`h-7 w-7 rounded-md border hover:ring-2 hover:ring-bb-accent ${
                      current
                        ? 'ring-2 ring-bb-accent'
                        : c.is_tool_layer
                          ? 'border-dashed border-bb-text-muted'
                          : 'border-bb-border'
                    }`}
                    style={{ backgroundColor: c.hex }}
                    title={c.name}
                    onClick={() => {
                      setColorPicker(null);
                      void updateLayer(activeLayer.id, { color_tag: c.hex });
                    }}
                  />
                );
              })}
            </div>
          </div>
        </>
      )}
    </div>
  );
}
