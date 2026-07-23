import { useState, } from 'react';
import { useTranslation } from 'react-i18next';
import { useProjectStore } from '../../stores/projectStore';
import { useAppStore } from '../../stores/appStore';
import { projectService } from '../../services/projectService';
import { SubLayerStack } from '../properties/SubLayerStack';
import { CheckSquare, Lock, ClipboardCopy, ClipboardPaste } from 'lucide-react';
import { useUiStore } from '../../stores/uiStore';
import { TextInput } from '../shared/TextInput';
import { ToggleSwitch } from '../shared/ToggleSwitch';
import { PALETTE_COLORS } from '../../constants/palette';
import { normColor } from '../../stores/layerFamilyResolver';
import { useNotificationStore } from '../../stores/notificationStore';

const SHOW_TOGGLE_ACTIVE_COLOR = 'bg-bb-accent';

/** Normalize color tag for comparison — strip alpha suffix, lowercase. */



function FrameToggle({
  active,
  onToggle,
}: {
  active: boolean;
  onToggle: () => void;
}) {
  const { t } = useTranslation();

  return (
    <button
      type="button"
      className="inline-flex w-full items-center justify-end gap-2 text-bb-text"
      onClick={(e) => {
        e.stopPropagation();
        onToggle();
      }}
      onContextMenu={(e) => {
        e.preventDefault();
        e.stopPropagation();
      }}
      title={
        active
          ? t('panels.layers.exclude_tool_layers_from_job_bounds')
          : t('panels.layers.include_tool_layers_in_job_bounds')
      }
      data-testid="frame-toggle"
    >
      <span
        className={`relative inline-flex w-7 h-3.5 shrink-0 rounded-full transition-colors ${
          active ? 'bg-green-500' : 'bg-bb-text/20'
        }`}
      >
        <span
          className={`absolute left-0 top-0.5 w-2.5 h-2.5 rounded-full bg-white shadow transition-transform ${
            active ? 'translate-x-3.5' : 'translate-x-0.5'
          }`}
        />
      </span>
      <span className="shrink-0">{t('panels.layers.frame')}</span>
    </button>
  );
}

