import { useState, } from 'react';
import { useTranslation } from 'react-i18next';
import { useProjectStore } from '../../stores/projectStore';
import { useUiStore } from '../../stores/uiStore';
import { useAppStore } from '../../stores/appStore';
import { projectService } from '../../services/projectService';
import { NumberInput } from '../shared/NumberInput';
import { SubLayerStack } from '../properties/SubLayerStack';
import { PALETTE_COLORS } from '../../constants/palette';
import { normColor } from '../../stores/layerFamilyResolver';
import { useNotificationStore } from '../../stores/notificationStore';
import { CheckSquare, ClipboardCopy, ClipboardPaste, Lock } from 'lucide-react';
import type { Layer, OperationType, VectorSettings } from '../../types/project';
import { effectiveLineIntervalMm } from '../../types/rasterSettings';
import {
  displaySpeedToMmMin,
  
  speedInputValue,
  speedMmMinToDisplay,
  speedStepForUnit,
  speedUnitLabel,
} from '../../utils/speedUnits';
import { mmToDisplay, displayToMm, roundDisplayLength, lengthStep, lengthUnitLabel, labelWithUnit } from '../../utils/lengthUnits';

const MODE_OPTIONS: { value: OperationType; labelKey: string }[] = [
  { value: 'line', labelKey: 'panels.layers.mode.line' },
  { value: 'fill', labelKey: 'panels.layers.mode.fill' },
  { value: 'offset_fill', labelKey: 'panels.layers.mode.offset_fill' },
];

const SHOW_TOGGLE_ACTIVE_COLOR = 'bg-bb-accent';
const AIR_TOGGLE_ACTIVE_COLOR = 'bg-yellow-500';

/** Normalize color tag for comparison — strip alpha suffix, lowercase. */

function primaryEntry(layer: Layer) {
  return layer.entries[0];
}

function layerOperation(layer: Layer): OperationType {
  return primaryEntry(layer)?.operation ?? 'line';
}

function primaryRasterSettings(layer: Layer) {
  return primaryEntry(layer)?.raster_settings ?? null;
}

function primaryVectorSettings(layer: Layer) {
  return primaryEntry(layer)?.vector_settings ?? null;
}

