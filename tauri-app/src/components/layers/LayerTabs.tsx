import { useTranslation } from 'react-i18next';
import { Eye, EyeOff, Plus } from 'lucide-react';
import { useProjectStore } from '../../stores/projectStore';
import { useAppStore } from '../../stores/appStore';
import { projectService } from '../../services/projectService';
import { useNotificationStore } from '../../stores/notificationStore';
import { wrapBackendError } from '../../i18n/errors';
import { resolveDestinationLayer, normColor } from '../../stores/layerFamilyResolver';
import { PALETTE_COLORS } from '../../constants/palette';
import { formatSpeedForDisplay } from '../../utils/speedUnits';

/**
 * Layer tabs above the workspace. One tab per layer: color dot, name,
 * speed/power summary, visibility eye. Clicking a tab with a selection
 * assigns the selection to that layer (the palette's one-click flow);
 * with no selection it sets the active layer for new objects. The `+`
 * tab creates a layer using the first unused palette color.
 */
export function LayerTabs() {
  const { t } = useTranslation();
  const project = useProjectStore((s) => s.project);
  const selectedObjectIds = useProjectStore((s) => s.selectedObjectIds);
  const selectedLayerId = useProjectStore((s) => s.selectedLayerId);
  const selectLayer = useProjectStore((s) => s.selectLayer);
  const reassignLayer = useProjectStore((s) => s.reassignLayer);
  const addLayer = useProjectStore((s) => s.addLayer);
  const updateLayer = useProjectStore((s) => s.updateLayer);
  const loadProject = useProjectStore((s) => s.loadProject);
  const displayUnit = useAppStore((s) => s.settings?.display_unit) === 'inches' ? 'inches' : 'mm';
  const speedTimeUnit = useAppStore((s) => s.settings?.speed_time_unit) === 'seconds' ? 'seconds' : 'minutes';

  if (!project) return null;

  const hasSelection = selectedObjectIds.length > 0;

  const handleTabClick = (layerId: string) => {
    if (hasSelection) {
      void reassignLayer(selectedObjectIds, layerId);
    }
    selectLayer(layerId);
  };

  const handleToggleVisible = async (layerId: string, visible: boolean) => {
    try {
      await projectService.setLayerVisible(layerId, visible);
      await loadProject({ invalidatePreview: true });
    } catch (error) {
      useNotificationStore.getState().push(wrapBackendError(String(error)), 'error');
    }
  };

  const handleAddLayer = async () => {
    const used = new Set(project.layers.map((l) => normColor(l.color_tag)));
    const nextColor = PALETTE_COLORS.find(
      (c) => !c.is_tool_layer && !used.has(normColor(c.hex)),
    );
    if (!nextColor) return;
    const out = resolveDestinationLayer({
      project,
      requestedLayerId: null,
      pendingColor: nextColor.hex,
      selectedLayerId: null,
      contentKind: 'non_raster',
    });
    if (out.kind === 'needs_create') {
      await addLayer(out.suggestedName, out.operation);
      const createdLayers = useProjectStore.getState().project?.layers ?? [];
      const created = createdLayers[createdLayers.length - 1];
      if (created) await updateLayer(created.id, { color_tag: out.colorTag });
    } else {
      selectLayer(out.layerId);
    }
  };

  return (
    <div className="no-select flex items-end gap-1 overflow-x-auto scrollbar-none px-3 pt-2 bg-bb-bg">
      {project.layers.map((layer) => {
        const active = layer.id === selectedLayerId;
        const entry = layer.entries[0];
        const hidden = layer.visible === false;
        return (
          <button
            key={layer.id}
            onClick={() => handleTabClick(layer.id)}
            className={`flex flex-shrink-0 items-center gap-1.5 rounded-t-lg border border-b-0 px-2.5 py-1 text-xxs ${
              active
                ? 'border-bb-accent/50 bg-bb-surface-2 text-bb-text'
                : 'border-bb-border bg-bb-surface text-bb-text-muted hover:text-bb-text'
            }`}
            title={layer.name}
          >
            <span
              className="h-2 w-2 flex-shrink-0 rounded-full"
              style={{ backgroundColor: layer.color_tag }}
            />
            <span className={`max-w-24 truncate ${active ? 'font-semibold' : ''}`}>{layer.name}</span>
            {!layer.is_tool_layer && entry && (
              <span className="text-bb-text-dim">
                {formatSpeedForDisplay(entry.speed_mm_min ?? 1000, displayUnit, speedTimeUnit)}/{entry.power_percent ?? 50}%
              </span>
            )}
            <span
              role="button"
              tabIndex={0}
              aria-label={t('panels.layers.header.show')}
              className={`${hidden ? 'text-bb-text-disabled' : 'text-bb-text-dim'} hover:text-bb-text`}
              onClick={(e) => {
                e.stopPropagation();
                void handleToggleVisible(layer.id, hidden);
              }}
              onKeyDown={(e) => {
                if (e.key === 'Enter' || e.key === ' ') {
                  e.stopPropagation();
                  void handleToggleVisible(layer.id, hidden);
                }
              }}
            >
              {hidden ? <EyeOff size={11} /> : <Eye size={11} />}
            </span>
          </button>
        );
      })}
      <button
        onClick={() => void handleAddLayer()}
        aria-label={t('panels.layers.add_layer')}
        title={t('panels.layers.add_layer')}
        className="flex flex-shrink-0 items-center rounded-t-lg border border-b-0 border-bb-border bg-bb-surface px-2 py-1 text-bb-text-muted hover:text-bb-text"
      >
        <Plus size={12} />
      </button>
    </div>
  );
}