export function LayerList() {
  const { t } = useTranslation();
  const layers = useProjectStore((s) => s.project?.layers ?? []);
  const selectedLayerId = useProjectStore((s) => s.selectedLayerId);
  const updateLayer = useProjectStore((s) => s.updateLayer);
  const reorderLayer = useProjectStore((s) => s.reorderLayer);
  const objects = useProjectStore((s) => s.project?.objects ?? []);
  const selectObjects = useProjectStore((s) => s.selectObjects);
  const lockObjects = useProjectStore((s) => s.lockObjects);
  const unlockObjects = useProjectStore((s) => s.unlockObjects);
  const copyLayerSettings = useProjectStore((s) => s.copyLayerSettings);
  const pasteLayerSettings = useProjectStore((s) => s.pasteLayerSettings);
  const layerSettingsClipboard = useUiStore((s) => s.layerSettingsClipboard);
  const loadProject = useProjectStore((s) => s.loadProject);
  const includeToolLayersInJobBounds = useAppStore(
    (s) => s.settings?.include_tool_layers_in_job_bounds ?? true,
  );
  const updateSettings = useAppStore((s) => s.updateSettings);
  const currentAppSettings = useAppStore((s) => s.settings);
  const [showColorPicker, setShowColorPicker] = useState(false);
  const [optimisticToolFrameBounds, setOptimisticToolFrameBounds] = useState<boolean | null>(null);



  
  const selectedLayer = layers.find((l) => l.id === selectedLayerId) ?? null;
  const activeLayer = selectedLayer ?? layers[0] ?? null;

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


  const handleToggleToolFrameBounds = async () => {
    const currentValue = optimisticToolFrameBounds ?? includeToolLayersInJobBounds;
    const nextValue = !currentValue;
    setOptimisticToolFrameBounds(nextValue);
    if (currentAppSettings) {
      useAppStore.setState({
        settings: {
          ...currentAppSettings,
          include_tool_layers_in_job_bounds: nextValue,
        },
      });
    }
    try {
      await updateSettings({
        include_tool_layers_in_job_bounds: nextValue,
      });
      setOptimisticToolFrameBounds(null);
    } catch (error) {
      if (currentAppSettings) {
        useAppStore.setState({ settings: currentAppSettings });
      }
      setOptimisticToolFrameBounds(null);
      notifyLayerError('panels.layers.errors.update_tool_frame', error);
    }
  };

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
        <div className="m-2 overflow-hidden rounded-xl border border-bb-accent/40 bg-bb-surface shadow-sm" data-testid="layer-block">
          <div className="px-3 py-2.5">
          <div className="mb-2 flex items-center justify-between">
            <span className="text-[10px] font-semibold uppercase tracking-wider text-bb-text-dim">
              {t('panels.properties.layer')}
            </span>
            {/* Move the layer earlier/later in the run order (tab order) */}
            <div className="flex items-center gap-0.5">
              <button
                className="rounded p-1 text-bb-text-muted hover:bg-bb-hover hover:text-bb-text disabled:opacity-30"
                title={t('panels.layers.move_up')}
                disabled={layers.findIndex((l) => l.id === activeLayer.id) === 0}
                onClick={() => void reorderLayer(activeLayer.id, layers.findIndex((l) => l.id === activeLayer.id) - 1)}
                data-testid="layer-move-earlier"
              >
                ◀
              </button>
              <button
                className="rounded p-1 text-bb-text-muted hover:bg-bb-hover hover:text-bb-text disabled:opacity-30"
                title={t('panels.layers.move_down')}
                disabled={layers.findIndex((l) => l.id === activeLayer.id) === layers.length - 1}
                onClick={() => void reorderLayer(activeLayer.id, layers.findIndex((l) => l.id === activeLayer.id) + 1)}
                data-testid="layer-move-later"
              >
                ▶
              </button>
            </div>
          </div>

          <TextInput
            label={t('panels.properties.name')}
            value={activeLayer.name}
            onChange={(name) => void updateLayer(activeLayer.id, { name })}
          />

          <div className="mt-2 flex items-center justify-between gap-4">
            {!activeLayer.is_tool_layer && (
              <div className="relative flex items-center gap-1.5 text-xs" data-testid="quick-edit">
                <span className="text-bb-text-muted shrink-0">{t('panels.layers.quick_edit.color')}</span>
                <button
                  className="h-6 w-8 rounded-md border border-bb-border shrink-0 hover:ring-1 hover:ring-bb-accent"
                  data-testid="quick-edit-color"
                  style={{ backgroundColor: activeLayer.color_tag }}
                  onClick={() => setShowColorPicker((v) => !v)}
                  aria-label={t('panels.layers.quick_edit.color')}
                />
                {showColorPicker && (
                  <>
                    <div className="fixed inset-0 z-40" onClick={() => setShowColorPicker(false)} />
                    <div
                      className="absolute left-0 top-full z-50 mt-1 grid grid-cols-8 gap-1.5 rounded-lg border border-bb-border bg-bb-panel p-2.5 shadow-lg"
                      data-testid="layer-color-picker"
                    >
                      {PALETTE_COLORS.filter((c) => {
                        if (normColor(c.hex) === normColor(activeLayer.color_tag)) return true;
                        return !layers.some(
                          (l) => l.id !== activeLayer.id && normColor(l.color_tag) === normColor(c.hex),
                        );
                      }).map((c) => (
                        <button
                          key={c.hex}
                          className={`h-6 w-6 rounded-md border hover:ring-1 hover:ring-bb-accent ${
                            c.is_tool_layer ? 'border-dashed border-bb-text-muted' : 'border-bb-border'
                          }`}
                          style={{ backgroundColor: c.hex }}
                          title={c.name}
                          onClick={() => {
                            setShowColorPicker(false);
                            void updateLayer(activeLayer.id, { color_tag: c.hex });
                          }}
                        />
                      ))}
                    </div>
                  </>
                )}
              </div>
            )}
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
              {activeLayer.is_tool_layer && (
                <FrameToggle
                  active={optimisticToolFrameBounds ?? includeToolLayersInJobBounds}
                  onToggle={() => void handleToggleToolFrameBounds()}
                />
              )}
            </div>
          </div>

          {/* Layer actions: select all, lock, copy/paste settings */}
          <div className="mt-2.5 flex gap-1 border-t border-bb-border pt-2">
            <button
              data-testid="select-all-on-layer"
              className="p-1 rounded hover:bg-bb-hover text-bb-text-muted hover:text-bb-text"
              title={t('panels.layers.select_all_objects_title')}
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
              data-testid="lock-layer"
              className="p-1 rounded hover:bg-bb-hover text-bb-text-muted hover:text-bb-text"
              title={t('panels.layers.toggle_lock_title')}
              onClick={() => {
                const layerObjs = objects.filter((o) => o.layer_id === activeLayer.id);
                const allLocked = layerObjs.length > 0 && layerObjs.every((o) => o.locked);
                const layerObjectIds = layerObjs.map((o) => o.id);
                if (layerObjectIds.length === 0) return;
                if (allLocked) void unlockObjects(layerObjectIds);
                else void lockObjects(layerObjectIds);
              }}
            >
              <Lock size={16} />
            </button>
            <button
              data-testid="copy-layer-settings"
              className="p-1 rounded hover:bg-bb-hover text-bb-text-muted hover:text-bb-text"
              title={t('panels.layers.copy_settings_title')}
              onClick={() => { if (!activeLayer.is_tool_layer) copyLayerSettings(activeLayer.id); }}
              disabled={activeLayer.is_tool_layer}
            >
              <ClipboardCopy size={16} />
            </button>
            <button
              data-testid="paste-layer-settings"
              className="p-1 rounded hover:bg-bb-hover text-bb-text-muted hover:text-bb-text disabled:opacity-40 disabled:cursor-not-allowed"
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
          </div>
          </div>

          {/* Cut settings — flat for a single sub-layer; the stacked
              sub-layer UI only appears once a second one exists. */}
          {!activeLayer.is_tool_layer && (
            <div className="border-t border-bb-border px-3 py-2.5">
              {activeLayer.entries.length > 1 && (
                <div className="pb-2 text-[10px] font-semibold uppercase tracking-wider text-bb-text-dim">
                  {t('panels.sub_layer_stack.title')}
                </div>
              )}
              <SubLayerStack layerId={activeLayer.id} />
            </div>
          )}
        </div>
      )}
    </div>
  );
}
