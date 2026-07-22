import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Eye, EyeOff, Plus, Image as ImageIcon } from 'lucide-react';
import { useProjectStore } from '../../stores/projectStore';
import { useAppStore } from '../../stores/appStore';
import { projectService } from '../../services/projectService';
import { useNotificationStore } from '../../stores/notificationStore';
import { wrapBackendError } from '../../i18n/errors';
import { resolveDestinationLayer, normColor } from '../../stores/layerFamilyResolver';
import { PALETTE_COLORS } from '../../constants/palette';
import { formatSpeedForDisplay } from '../../utils/speedUnits';
import { useUiStore } from '../../stores/uiStore';
import { ContextMenu } from '../shared/ContextMenu';
import { buildLayerContextMenuItems } from './layerMenuItems';
import { buildLayerListHeaderMenuItems } from './LayerListHeaderMenu';
import { CutSettingsEditor } from './CutSettingsEditor';
import { displayLayerName } from './layerNaming';

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
  const selectObjects = useProjectStore((s) => s.selectObjects);
  const copyLayerSettings = useProjectStore((s) => s.copyLayerSettings);
  const pasteLayerSettings = useProjectStore((s) => s.pasteLayerSettings);
  const setAllLayersEnabled = useProjectStore((s) => s.setAllLayersEnabled);
  const setAllLayersVisible = useProjectStore((s) => s.setAllLayersVisible);
  const sortLayersCutLast = useProjectStore((s) => s.sortLayersCutLast);
  const flashLayer = useUiStore((s) => s.flashLayer);
  const layerSettingsClipboard = useUiStore((s) => s.layerSettingsClipboard);
  const displayUnit = useAppStore((s) => s.settings?.display_unit) === 'inches' ? 'inches' : 'mm';
  const speedTimeUnit = useAppStore((s) => s.settings?.speed_time_unit) === 'seconds' ? 'seconds' : 'minutes';
  const canvasDark = useAppStore((s) => s.settings?.dark_mode) === true;
  const [dragIndex, setDragIndex] = useState<number | null>(null);
  const [dropIndex, setDropIndex] = useState<number | null>(null);
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number; layerId: string } | null>(null);
  const [stripMenu, setStripMenu] = useState<{ x: number; y: number } | null>(null);
  const [editingLayerId, setEditingLayerId] = useState<string | null>(null);
  const [renamingLayerId, setRenamingLayerId] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState('');
  const [addMenu, setAddMenu] = useState<{ x: number; y: number } | null>(null);

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

  const handleTabClick = (layerId: string, shiftKey: boolean) => {
    if (shiftKey) {
      // Shift-click selects all objects on the layer (old row behavior).
      const layerObjIds = project.objects
        .filter((o) => o.layer_id === layerId)
        .map((o) => o.id)
        .reverse();
      selectObjects(layerObjIds);
      return;
    }
    if (hasSelection) {
      void reassignLayer(selectedObjectIds, layerId);
    }
    selectLayer(layerId);
  };

  const handleCommitRename = async () => {
    if (renamingLayerId && renameValue.trim()) {
      const renamed = await updateLayer(renamingLayerId, { name: renameValue.trim() });
      if (!renamed) return;
    }
    setRenamingLayerId(null);
  };

  const handleToggleVisible = async (layerId: string, visible: boolean) => {
    try {
      await projectService.setLayerVisible(layerId, visible);
      await loadProject({ invalidatePreview: true });
    } catch (error) {
      useNotificationStore.getState().push(wrapBackendError(String(error)), 'error');
    }
  };

  const createOrSelectLayerForColor = async (colorHex: string) => {
    const out = resolveDestinationLayer({
      project,
      requestedLayerId: null,
      pendingColor: colorHex,
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

  const handleAddLayer = async () => {
    const used = new Set(project.layers.map((l) => normColor(l.color_tag)));
    const nextColor = PALETTE_COLORS.find(
      (c) => !c.is_tool_layer && !used.has(normColor(c.hex)),
    );
    if (!nextColor) return;
    await createOrSelectLayerForColor(nextColor.hex);
  };

  const toolColors = PALETTE_COLORS.filter((c) => c.is_tool_layer);

  return (
    <div
      className="no-select flex items-end overflow-x-auto scrollbar-none px-3 pt-2 bg-bb-bg"
      onContextMenu={(e) => {
        // Right-click on the strip background: batch enable/show/sort menu.
        if (e.target === e.currentTarget) {
          e.preventDefault();
          setStripMenu({ x: e.clientX, y: e.clientY });
        }
      }}
    >
      {project.layers.map((layer, index) => {
        const activeLayerId = selectedLayerId ?? project.layers[0]?.id;
        const active = layer.id === activeLayerId;
        const entry = layer.entries[0];
        const hidden = layer.visible === false;
        return (
          <button
            key={layer.id}
            onClick={(e) => handleTabClick(layer.id, e.shiftKey)}
            onDoubleClick={() => {
              if (!layer.is_tool_layer) setEditingLayerId(layer.id);
            }}
            onContextMenu={(e) => {
              e.preventDefault();
              e.stopPropagation();
              selectLayer(layer.id);
              if (e.shiftKey) {
                // Shift-right-click: flash the layer's content instead of a menu.
                flashLayer(layer.id);
                return;
              }
              queueMicrotask(() => setContextMenu({ x: e.clientX, y: e.clientY, layerId: layer.id }));
            }}
            draggable
            data-testid="layer-tab"
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
            } ${layer.is_tool_layer ? 'border-dashed' : ''} ${dragIndex === index ? 'opacity-40' : ''}`}
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
            {layer.entries[0]?.operation === 'image' && (
              <ImageIcon size={10} className="flex-shrink-0" data-testid="tab-image-marker" />
            )}
            {renamingLayerId === layer.id ? (
              <input
                autoFocus
                className="w-24 rounded border border-bb-accent bg-bb-input px-0.5 text-xxs text-bb-text"
                value={renameValue}
                onChange={(e) => setRenameValue(e.target.value)}
                onBlur={() => { void handleCommitRename(); }}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') void handleCommitRename();
                  if (e.key === 'Escape') setRenamingLayerId(null);
                }}
                onClick={(e) => e.stopPropagation()}
                data-testid="tab-rename-input"
              />
            ) : (
              <span className="min-w-0 max-w-32 truncate" data-testid="tab-label">{displayLayerName(layer)}</span>
            )}
            {active && !layer.is_tool_layer && entry && (
              <span className="flex-shrink-0" data-testid="speed-power" style={{ color: tabColors.dim }}>
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
        onClick={(e) => {
          const rect = e.currentTarget.getBoundingClientRect();
          setAddMenu({ x: rect.left, y: rect.bottom + 2 });
        }}
        aria-label={t('panels.layers.add_layer')}
        title={t('panels.layers.add_layer')}
        data-testid="add-layer-tab"
        className="relative flex flex-shrink-0 items-center rounded-t-lg border border-b-0 px-2 py-1 opacity-80 hover:opacity-100"
        style={{ ...tabColors.inactive, marginLeft: project.layers.length > 0 ? -8 : 0, zIndex: 1 }}
      >
        <Plus size={12} />
      </button>

      {/* Per-layer context menu (ported from the layer table rows) */}
      {contextMenu && (() => {
        const ctxLayer = project.layers.find((l) => l.id === contextMenu.layerId);
        if (!ctxLayer) return null;
        return (
          <ContextMenu
            x={contextMenu.x}
            y={contextMenu.y}
            items={buildLayerContextMenuItems(t, ctxLayer, project.objects, {
              toggleEnabled: (layerId, enabled) => void updateLayer(layerId, { enabled }),
              toggleVisible: (layerId, visible) => void handleToggleVisible(layerId, visible),
              selectObjects,
              copySettings: (layer) => { if (!layer.is_tool_layer) copyLayerSettings(layer.id); },
              pasteSettings: (layerId) => void pasteLayerSettings(layerId),
              startRename: (layerId) => {
                const layer = project.layers.find((l) => l.id === layerId);
                setRenamingLayerId(layerId);
                setRenameValue(layer?.name ?? '');
              },
              hasClipboard: layerSettingsClipboard !== null && layerSettingsClipboard.length > 0,
              disableAllButThis: (layerId) => void setAllLayersEnabled({ kind: 'only_this_on', keep: layerId }),
              hideAllButThis: (layerId) => void setAllLayersVisible({ kind: 'only_this_on', keep: layerId }),
              flashLayer: (layerId) => flashLayer(layerId),
            })}
            onClose={() => setContextMenu(null)}
          />
        );
      })()}

      {/* + menu: new layer / tool layers (portal — the strip scroll-clips inline popovers) */}
      {addMenu && (
        <ContextMenu
          x={addMenu.x}
          y={addMenu.y}
          items={[
            {
              id: 'add-layer',
              label: t('panels.layers.add_layer'),
              onClick: () => void handleAddLayer(),
            },
            ...toolColors.map((c, i) => ({
              id: `add-tool-${i + 1}`,
              label: t(i === 0 ? 'panels.color_palette.colors.tool_1' : 'panels.color_palette.colors.tool_2'),
              onClick: () => void createOrSelectLayerForColor(c.hex),
            })),
          ]}
          onClose={() => setAddMenu(null)}
        />
      )}

      {/* Strip background menu: enable/show all, sort cuts last */}
      {stripMenu && (
        <ContextMenu
          x={stripMenu.x}
          y={stripMenu.y}
          items={buildLayerListHeaderMenuItems(t, {
            setAllLayersEnabled: (mode) => void setAllLayersEnabled(mode),
            setAllLayersVisible: (mode) => void setAllLayersVisible(mode),
            sortLayersCutLast: () => void sortLayersCutLast(),
          })}
          onClose={() => setStripMenu(null)}
        />
      )}

      {/* Double-click: full per-layer cut settings editor */}
      {editingLayerId && (
        <CutSettingsEditor
          layerId={editingLayerId}
          onClose={() => setEditingLayerId(null)}
          onSwitchLayer={(newId) => setEditingLayerId(newId)}
        />
      )}
    </div>
  );
}
