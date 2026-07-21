import { useRef, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { PALETTE_COLORS } from '../../constants/palette.js';
import { useProjectStore } from '../../stores/projectStore.js';
import { useUiStore } from '../../stores/uiStore.js';
import { projectService } from '../../services/projectService.js';
import {
  resolveDestinationLayer,
  normColor,
  type ContentKind,
  type NeedsBackendCreate,
} from '../../stores/layerFamilyResolver';
import { objectContentKind } from '../../commands/selectionContext';
import type { PhysicalDockZone } from '../../panels';

function getCurrentLayerColor(
  selectedObjectIds: string[],
  project: { layers: Array<{ id: string; color_tag: string }>; objects: Array<{ id: string; layer_id: string }> } | null,
): string | null {
  if (selectedObjectIds.length === 0 || !project) return null;

  const firstObjectId = selectedObjectIds[0];
  const firstObject = project.objects.find((o) => o.id === firstObjectId);
  if (!firstObject) return null;

  const layer = project.layers.find((l) => l.id === firstObject.layer_id);
  return layer?.color_tag ?? null;
}

function pickDefaultFamilyLayerId(
  project: {
    layers: Array<{ id: string; color_tag: string; operation?: string }>;
  },
  colorHex: string,
): string | null {
  const target = normColor(colorHex);
  const family = project.layers.filter(
    (l) => normColor(l.color_tag) === target,
  );
  if (family.length === 0) return null;
  return family.find((l) => l.operation !== 'image')?.id ?? family[0].id;
}

/** Compute a readable text color for a given background hex */
function contrastColor(hex: string): string {
  const r = parseInt(hex.slice(1, 3), 16);
  const g = parseInt(hex.slice(3, 5), 16);
  const b = parseInt(hex.slice(5, 7), 16);
  // Perceived brightness (ITU-R BT.601)
  const brightness = (r * 299 + g * 587 + b * 114) / 1000;
  return brightness > 128 ? '#000000' : '#FFFFFF';
}

const COLOR_NAME_KEYS: Record<string, string> = {
  Black: 'panels.color_palette.colors.black',
  Red: 'panels.color_palette.colors.red',
  Green: 'panels.color_palette.colors.green',
  Blue: 'panels.color_palette.colors.blue',
  Cyan: 'panels.color_palette.colors.cyan',
  Magenta: 'panels.color_palette.colors.magenta',
  Yellow: 'panels.color_palette.colors.yellow',
  Orange: 'panels.color_palette.colors.orange',
  Lilac: 'panels.color_palette.colors.lilac',
  'Sea Green': 'panels.color_palette.colors.sea_green',
  Pink: 'panels.color_palette.colors.pink',
  Moss: 'panels.color_palette.colors.moss',
  'Sky Blue': 'panels.color_palette.colors.sky_blue',
  Brown: 'panels.color_palette.colors.brown',
  Maroon: 'panels.color_palette.colors.maroon',
  'Dark Green': 'panels.color_palette.colors.dark_green',
  Navy: 'panels.color_palette.colors.navy',
  Olive: 'panels.color_palette.colors.olive',
  'Dark Cyan': 'panels.color_palette.colors.dark_cyan',
  'Dark Magenta': 'panels.color_palette.colors.dark_magenta',
  Coral: 'panels.color_palette.colors.coral',
  'Pale Green': 'panels.color_palette.colors.pale_green',
  Violet: 'panels.color_palette.colors.violet',
  Sand: 'panels.color_palette.colors.sand',
  'Steel Blue': 'panels.color_palette.colors.steel_blue',
  Plum: 'panels.color_palette.colors.plum',
  Gray: 'panels.color_palette.colors.gray',
  'Light Gray': 'panels.color_palette.colors.light_gray',
  'Dark Gray': 'panels.color_palette.colors.dark_gray',
  Gold: 'panels.color_palette.colors.gold',
  'Tool 1': 'panels.color_palette.colors.tool_1',
  'Tool 2': 'panels.color_palette.colors.tool_2',
};

/** Determine which zone the color_palette panel lives in. */
function useColorPaletteZone(): PhysicalDockZone | 'floating' | null {
  const panelLayout = useUiStore((s) => s.panelLayout);

  // Check floating first
  const isFloating = panelLayout.floatingPanels.some(
    (fp) => fp.panelId === 'color_palette' && !panelLayout.hiddenPanelIds.includes('color_palette'),
  );
  if (isFloating) return 'floating';

  // Check docked zones
  for (const zoneKey of Object.keys(panelLayout.zones) as PhysicalDockZone[]) {
    if (panelLayout.zones[zoneKey].panelIds.includes('color_palette')) {
      return zoneKey;
    }
  }

  return null;
}

export function ColorPalette(): React.JSX.Element {
  const { t } = useTranslation();
  const selectedObjectIds = useProjectStore((s) => s.selectedObjectIds);
  const project = useProjectStore((s) => s.project);
  const selectedLayerId = useProjectStore((s) => s.selectedLayerId);
  const selectLayer = useProjectStore((s) => s.selectLayer);
  const reassignLayer = useProjectStore((s) => s.reassignLayer);
  const updateLayer = useProjectStore((s) => s.updateLayer);
  const removeLayer = useProjectStore((s) => s.removeLayer);
  const loadProject = useProjectStore((s) => s.loadProject);

  const zone = useColorPaletteZone();
  const isVertical = zone === 'left' || zone === 'upper-right' || zone === 'lower-right';
  const isBottom = zone === 'bottom';

  const scrollRef = useRef<HTMLDivElement>(null);

  const scrollUp = useCallback(() => {
    scrollRef.current?.scrollBy({ top: -42, behavior: 'smooth' });
  }, []);

  const scrollDown = useCallback(() => {
    scrollRef.current?.scrollBy({ top: 42, behavior: 'smooth' });
  }, []);

  const pendingColor = useProjectStore((s) => s.pendingPaletteColor);
  const objectLayerColor = getCurrentLayerColor(selectedObjectIds, project);
  // Highlight priority tracks the active target family first, then
  // falls back to the selected object's family.
  const selectedLayerTag = project?.layers.find((l) => l.id === selectedLayerId)?.color_tag ?? null;
  const currentLayerColor = pendingColor ?? selectedLayerTag ?? objectLayerColor ?? (project?.layers[0]?.color_tag ?? null);
  const hasSelection = selectedObjectIds.length > 0;

  let regularIndex = 0;
  let toolIndex = 0;

  const swatchSize = isBottom
    ? 'flex-1 min-w-0 max-w-10 aspect-square'
    : 'w-10 h-10 flex-shrink-0';

  const swatches = PALETTE_COLORS.map((color) => {
    const isCurrentLayer =
      currentLayerColor !== null &&
      normColor(color.hex) === normColor(currentLayerColor);

    let borderClass: string;
    if (isCurrentLayer) {
      borderClass = 'border-2 border-white';
    } else if (color.is_tool_layer) {
      borderClass = 'border-dashed border-bb-border';
    } else {
      borderClass = 'border-bb-border/50';
    }

    const label = color.is_tool_layer
      ? `T${++toolIndex}`
      : String(regularIndex++).padStart(2, '0');
    const colorName = t(COLOR_NAME_KEYS[color.name] ?? 'panels.color_palette.colors.unknown');

    const handleClick = async (): Promise<void> => {
      if (!project) return;

      const matchingLayerId = pickDefaultFamilyLayerId(project, color.hex);

      if (!hasSelection) {
        // No objects selected
        if (matchingLayerId) {
          // Family exists — select the family's default row.
          selectLayer(matchingLayerId);
        } else {
          // No layer yet — defer creation until an object is actually drawn/imported
          useProjectStore.getState().setPendingPaletteColor(color.hex);
        }
        return;
      }

      // --- Objects are selected ---

      // Collect source layer info
      const selectedSet = new Set(selectedObjectIds);
      const sourceLayerIds = [...new Set(
        selectedObjectIds
          .map((id) => project.objects.find((o) => o.id === id)?.layer_id)
          .filter((id): id is string => !!id),
      )];

      // Partition the selection by effective content type. Raster
      // objects (and clones of rasters) must land on an image row
      // of the target color; vector/text/shape on a non-image row.
      // This prevents mixing content types on a single layer when
      // the user clicks a swatch with a heterogeneous selection.
      const selectedObjectRecords = selectedObjectIds
        .map((id) => project.objects.find((o) => o.id === id))
        .filter((o): o is NonNullable<typeof o> => !!o);
      const rasterSel = selectedObjectRecords.filter(
        (o) => objectContentKind(o.data, project.objects) === 'raster',
      );
      const vectorSel = selectedObjectRecords.filter(
        (o) => objectContentKind(o.data, project.objects) === 'non_raster',
      );

      // Fast path: single source layer, fully-selected, uniform
      // content kind, and no existing non-matching sibling on the
      // target color → just recolor the source layer in place for
      // a clean single-undo step. The backend invariant stays
      // happy because the layer's operation doesn't change.
      if (sourceLayerIds.length === 1) {
        const srcId = sourceLayerIds[0];
        const allOnSourceSelected = project.objects
          .filter((o) => o.layer_id === srcId)
          .every((o) => selectedSet.has(o.id));
        const uniformKind = rasterSel.length === 0 || vectorSel.length === 0;
        if (allOnSourceSelected && uniformKind) {
          const srcLayer = project.layers.find((l) => l.id === srcId);
          // Only take the shortcut when the target color has no
          // matching sibling yet — otherwise we'd end up with two
          // layers sharing color+type and the resolver would have
          // to disambiguate later.
          const targetNorm = normColor(color.hex);
          const targetHasSiblingOfKind = project.layers.some((l) => {
            const op = l.entries[0]?.operation ?? 'line';
            return normColor(l.color_tag) === targetNorm
              && ((rasterSel.length > 0 && op === 'image')
                || (vectorSel.length > 0 && op !== 'image'));
          });
          if (srcLayer && !targetHasSiblingOfKind) {
            await updateLayer(srcId, { color_tag: color.hex });
            selectLayer(srcId);
            return;
          }
        }
      }

      // General path: resolve each partition independently via the
      // layer-family resolver and reassign. Cleanup happens once at
      // the end so we don't remove a layer mid-flight that the
      // other partition still needs.
      const resolveAndCreate = async (
        contentKind: ContentKind,
      ): Promise<string> => {
        const latestProject = useProjectStore.getState().project!;
        let out = resolveDestinationLayer({
          project: latestProject,
          pendingColor: color.hex,
          selectedLayerId: latestProject.layers.find(
            (l) => normColor(l.color_tag) === normColor(color.hex),
          )?.id ?? null,
          contentKind,
        });
        if (out.kind === 'needs_create') {
          const req = out as NeedsBackendCreate;
          const created = await projectService.addLayer(req.suggestedName, req.operation);
          await projectService.updateLayer(created.id, { color_tag: req.colorTag });
          if (req.copyFrom) {
            const entry = created.entries[0];
            if (entry) {
              await projectService.updateCutEntry(created.id, entry.id, {
                speed_mm_min: req.copyFrom.entries[0]?.speed_mm_min ?? 1000,
                power_percent: req.copyFrom.entries[0]?.power_percent ?? 50,
              });
            }
          }
          await loadProject();
          // After loadProject the store has the new layer; re-resolve
          // to find it and return a concrete id.
          const after = useProjectStore.getState().project!;
          out = resolveDestinationLayer({
            project: after,
            pendingColor: color.hex,
            contentKind,
          });
        }
        if (out.kind === 'resolved') return out.layerId;
        throw new Error('Layer-family resolver did not return a concrete layer');
      };

      let lastTargetId: string | null = null;
      if (rasterSel.length > 0) {
        const dest = await resolveAndCreate('raster');
        const ok = await reassignLayer(rasterSel.map((o) => o.id), dest);
        if (!ok) return;
        lastTargetId = dest;
      }
      if (vectorSel.length > 0) {
        const dest = await resolveAndCreate('non_raster');
        const ok = await reassignLayer(vectorSel.map((o) => o.id), dest);
        if (!ok) return;
        lastTargetId = dest;
      }

      // Clean up any source layers that are now empty. Reload the
      // project first so local state reflects any layers the backend
      // already auto-cleaned during reassignment — this prevents the
      // double-delete error where we try to remove a layer that the
      // backend already removed.
      await useProjectStore.getState().loadProject();
      const updated = useProjectStore.getState().project;
      if (updated) {
        for (const srcId of sourceLayerIds) {
          const layerStillExists = updated.layers.some((l) => l.id === srcId);
          const layerIsEmpty = !updated.objects.some((o) => o.layer_id === srcId);
          if (layerStillExists && layerIsEmpty) {
            await removeLayer(srcId);
          }
        }
      }
      if (lastTargetId) {
        selectLayer(lastTargetId);
      }
    };

    return (
      <button
        key={color.hex}
        className={`relative ${swatchSize} rounded-sm border hover:scale-110 transition-transform ${borderClass} flex items-center justify-center`}
        style={{ backgroundColor: color.hex }}
        title={t('panels.color_palette.swatch_title', { label, color: colorName })}
        aria-label={colorName}
        disabled={!project}
        onClick={handleClick}
      >
        <span
          className="text-[10px] leading-none font-semibold select-none pointer-events-none"
          style={{ color: contrastColor(color.hex) }}
        >
          {label}
        </span>
      </button>
    );
  });

  if (isVertical) {
    return (
      <div className="flex flex-col items-center h-full bg-bb-panel py-0.5">
        <button
          className="flex-shrink-0 w-10 h-4 flex items-center justify-center text-bb-text-dim hover:text-bb-text"
          onClick={scrollUp}
          aria-label={t('panels.color_palette.scroll_up')}
        >
          <svg width="10" height="6" viewBox="0 0 10 6" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
            <polyline points="1,5 5,1 9,5" />
          </svg>
        </button>
        <div
          ref={scrollRef}
          className="flex-1 min-h-0 overflow-y-hidden flex flex-col items-center gap-1.5 px-0.5"
        >
          {swatches}
        </div>
        <button
          className="flex-shrink-0 w-10 h-4 flex items-center justify-center text-bb-text-dim hover:text-bb-text"
          onClick={scrollDown}
          aria-label={t('panels.color_palette.scroll_down')}
        >
          <svg width="10" height="6" viewBox="0 0 10 6" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
            <polyline points="1,1 5,5 9,1" />
          </svg>
        </button>
      </div>
    );
  }

  // Horizontal layout (bottom zone, floating)
  return (
    <div className={`flex h-full min-w-0 items-center flex-nowrap gap-1.5 px-2 py-1 bg-bb-panel overflow-y-hidden scrollbar-none ${isBottom ? 'overflow-hidden' : 'overflow-x-auto'}`}>
      {swatches}
    </div>
  );
}
