import { useState } from 'react';
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
  const reorderLayer = useProjectStore((s) => s.reorderLayer);
  const displayUnit = useAppStore((s) => s.settings?.display_unit) === 'inches' ? 'inches' : 'mm';
  const speedTimeUnit = useAppStore((s) => s.settings?.speed_time_unit) === 'seconds' ? 'seconds' : 'minutes';
  const canvasDark = useAppStore((s) => s.settings?.dark_mode) === true;
  const [dragIndex, setDragIndex] = useState<number | null>(null);
  const [dropIndex, setDropIndex] = useState<number | null>(null);

  if (!project) return null;

  // Tabs sit on the canvas edge, so they follow the canvas light/dark
  // setting (settings.dark_mode), not the app chrome theme — matching
  // the workspace paper they attach to.
  const tabColors = canvasDark
    ? {
        active: { background: '#2a2a2a', borderColor: '#555555', color: '#e5e5e5' },
        inactive: { background: '#3a3a3a', borderColor: '#4a4a4a', color: '#9a9a9a' },
        dim: '#8a8a8a',
      }
    : {
        active: { background: '#ffffff', borderColor: '#cccccc', color: '#1e293b' },
        inactive: { background: '#eceff1', borderColor: '#d6dade', color: '#64748b' },
        dim: '#94a3b8',
      };

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
    <div className="no-select flex items-end overflow-x-auto scrollbar-none px-3 pt-2 bg-bb-bg">
      {project.layers.map((layer, index) => {
        const active = layer.id === selectedLayerId;
        const entry = layer.entries[0];
        const hidden = layer.visible === false;
        return (
          <button
            key={layer.id}
            onClick={() => handleTabClick(layer.id)}
            draggable
            onDragStart={(e) => {
              setDragIndex(index);
              e.dataTransfer.effectAllowed = 'move';
            }}
            onDragOver={(e) => {
              if (dragIndex === null || dragIndex === index) return;
              e.preventDefault();
              e.dataTransfer.dropEffect = 'move';
              setDropIndex(index);
            }}
            onDragLeave={() => {
              if (dropIndex === index) setDropIndex(null);
            }}
            onDrop={(e) => {
              e.preventDefault();
              if (dragIndex !== null && dragIndex !== index) {
                const dragged = project.layers[dragIndex];
                if (dragged) void reorderLayer(dragged.id, index);
              }
              setDragIndex(null);
              setDropIndex(null);
            }}
            onDragEnd={() => {
              setDragIndex(null);
              setDropIndex(null);
            }}
            className={`relative flex items-center gap-1.5 rounded-t-lg border border-b-0 px-2.5 text-xxs ${
              active
                ? 'flex-shrink-0 py-1.5 font-semibold shadow-sm'
                : 'min-w-0 flex-shrink py-1 opacity-80 hover:opacity-100'
            } ${dragIndex === index ? 'opacity-40' : ''}`}
            style={{
              ...(active ? tabColors.active : tabColors.inactive),
              marginLeft: index === 0 ? 0 : -8,
              zIndex: active ? 30 : 20 - Math.min(index, 15),
              // Inactive tabs compress browser-style; active keeps full detail.
              ...(active ? {} : { maxWidth: '8rem', minWidth: '1.9rem' }),
              ...(dropIndex === index
                ? { outline: '2px solid rgb(var(--bb-accent))', outlineOffset: -2 }
                : {}),
            }}
            title={layer.name}
          >
            <span
              className="h-2 w-2 flex-shrink-0 rounded-full"
              style={{ backgroundColor: layer.color_tag }}
            />
            <span className="min-w-0 max-w-32 truncate">{layer.name}</span>
            {active && !layer.is_tool_layer && entry && (
              <span className="flex-shrink-0" style={{ color: tabColors.dim }}>
                {formatSpeedForDisplay(entry.speed_mm_min ?? 1000, displayUnit, speedTimeUnit)}/{entry.power_percent ?? 50}%
              </span>
            )}
            {(active || hidden) && (
            <span
              role="button"
              tabIndex={0}
              aria-label={t('panels.layers.header.show')}
              className="flex-shrink-0"
              style={{ color: hidden ? tabColors.dim : undefined, opacity: hidden ? 0.6 : 1 }}
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
            )}
          </button>
        );
      })}
      <button
        onClick={() => void handleAddLayer()}
        aria-label={t('panels.layers.add_layer')}
        title={t('panels.layers.add_layer')}
        className="relative flex flex-shrink-0 items-center rounded-t-lg border border-b-0 px-2 py-1 opacity-80 hover:opacity-100"
        style={{ ...tabColors.inactive, marginLeft: project.layers.length > 0 ? -8 : 0, zIndex: 1 }}
      >
        <Plus size={12} />
      </button>
    </div>
  );
}