function ToggleSwitch({
  active,
  activeColor,
  onClick,
  testId,
}: {
  active: boolean;
  activeColor?: string;
  onClick: () => void;
  testId?: string;
}) {
  return (
    <button
      onClick={(e) => { e.stopPropagation(); onClick(); }}
      onContextMenu={(e) => { e.preventDefault(); e.stopPropagation(); }}
      className={`relative w-7 h-3.5 rounded-full mx-auto transition-colors ${
        active ? (activeColor ?? 'bg-green-500') : 'bg-bb-text/20'
      }`}
      data-testid={testId}
    >
      <div className={`absolute top-0.5 w-2.5 h-2.5 rounded-full bg-white shadow transition-transform ${
        active ? 'translate-x-3.5' : 'translate-x-0.5'
      }`} />
    </button>
  );
}

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
          className={`absolute top-0.5 w-2.5 h-2.5 rounded-full bg-white shadow transition-transform ${
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
  const updateCutEntry = useProjectStore((s) => s.updateCutEntry);
  const lockObjects = useProjectStore((s) => s.lockObjects);
  const unlockObjects = useProjectStore((s) => s.unlockObjects);
  const loadProject = useProjectStore((s) => s.loadProject);
  const includeToolLayersInJobBounds = useAppStore(
    (s) => s.settings?.include_tool_layers_in_job_bounds ?? true,
  );
  const updateSettings = useAppStore((s) => s.updateSettings);
  const currentAppSettings = useAppStore((s) => s.settings);
  const displayUnit = currentAppSettings?.display_unit === 'inches' ? 'inches' : 'mm';
  const speedTimeUnit = currentAppSettings?.speed_time_unit === 'seconds' ? 'seconds' : 'minutes';
  const speedLabel = speedUnitLabel(displayUnit, speedTimeUnit);
  const speedStep = speedStepForUnit(displayUnit, speedTimeUnit);
  const maxDisplaySpeed = speedMmMinToDisplay(50000, displayUnit, speedTimeUnit);
  const minDisplaySpeed = speedMmMinToDisplay(1, displayUnit, speedTimeUnit);
  const [showColorPicker, setShowColorPicker] = useState(false);
  const [optimisticToolFrameBounds, setOptimisticToolFrameBounds] = useState<boolean | null>(null);

  // M4: app-scoped clipboard via uiStore (replaces the old narrow component-local useRef).
  const copyLayerSettings = useProjectStore((s) => s.copyLayerSettings);
  const pasteLayerSettings = useProjectStore((s) => s.pasteLayerSettings);
  const layerSettingsClipboard = useUiStore((s) => s.layerSettingsClipboard);

  const objects = useProjectStore((s) => s.project?.objects ?? []);
  const selectObjects = useProjectStore((s) => s.selectObjects);

  
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

  const handleToggleAirAssist = async (layerId: string, airAssist: boolean) => {
    try {
      await projectService.setLayerAirAssist(layerId, airAssist);
      await reloadLayers(true);
    } catch (error) {
      notifyLayerError('panels.layers.errors.update_air_assist', error);
    }
  };

  const handleModeChange = async (layer: Layer, operation: OperationType) => {
    try {
      const entry = primaryEntry(layer);
      if (!entry) return;
      await projectService.updateCutEntry(layer.id, entry.id, { operation });
      await reloadLayers(true);
    } catch (error) {
      notifyLayerError('panels.layers.errors.update_mode', error);
    }
  };

  const getPassCount = (layer: Layer): number => {
    if (primaryVectorSettings(layer)) return primaryVectorSettings(layer)!.passes;
    if (primaryRasterSettings(layer)) return primaryRasterSettings(layer)!.passes;
    return 1;
  };

  const setPassCount = async (layer: Layer, passes: number) => {
    const entry = primaryEntry(layer);
    if (!entry) return;
    try {
      if (entry.raster_settings) {
        await projectService.updateCutEntry(layer.id, entry.id, {
          raster_settings: { ...entry.raster_settings, passes },
        });
      } else if (entry.vector_settings) {
        await projectService.updateCutEntry(layer.id, entry.id, {
          vector_settings: { ...entry.vector_settings, passes },
        });
      } else {
        return;
      }
      await reloadLayers(true);
    } catch (error) {
      notifyLayerError('panels.layers.errors.update_passes', error);
      await reloadLayers();
    }
  };

  const getInterval = (layer: Layer): number => {
    return Math.round(effectiveLineIntervalMm(primaryRasterSettings(layer)) * 1000) / 1000;
  };

  const setInterval = (layer: Layer, lineIntervalMm: number) => {
    const entry = primaryEntry(layer);
    if (!entry?.raster_settings) return;
    useProjectStore.getState().updateCutEntry(layer.id, entry.id, {
      raster_settings: {
        ...entry.raster_settings,
        line_interval_mm: lineIntervalMm,
        dpi: lineIntervalMm > 0 ? Math.round(25.4 / lineIntervalMm) : 254,
      },
    });
  };

  const isFillMode = (layer: Layer) =>
    !layer.is_tool_layer &&
    (layerOperation(layer) === 'fill' || layerOperation(layer) === 'offset_fill' || layerOperation(layer) === 'image');

  const isVectorMode = (layer: Layer) =>
    !layer.is_tool_layer &&
    (layerOperation(layer) === 'line' || layerOperation(layer) === 'cut' || layerOperation(layer) === 'score');

  const updateVectorSettings = async (layer: Layer, patch: Partial<VectorSettings>) => {
    const entry = primaryEntry(layer);
    const vs = entry?.vector_settings;
    if (!vs) return;
    try {
      await projectService.updateCutEntry(layer.id, entry.id, {
        vector_settings: { ...vs, ...patch },
      });
      await reloadLayers(true);
    } catch (error) {
      notifyLayerError('panels.layers.errors.update_vector_settings', error);
      await reloadLayers();
    }
  };

  // M4: full-stack copy/paste via app-scoped clipboard. Backend mints fresh entry IDs and
  // replaces the target layer's entries[] in one atomic op (one undo snapshot).
  const handleCopySettings = (layer: Layer) => {
    if (layer.is_tool_layer) return;
    copyLayerSettings(layer.id);
  };

  const handlePasteSettings = async (layerId: string) => {
    try {
      await pasteLayerSettings(layerId);
    } catch (error) {
      notifyLayerError('panels.layers.errors.paste_settings', error);
    }
  };

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
      {/* Layer action buttons */}
      {activeLayer && (
        <div className="flex gap-1 px-1.5 py-1 border-b border-bb-border bg-bb-bg-alt" data-testid="layer-actions">
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
              if (allLocked) {
                void unlockObjects(layerObjectIds);
              } else {
                void lockObjects(layerObjectIds);
              }
            }}
          >
            <Lock size={16} />
          </button>
          {/* M4: dedicated Copy/Paste side buttons for the Cuts/Layers panel. */}
          <button
            data-testid="copy-layer-settings"
            className="p-1 rounded hover:bg-bb-hover text-bb-text-muted hover:text-bb-text"
            title={t('panels.layers.copy_settings_title')}
            onClick={() => handleCopySettings(activeLayer)}
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
            onClick={() => void handlePasteSettings(activeLayer.id)}
          >
            <ClipboardPaste size={16} />
          </button>
        </div>
      )}

      {/* Layer-level toggles for the selected layer (moved out of the rows) */}
      {activeLayer && (
        <div
          className="flex items-center gap-5 px-2 py-1.5 border-t-2 border-bb-border bg-bb-surface text-xs"
          data-testid="layer-toggles"
        >
          {!activeLayer.is_tool_layer && (
            <label className="flex items-center gap-1.5 text-bb-text-muted">
              {t('panels.layers.header.output')}
              <ToggleSwitch
                active={activeLayer.enabled}
                onClick={() => updateLayer(activeLayer.id, { enabled: !activeLayer.enabled })}
                testId="output-toggle"
              />
            </label>
          )}
          <label className="flex items-center gap-1.5 text-bb-text-muted">
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
          {!activeLayer.is_tool_layer && (
            <label className="flex items-center gap-1.5 text-bb-text-muted">
              {t('panels.layers.header.air')}
              <ToggleSwitch
                active={primaryEntry(activeLayer)?.air_assist === true}
                activeColor={AIR_TOGGLE_ACTIVE_COLOR}
                onClick={() => void handleToggleAirAssist(activeLayer.id, !(primaryEntry(activeLayer)?.air_assist ?? false))}
                testId="air-toggle"
              />
            </label>
          )}
        </div>
      )}

      {/* Quick-edit section — always visible, shows selected or first layer */}
      {activeLayer && !activeLayer.is_tool_layer && (
        <div className="px-2 py-1.5 border-t-2 border-bb-border bg-bb-surface" data-testid="quick-edit">
          <div className="grid grid-cols-2 gap-x-3 gap-y-1">
            {/* Mode (Line / Fill / Offset Fill) for the selected layer */}
            <div className="flex items-center gap-1.5 text-xs">
              <span className="text-bb-text-muted shrink-0">{t('panels.layers.header.mode')}</span>
              {primaryEntry(activeLayer)?.operation === 'image' ? (
                <span className="text-xs text-bb-text" data-testid="quick-edit-mode-image">{t('panels.layers.mode.image')}</span>
              ) : (
                <select
                  className="bg-bb-input text-bb-text border border-bb-border rounded text-xs px-0.5 py-0 h-5 w-full min-w-0 truncate"
                  value={primaryEntry(activeLayer)?.operation ?? 'line'}
                  onChange={(e) => void handleModeChange(activeLayer, e.target.value as OperationType)}
                  data-testid="quick-edit-mode"
                >
                  {MODE_OPTIONS.map((opt) => (
                    <option key={opt.value} value={opt.value}>
                      {t(opt.labelKey)}
                    </option>
                  ))}
                </select>
              )}
            </div>
            {/* Color swatch — click to pick a palette color for the layer */}
            <div className="relative flex items-center gap-1.5 text-xs">
              <span className="text-bb-text-muted shrink-0">{t('panels.layers.quick_edit.color')}</span>
              <button
                className="w-5 h-4 rounded-sm border border-bb-border shrink-0 hover:ring-1 hover:ring-bb-accent"
                data-testid="quick-edit-color"
                style={{ backgroundColor: activeLayer.color_tag }}
                onClick={() => setShowColorPicker((v) => !v)}
                aria-label={t('panels.layers.quick_edit.color')}
              />
              {showColorPicker && (
                <>
                  <div className="fixed inset-0 z-40" onClick={() => setShowColorPicker(false)} />
                  <div
                    className="absolute left-0 top-full z-50 mt-1 grid grid-cols-10 gap-1 rounded-lg border border-bb-border bg-bb-panel p-2 shadow-lg"
                    data-testid="layer-color-picker"
                  >
                    {PALETTE_COLORS.filter((c) => {
                      if (c.is_tool_layer) return false;
                      // Hide colors already used by other layers — one color,
                      // one layer family. The current layer's own color stays.
                      if (normColor(c.hex) === normColor(activeLayer.color_tag)) return true;
                      return !layers.some(
                        (l) => l.id !== activeLayer.id && normColor(l.color_tag) === normColor(c.hex),
                      );
                    }).map((c) => (
                      <button
                        key={c.hex}
                        className="h-4 w-4 rounded-sm border border-bb-border hover:ring-1 hover:ring-bb-accent"
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

            {/* Speed */}
            <NumberInput
              label={t('panels.layers.quick_edit.speed_with_unit', { unit: speedLabel })}
              value={speedInputValue(primaryEntry(activeLayer)?.speed_mm_min ?? 1000, displayUnit, speedTimeUnit)}
              onChange={(v) => {
                const entry = primaryEntry(activeLayer);
                if (entry) void updateCutEntry(activeLayer.id, entry.id, {
                  speed_mm_min: displaySpeedToMmMin(v, displayUnit, speedTimeUnit),
                });
              }}
              min={minDisplaySpeed}
              max={maxDisplaySpeed}
              step={speedStep}
            />

            {/* Passes */}
            <NumberInput
              label={t('panels.layers.quick_edit.passes')}
              value={getPassCount(activeLayer)}
              onChange={(p) => { void setPassCount(activeLayer, p); }}
              min={1}
              step={1}
            />

            {/* Max Pwr */}
            <NumberInput
              label={t('panels.layers.quick_edit.max_power')}
              value={primaryEntry(activeLayer)?.power_percent ?? 50}
              onChange={(v) => {
                const entry = primaryEntry(activeLayer);
                if (entry) void updateCutEntry(activeLayer.id, entry.id, { power_percent: v });
              }}
              min={0}
              max={100}
              step={1}
            />

            {/* Interval — only for fill/image modes */}
            {isFillMode(activeLayer) && (
              <NumberInput
                label={labelWithUnit(t('panels.layers.quick_edit.interval'), lengthUnitLabel(displayUnit))}
                value={roundDisplayLength(mmToDisplay(getInterval(activeLayer), displayUnit), displayUnit)}
                onChange={(v) => setInterval(activeLayer, displayToMm(v, displayUnit))}
                min={mmToDisplay(0.01, displayUnit)}
                step={lengthStep(displayUnit, 0.01, 0.001)}
              />
            )}

            {/* Min Pwr */}
            <NumberInput
              label={t('panels.layers.quick_edit.min_power')}
              value={primaryEntry(activeLayer)?.power_min_percent ?? 0}
              onChange={(v) => {
                const entry = primaryEntry(activeLayer);
                if (entry) void updateCutEntry(activeLayer.id, entry.id, { power_min_percent: v });
              }}
              min={0}
              max={100}
              step={1}
            />
          </div>

          {/* Vector-only: perforation & kerf */}
          {isVectorMode(activeLayer) && primaryVectorSettings(activeLayer) && (
            <div className="mt-1.5 pt-1.5 border-t border-bb-border" data-testid="vector-quick-edit">
              <div className="grid grid-cols-2 gap-x-3 gap-y-1">
                <label className="flex items-center gap-1 text-xs col-span-2">
                  <input
                    type="checkbox"
                    checked={primaryVectorSettings(activeLayer)?.perforation_enabled ?? false}
                    onChange={(e) => { void updateVectorSettings(activeLayer, { perforation_enabled: e.target.checked }); }}
                    data-testid="perf-toggle"
                  />
                  {t('panels.layers.quick_edit.perforation')}
                </label>
                {primaryVectorSettings(activeLayer)?.perforation_enabled && (
                  <>
                    <NumberInput
                      label={t('panels.layers.quick_edit.on_ms')}
                      value={primaryVectorSettings(activeLayer)?.perforation_on_ms ?? 10}
                      onChange={(v) => { void updateVectorSettings(activeLayer, { perforation_on_ms: v }); }}
                      min={0}
                      step={1}
                    />
                    <NumberInput
                      label={t('panels.layers.quick_edit.off_ms')}
                      value={primaryVectorSettings(activeLayer)?.perforation_off_ms ?? 10}
                      onChange={(v) => { void updateVectorSettings(activeLayer, { perforation_off_ms: v }); }}
                      min={0}
                      step={1}
                    />
                  </>
                )}
                <NumberInput
                  label={labelWithUnit(t('panels.layers.quick_edit.kerf_mm'), lengthUnitLabel(displayUnit))}
                  value={roundDisplayLength(mmToDisplay(primaryVectorSettings(activeLayer)?.kerf_offset_mm ?? 0, displayUnit), displayUnit)}
                  onChange={(v) => { void updateVectorSettings(activeLayer, { kerf_offset_mm: displayToMm(v, displayUnit) || undefined }); }}
                  min={0}
                  step={lengthStep(displayUnit, 0.01, 0.001)}
                />
              </div>
            </div>
          )}
        </div>
      )}

      {/* Sub-layer stack (multi-pass entries) for the selected layer */}
      {activeLayer && !activeLayer.is_tool_layer && (
        <div className="border-t border-bb-border px-2 py-1.5">
          <SubLayerStack layerId={activeLayer.id} />
        </div>
      )}
    </div>
  );
}
