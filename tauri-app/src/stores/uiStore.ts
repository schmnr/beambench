import { create } from 'zustand';
import type {
  CutEntryTemplate,
  DockOptions,
  NestOptions,
  Point2D,
  TextAlignment,
  TextAlignmentV,
  TextCirclePlacement,
  TextLayoutMode,
  TextTransformStyle,
} from '../types/project';
import type { OffsetPreviewPath } from '../types/vector';
import type { PhysicalDockZone, PanelLayoutState, FloatingPanelState, ToolbarId } from '../panels';
import { createDefaultLayout, getPanelById, normalizeToolbarVisibility } from '../panels';
import { DEFAULT_GRID_SPACING_MM, MIN_ZOOM, MAX_ZOOM, ZOOM_STEP } from '../canvas/constants';
import { appService } from '../services/appService';
import { commitPendingTextEdit, hasPendingTextEdit, isNewEmptyText } from '../canvas/textEditSession';
import { useProjectStore } from './projectStore';
import { useMeasurementStore } from './measurementStore';

export type ToolType = 'select' | 'rect' | 'ellipse' | 'star' | 'text' | 'node' | 'line' | 'polygon' | 'trim' | 'tabs' | 'radius' | 'measure' | 'laser_position' | 'two_point_rotate_scale' | 'warp_selection' | 'deform_selection';

export type NodeSubMode =
  | 'select' | 'insert' | 'delete_node' | 'break'
  | 'delete_segment' | 'to_line' | 'to_smooth' | 'to_corner'
  | 'insert_midpoint' | 'align' | 'trim' | 'extend'
  | 'close_open' | 'auto_join';

export type ViewStyle = 'wireframe_coarse' | 'wireframe_smooth' | 'filled_coarse' | 'filled_smooth';

export interface ViewStyleRenderOptions {
  antialiasing: boolean;
  filledRendering: boolean;
}

export function renderOptionsFromViewStyle(style: ViewStyle): ViewStyleRenderOptions {
  return {
    antialiasing: style.endsWith('_smooth'),
    filledRendering: style.startsWith('filled_'),
  };
}

export function viewStyleFromRenderOptions(options: Partial<ViewStyleRenderOptions>): ViewStyle {
  const fillMode = options.filledRendering ? 'filled' : 'wireframe';
  const smoothing = options.antialiasing ? 'smooth' : 'coarse';
  return `${fillMode}_${smoothing}` as ViewStyle;
}

/** Defaults applied when the Text tool creates a new text object. */
export interface TextDefaults {
  font_family: string;
  font_size_mm: number;
  alignment: TextAlignment;
  alignment_v: TextAlignmentV;
  bold: boolean;
  italic: boolean;
  upper_case: boolean;
  welded: boolean;
  h_spacing: number;
  v_spacing: number;
  layout_mode: TextLayoutMode;
  on_path: boolean;
  path_offset: number;
  distort: boolean;
  bend_radius: number;
  transform_style: TextTransformStyle;
  transform_curve: number;
  circle_placement: TextCirclePlacement;
}

export const DEFAULT_TEXT_DEFAULTS: TextDefaults = {
  font_family: 'Arial',
  font_size_mm: 25,
  alignment: 'center',
  alignment_v: 'middle',
  bold: false,
  italic: false,
  upper_case: false,
  welded: false,
  h_spacing: 0,
  v_spacing: 0,
  layout_mode: 'straight',
  on_path: false,
  path_offset: 0,
  distort: false,
  bend_radius: 0,
  transform_style: 'none',
  transform_curve: 0,
  circle_placement: 'top_outside',
};

export const DEFAULT_DOCK_SETTINGS: DockOptions = {
  moveAsGroup: false,
  lockInnerObjects: false,
  paddingMm: 0,
};

export const DEFAULT_NEST_SETTINGS: NestOptions = {
  paddingMm: 0,
  allowRotation: true,
  allowMirror: false,
  lockInnerObjects: false,
  timeLimitMs: 15000,
  rotationStepDeg: 15,
};

export interface JobOptions {
  cut_selected_graphics: boolean;
  use_selection_origin: boolean;
}

export const DEFAULT_JOB_OPTIONS: JobOptions = {
  cut_selected_graphics: false,
  use_selection_origin: false,
};

const EMPTY_TEXT_EDIT_STATE = {
  textEditObjectId: null,
  textEditClickPos: null,
  textEditMode: null,
  textEditCaretIndex: null,
} as const;

interface UiStoreState {
  // Panel layout
  panelLayout: PanelLayoutState;
  nextFloatingZIndex: number;

  activeTool: ToolType;
  zoom: number;

  // Viewport
  viewportOffset: Point2D;

  // Grid & snap
  gridVisible: boolean;
  snapToGrid: boolean;
  snapToObjects: boolean;
  gridSpacingMm: number;
  nudgeStepMm: number;
  nudgeStepFineMm: number;
  nudgeStepCoarseMm: number;

  // Cursor
  cursorWorldPos: Point2D | null;

  // View style
  viewStyle: ViewStyle;

  // Side panels
  sidePanelsVisible: boolean;

  // Mode toggles
  rotaryEnabled: boolean;
  printAndCutEnabled: boolean;

  // Camera window
  cameraWindowOpen: boolean;

  // Session-only job controls. These are intentionally not serialized.
  jobOptions: JobOptions;
  updateJobOptions: (partial: Partial<JobOptions>) => void;

  // Clipboard state (for menu disabled gating)
  hasClipboard: boolean;
  setHasClipboard: (has: boolean) => void;

  // M4: app-scoped layer-settings clipboard. Cleared on project create/open/close/replace by
  // projectStore. Use `setLayerSettingsClipboard(null)` to clear explicitly.
  layerSettingsClipboard: CutEntryTemplate[] | null;
  setLayerSettingsClipboard: (entries: CutEntryTemplate[] | null) => void;

  // M4: transient flash highlight on the canvas. Set by projectStore.flashLayer; auto-clears
  // after FLASH_DURATION_MS.
  flashedLayerId: string | null;
  flashLayer: (layerId: string) => void;

  // Show last laser position on canvas
  showLastPosition: boolean;
  toggleShowLastPosition: () => void;

  // Lock aspect ratio (global state, shared between toolbar and canvas)
  lockAspect: boolean;
  setLockAspect: (locked: boolean) => void;
  toggleLockAspect: () => void;

  // Default corner radius for rectangle tool
  defaultCornerRadius: number;
  setDefaultCornerRadius: (r: number) => void;

  // Text tool defaults — applied when creating new text objects
  textDefaults: TextDefaults;
  updateTextDefaults: (partial: Partial<TextDefaults>) => void;

  // Radius tool value (per-session override; null = use persisted setting)
  radiusToolValue: number | null;
  setRadiusToolValue: (v: number | null) => void;

  // Arrangement dialog memory
  dockSettings: DockOptions;
  updateDockSettings: (partial: Partial<DockOptions>) => void;
  nestSettings: NestOptions;
  updateNestSettings: (partial: Partial<NestOptions>) => void;
  nestingInProgress: boolean;
  setNestingInProgress: (inProgress: boolean) => void;
  moveWindowJogDistanceMm: number;
  setMoveWindowJogDistanceMm: (distanceMm: number) => void;
  moveWindowJogFeedRateMmMin: number;
  setMoveWindowJogFeedRateMmMin: (feedRateMmMin: number) => void;

  // Toolbar submenu memory
  lastShapeSubTool: string;
  setLastShapeSubTool: (id: string) => void;
  lastBooleanOp: string;
  setLastBooleanOp: (id: string) => void;

  // App-level dialogs whose state must be visible to native menu state sync
  showNotesDialog: boolean;
  setShowNotesDialog: (show: boolean) => void;
  toggleNotesDialog: () => void;

  // Node editing info (displayed in StatusBar)
  nodeEditNodeCount: number;
  nodeSubMode: NodeSubMode;

  // Inline text editing — when non-null, the text overlay is shown for this object
  textEditObjectId: string | null;
  /** World-space click position where the text tool was activated (for textarea placement). */
  textEditClickPos: Point2D | null;
  /** How the edit session was initiated: 'new' (created), 'tool-click' (text tool on existing), 'double-click' (select tool). */
  textEditMode: 'new' | 'tool-click' | 'double-click' | null;
  /** Caret position for tool-click on straight text; null → select-all fallback. */
  textEditCaretIndex: number | null;
  setTextEditObjectId: (objectId: string | null) => void;
  /** Begin or update a text edit session. Commits pending edit when switching objects. */
  beginTextEditSession: (objectId: string, mode: 'new' | 'tool-click' | 'double-click', clickPos?: Point2D, caretIndex?: number) => void;

  // Set Start Point pick mode — when non-null, the next canvas click
  // provides the world coordinate for setStartPoint on this object ID
  pendingStartPointObjectId: string | null;
  setPendingStartPoint: (objectId: string | null) => void;

  // Guide path pick mode — when non-null, the next canvas click on a
  // vector/shape selects it as the guide path for this text object ID
  pendingGuidePathTextId: string | null;
  setPendingGuidePathText: (objectId: string | null) => void;

  // Offset dialog live preview — dashed ghost paths (world coords) rendered on
  // the canvas while the Offset dialog is open. Null when no preview is active.
  offsetPreview: OffsetPreviewPath[] | null;
  setOffsetPreview: (paths: OffsetPreviewPath[] | null) => void;

  // Panel layout actions
  setPanelLayout: (layout: PanelLayoutState) => void;
  setZoneActiveTab: (zone: PhysicalDockZone, tabId: string) => void;
  showPanel: (panelId: string) => void;
  togglePanelVisibility: (panelId: string) => void;
  setToolbarVisibility: (toolbarId: ToolbarId, visible: boolean) => void;
  toggleToolbarVisibility: (toolbarId: ToolbarId) => void;
  setUpperSplitRatio: (ratio: number) => void;
  setRightPanelWidth: (width: number) => void;
  setLeftPanelWidth: (width: number) => void;
  setBottomPanelHeight: (height: number) => void;
  resetLayout: () => void;

  // Floating panel actions
  floatPanel: (panelId: string, x: number, y: number, w: number, h: number) => void;
  dockPanel: (panelId: string, targetZone: PhysicalDockZone, insertIndex?: number) => void;
  moveFloatingPanel: (panelId: string, x: number, y: number) => void;
  resizeFloatingPanel: (panelId: string, w: number, h: number) => void;
  bringToFront: (panelId: string) => void;
  closeFloatingPanel: (panelId: string) => void;
  movePanelBetweenZones: (panelId: string, fromZone: PhysicalDockZone, toZone: PhysicalDockZone, insertIndex?: number) => void;
  reorderPanelInZone: (panelId: string, zone: PhysicalDockZone, newIndex: number) => void;

  // Tool actions
  setActiveTool: (tool: ToolType) => void;

  // Zoom actions
  setZoom: (zoom: number) => void;
  zoomIn: () => void;
  zoomOut: () => void;
  zoomBy: (factor: number) => void;

  // Viewport actions
  setViewportOffset: (offset: Point2D) => void;
  panBy: (dx: number, dy: number) => void;
  zoomToFit: (offset: Point2D, zoom: number) => void;

  // Grid actions
  toggleGrid: () => void;
  toggleSnap: () => void;
  toggleSnapToObjects: () => void;
  setGridSpacing: (spacing: number) => void;
  setNudgeSteps: (steps: {
    normal: number;
    fine: number;
    coarse: number;
  }) => void;

  // Cursor actions
  setCursorWorldPos: (pos: Point2D | null) => void;

  // View style actions
  setViewStyle: (style: ViewStyle) => void;
  toggleFilledRendering: () => void;

  // Side panels actions
  toggleSidePanels: () => void;

  // Mode toggle actions
  toggleRotary: () => void;
  togglePrintAndCut: () => void;

  // Camera window actions
  toggleCameraWindow: () => void;

  // Node editing actions
  setNodeEditNodeCount: (count: number) => void;
  setNodeSubMode: (mode: NodeSubMode) => void;
}

const clampZoom = (z: number) => Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, z));

/** M4: how long a flash highlight stays visible before auto-clearing. */
const FLASH_DURATION_MS = 600;

/** Fix active tab after removing a panel from a zone. */
function fixActiveTab(zone: { panelIds: string[]; activeTab: string }, removedId: string, hidden: string[]): { panelIds: string[]; activeTab: string } {
  if (zone.activeTab !== removedId) return zone;
  const firstVisible = zone.panelIds.filter((id) => id !== removedId).find((id) => !hidden.includes(id));
  return { ...zone, activeTab: firstVisible ?? '' };
}

function sanitizePanelLayout(layout: PanelLayoutState): PanelLayoutState {
  const hiddenPanelIds = [...new Set(layout.hiddenPanelIds.filter((id) => getPanelById(id)))];
  const zones = { ...layout.zones };
  for (const zoneKey of Object.keys(zones) as PhysicalDockZone[]) {
    const zone = zones[zoneKey];
    const panelIds = zone.panelIds.filter((id) => getPanelById(id));
    zones[zoneKey] = {
      panelIds,
      activeTab: panelIds.includes(zone.activeTab) && !hiddenPanelIds.includes(zone.activeTab)
        ? zone.activeTab
        : panelIds.find((id) => !hiddenPanelIds.includes(id)) ?? '',
    };
  }

  return {
    ...layout,
    zones,
    hiddenPanelIds,
    floatingPanels: layout.floatingPanels.filter((fp) => getPanelById(fp.panelId)),
    toolbarVisibility: normalizeToolbarVisibility(layout.toolbarVisibility),
  };
}

function showMeasurementPanel(
  layout: PanelLayoutState,
  nextFloatingZIndex: number,
): { layout: PanelLayoutState; nextFloatingZIndex: number } {
  const panelId = 'measurement';
  const hiddenPanelIds = layout.hiddenPanelIds.filter((id) => id !== panelId);
  const floatingPanels = layout.floatingPanels;
  const floatingPanel = floatingPanels.find((panel) => panel.panelId === panelId);
  if (floatingPanel) {
    return {
      layout: {
        ...layout,
        sidePanelsVisible: true,
        hiddenPanelIds,
        floatingPanels: floatingPanels.map((panel) =>
          panel.panelId === panelId ? { ...panel, zIndex: nextFloatingZIndex } : panel,
        ),
      },
      nextFloatingZIndex: nextFloatingZIndex + 1,
    };
  }

  const existingZoneKey = (Object.keys(layout.zones) as PhysicalDockZone[]).find((zoneKey) =>
    layout.zones[zoneKey].panelIds.includes(panelId),
  );
  if (existingZoneKey) {
    return {
      layout: {
        ...layout,
        sidePanelsVisible: true,
        hiddenPanelIds,
        zones: {
          ...layout.zones,
          [existingZoneKey]: {
            ...layout.zones[existingZoneKey],
            activeTab: panelId,
          },
        },
      },
      nextFloatingZIndex,
    };
  }

  const targetZone: PhysicalDockZone = 'upper-right';
  return {
    layout: {
      ...layout,
      sidePanelsVisible: true,
      hiddenPanelIds,
      zones: {
        ...layout.zones,
        [targetZone]: {
          ...layout.zones[targetZone],
          panelIds: [...layout.zones[targetZone].panelIds, panelId],
          activeTab: panelId,
        },
      },
    },
    nextFloatingZIndex,
  };
}

export const useUiStore = create<UiStoreState>((set) => ({
  panelLayout: createDefaultLayout(),
  nextFloatingZIndex: 1,
  activeTool: 'select',
  zoom: 100,

  // Default viewport offset: center of 400x400mm bed
  viewportOffset: { x: 200, y: 200 },

  gridVisible: true,
  snapToGrid: false,
  snapToObjects: false,
  gridSpacingMm: DEFAULT_GRID_SPACING_MM,
  nudgeStepMm: 5,
  nudgeStepFineMm: 1,
  nudgeStepCoarseMm: 20,

  viewStyle: 'wireframe_smooth',
  sidePanelsVisible: true,

  cursorWorldPos: null,
  rotaryEnabled: false,
  printAndCutEnabled: false,
  cameraWindowOpen: false,
  jobOptions: { ...DEFAULT_JOB_OPTIONS },
  hasClipboard: false,
  layerSettingsClipboard: null,
  flashedLayerId: null,
  showLastPosition: false,
  lockAspect: false,
  defaultCornerRadius: 0,
  textDefaults: { ...DEFAULT_TEXT_DEFAULTS },
  radiusToolValue: null,
  dockSettings: { ...DEFAULT_DOCK_SETTINGS },
  nestSettings: { ...DEFAULT_NEST_SETTINGS },
  nestingInProgress: false,
  moveWindowJogDistanceMm: 10,
  moveWindowJogFeedRateMmMin: 1000,
  lastShapeSubTool: 'rect',
  lastBooleanOp: 'union',
  showNotesDialog: false,
  nodeEditNodeCount: 0,
  nodeSubMode: 'select' as NodeSubMode,
  textEditObjectId: null,
  textEditClickPos: null,
  textEditMode: null,
  textEditCaretIndex: null,
  pendingStartPointObjectId: null,
  pendingGuidePathTextId: null,
  offsetPreview: null,

  setPanelLayout: (layout) => {
    const sanitized = sanitizePanelLayout(layout);
    set({
      panelLayout: sanitized,
      sidePanelsVisible: sanitized.sidePanelsVisible,
    });
  },

  setShowNotesDialog: (show) => set({ showNotesDialog: show }),
  toggleNotesDialog: () => set((s) => ({ showNotesDialog: !s.showNotesDialog })),

  setZoneActiveTab: (zone, tabId) =>
    set((s) => ({
      panelLayout: {
        ...s.panelLayout,
        zones: {
          ...s.panelLayout.zones,
          [zone]: { ...s.panelLayout.zones[zone], activeTab: tabId },
        },
      },
    })),

  showPanel: (panelId) =>
    set((s) => {
      const hidden = s.panelLayout.hiddenPanelIds;
      const isHidden = hidden.includes(panelId);
      const newLayout: PanelLayoutState = {
        ...s.panelLayout,
        hiddenPanelIds: isHidden ? hidden.filter((id) => id !== panelId) : hidden,
      };
      const floatingIndex = newLayout.floatingPanels.findIndex((fp) => fp.panelId === panelId);
      if (floatingIndex >= 0) {
        const nextZ = s.nextFloatingZIndex;
        newLayout.floatingPanels = newLayout.floatingPanels.map((fp, index) =>
          index === floatingIndex ? { ...fp, zIndex: nextZ } : fp,
        );
        appService.persistLayout(newLayout);
        return { panelLayout: newLayout, nextFloatingZIndex: nextZ + 1 };
      }

      const def = getPanelById(panelId);
      if (def) {
        const existingZoneKey = (Object.keys(newLayout.zones) as PhysicalDockZone[]).find(
          (zk) => newLayout.zones[zk].panelIds.includes(panelId),
        );
        if (!existingZoneKey) {
          if (def.defaultZone === 'floating') {
            const size = def.defaultFloatSize ?? { w: 384, h: 300 };
            const nextZ = s.nextFloatingZIndex;
            newLayout.floatingPanels = [
              ...newLayout.floatingPanels,
              { panelId, x: 100, y: 100, width: size.w, height: size.h, zIndex: nextZ },
            ];
            appService.persistLayout(newLayout);
            return { panelLayout: newLayout, nextFloatingZIndex: nextZ + 1 };
          }
          const targetZone = def.defaultZone as PhysicalDockZone;
          const zone = newLayout.zones[targetZone];
          if (zone) {
            newLayout.zones = {
              ...newLayout.zones,
              [targetZone]: { ...zone, panelIds: [...zone.panelIds, panelId], activeTab: panelId },
            };
          }
        } else {
          const zone = newLayout.zones[existingZoneKey];
          newLayout.zones = {
            ...newLayout.zones,
            [existingZoneKey]: { ...zone, activeTab: panelId },
          };
        }
      }

      appService.persistLayout(newLayout);
      return { panelLayout: newLayout };
    }),

  togglePanelVisibility: (panelId) =>
    set((s) => {
      const hidden = s.panelLayout.hiddenPanelIds;
      const isHidden = hidden.includes(panelId);
      const newHidden = isHidden ? hidden.filter((id) => id !== panelId) : [...hidden, panelId];

      // Keep floating entries on hide — position/size is preserved in floatingPanels.
      // FloatingPanelLayer filters out hidden panels before rendering.
      const newLayout: PanelLayoutState = { ...s.panelLayout, hiddenPanelIds: newHidden };

      if (isHidden) {
        // Showing: if the panel is already in floatingPanels, unhide it and bring to front
        const wasFloating = newLayout.floatingPanels.some((fp) => fp.panelId === panelId);
        if (wasFloating) {
          const nextZ = s.nextFloatingZIndex;
          newLayout.floatingPanels = newLayout.floatingPanels.map((fp) =>
            fp.panelId === panelId ? { ...fp, zIndex: nextZ } : fp,
          );
          appService.persistLayout(newLayout);
          return { panelLayout: newLayout, nextFloatingZIndex: nextZ + 1 };
        }

        const def = getPanelById(panelId);
        // If it defaults to floating (and has no saved float entry), create a new one
        if (def?.defaultZone === 'floating') {
          const size = def.defaultFloatSize ?? { w: 384, h: 300 };
          const nextZ = s.nextFloatingZIndex;
          newLayout.floatingPanels = [
            ...newLayout.floatingPanels,
            { panelId, x: 100, y: 100, width: size.w, height: size.h, zIndex: nextZ },
          ];
          appService.persistLayout(newLayout);
          return { panelLayout: newLayout, nextFloatingZIndex: nextZ + 1 };
        }

        // If unhiding a docked panel that is not in any zone, insert it into its defaultZone.
        // If it IS already in a zone, activate its tab so the user actually sees it.
        if (def) {
          const existingZoneKey = (Object.keys(newLayout.zones) as PhysicalDockZone[]).find(
            (zk) => newLayout.zones[zk].panelIds.includes(panelId),
          );
          if (!existingZoneKey) {
            const targetZone = def.defaultZone as PhysicalDockZone;
            const zone = newLayout.zones[targetZone];
            if (zone) {
              newLayout.zones = {
                ...newLayout.zones,
                [targetZone]: { ...zone, panelIds: [...zone.panelIds, panelId], activeTab: panelId },
              };
            }
          } else {
            const zone = newLayout.zones[existingZoneKey];
            newLayout.zones = {
              ...newLayout.zones,
              [existingZoneKey]: { ...zone, activeTab: panelId },
            };
          }
        }
      }

      // If hiding the active tab in a zone, switch to the first visible tab in that zone
      const newZones = { ...newLayout.zones };
      if (!isHidden) {
        for (const zoneKey of Object.keys(newZones) as PhysicalDockZone[]) {
          const zone = newZones[zoneKey];
          if (zone.activeTab === panelId) {
            const firstVisible = zone.panelIds.find((id) => !newHidden.includes(id));
            newZones[zoneKey] = { ...zone, activeTab: firstVisible ?? '' };
          }
        }
      }

      const layout = {
        ...newLayout,
        zones: newZones,
      };
      appService.persistLayout(layout);
      return {
        panelLayout: layout,
        ...(!isHidden && panelId === 'camera' ? { cameraWindowOpen: false } : {}),
      };
    }),

  setToolbarVisibility: (toolbarId, visible) =>
    set((s) => {
      const layout: PanelLayoutState = {
        ...s.panelLayout,
        toolbarVisibility: {
          ...normalizeToolbarVisibility(s.panelLayout.toolbarVisibility),
          [toolbarId]: visible,
        },
      };
      appService.persistLayout(layout);
      return { panelLayout: layout };
    }),

  toggleToolbarVisibility: (toolbarId) =>
    set((s) => {
      const current = normalizeToolbarVisibility(s.panelLayout.toolbarVisibility);
      const layout: PanelLayoutState = {
        ...s.panelLayout,
        toolbarVisibility: {
          ...current,
          [toolbarId]: !current[toolbarId],
        },
      };
      appService.persistLayout(layout);
      return { panelLayout: layout };
    }),

  setUpperSplitRatio: (ratio) =>
    set((s) => ({
      panelLayout: {
        ...s.panelLayout,
        upperSplitRatio: Math.max(0.2, Math.min(0.8, ratio)),
      },
    })),

  setRightPanelWidth: (width) =>
    set((s) => ({
      panelLayout: {
        ...s.panelLayout,
        rightPanelWidth: Math.max(180, Math.min(600, width)),
      },
    })),

  setLeftPanelWidth: (width) =>
    set((s) => ({
      panelLayout: { ...s.panelLayout, leftPanelWidth: Math.max(150, Math.min(600, width)) },
    })),

  setBottomPanelHeight: (height) =>
    set((s) => ({
      panelLayout: { ...s.panelLayout, bottomPanelHeight: Math.max(36, Math.min(500, height)) },
    })),

  resetLayout: () => {
    const layout = createDefaultLayout();
    appService.persistLayout(layout);
    set({ panelLayout: layout, nextFloatingZIndex: 1, sidePanelsVisible: true, cameraWindowOpen: false });
  },

  // --- Floating panel actions ---

  floatPanel: (panelId, x, y, w, h) =>
    set((s) => {
      const newZones = { ...s.panelLayout.zones };
      const hidden = s.panelLayout.hiddenPanelIds;

      // Remove from whichever dock zone it's in, recording the origin zone and tab index
      let originZone: string | undefined;
      let originIndex: number | undefined;
      for (const zoneKey of Object.keys(newZones) as PhysicalDockZone[]) {
        const zone = newZones[zoneKey];
        const idx = zone.panelIds.indexOf(panelId);
        if (idx >= 0) {
          originZone = zoneKey;
          originIndex = idx;
          const newPanelIds = zone.panelIds.filter((id) => id !== panelId);
          newZones[zoneKey] = fixActiveTab({ ...zone, panelIds: newPanelIds }, panelId, hidden);
        }
      }

      const nextZ = s.nextFloatingZIndex;
      const fp: FloatingPanelState = { panelId, x, y, width: w, height: h, zIndex: nextZ, originZone, originIndex };
      const newFloating = [...s.panelLayout.floatingPanels.filter((f) => f.panelId !== panelId), fp];

      // Remove from hidden if present
      const newHidden = hidden.filter((id) => id !== panelId);

      const layout: PanelLayoutState = {
        ...s.panelLayout,
        zones: newZones,
        floatingPanels: newFloating,
        hiddenPanelIds: newHidden,
      };
      appService.persistLayout(layout);
      return { panelLayout: layout, nextFloatingZIndex: nextZ + 1 };
    }),

  dockPanel: (panelId, targetZone, insertIndex) =>
    set((s) => {
      // Remove from floatingPanels
      const newFloating = s.panelLayout.floatingPanels.filter((fp) => fp.panelId !== panelId);

      // Remove from hidden
      const newHidden = s.panelLayout.hiddenPanelIds.filter((id) => id !== panelId);

      // Add to target zone
      const newZones = { ...s.panelLayout.zones };
      const zone = newZones[targetZone];
      const panelIds = zone.panelIds.filter((id) => id !== panelId);
      const idx = insertIndex !== undefined ? Math.min(insertIndex, panelIds.length) : panelIds.length;
      panelIds.splice(idx, 0, panelId);
      newZones[targetZone] = { panelIds, activeTab: panelId };

      const layout: PanelLayoutState = {
        ...s.panelLayout,
        zones: newZones,
        floatingPanels: newFloating,
        hiddenPanelIds: newHidden,
      };
      appService.persistLayout(layout);
      return { panelLayout: layout };
    }),

  moveFloatingPanel: (panelId, x, y) =>
    set((s) => {
      const newFloating = s.panelLayout.floatingPanels.map((fp) =>
        fp.panelId === panelId ? { ...fp, x, y } : fp,
      );
      const layout = { ...s.panelLayout, floatingPanels: newFloating };
      appService.persistLayout(layout);
      return { panelLayout: layout };
    }),

  resizeFloatingPanel: (panelId, w, h) =>
    set((s) => {
      const def = getPanelById(panelId);
      const minW = def?.minFloatSize?.w ?? 200;
      const minH = def?.minFloatSize?.h ?? 150;
      const newFloating = s.panelLayout.floatingPanels.map((fp) =>
        fp.panelId === panelId ? { ...fp, width: Math.max(minW, w), height: Math.max(minH, h) } : fp,
      );
      const layout = { ...s.panelLayout, floatingPanels: newFloating };
      appService.persistLayout(layout);
      return { panelLayout: layout };
    }),

  bringToFront: (panelId) =>
    set((s) => {
      const nextZ = s.nextFloatingZIndex;
      const newFloating = s.panelLayout.floatingPanels.map((fp) =>
        fp.panelId === panelId ? { ...fp, zIndex: nextZ } : fp,
      );
      const layout = { ...s.panelLayout, floatingPanels: newFloating };
      appService.persistLayout(layout);
      return { panelLayout: layout, nextFloatingZIndex: nextZ + 1 };
    }),

  closeFloatingPanel: (panelId) =>
    set((s) => {
      // Keep floating entry (position/size preserved) — just hide the panel
      const newHidden = s.panelLayout.hiddenPanelIds.includes(panelId)
        ? s.panelLayout.hiddenPanelIds
        : [...s.panelLayout.hiddenPanelIds, panelId];
      const layout: PanelLayoutState = { ...s.panelLayout, hiddenPanelIds: newHidden };
      appService.persistLayout(layout);
      return {
        panelLayout: layout,
        ...(panelId === 'camera' ? { cameraWindowOpen: false } : {}),
      };
    }),

  movePanelBetweenZones: (panelId, fromZone, toZone, insertIndex) =>
    set((s) => {
      const newZones = { ...s.panelLayout.zones };
      const hidden = s.panelLayout.hiddenPanelIds;

      // Remove from source zone
      const fromState = newZones[fromZone];
      const fromIds = fromState.panelIds.filter((id) => id !== panelId);
      newZones[fromZone] = fixActiveTab({ ...fromState, panelIds: fromIds }, panelId, hidden);

      // Add to target zone
      const toState = newZones[toZone];
      const toIds = toState.panelIds.filter((id) => id !== panelId);
      const idx = insertIndex !== undefined ? Math.min(insertIndex, toIds.length) : toIds.length;
      toIds.splice(idx, 0, panelId);
      newZones[toZone] = { panelIds: toIds, activeTab: panelId };

      const layout: PanelLayoutState = { ...s.panelLayout, zones: newZones };
      appService.persistLayout(layout);
      return { panelLayout: layout };
    }),

  reorderPanelInZone: (panelId, zone, newIndex) =>
    set((s) => {
      const newZones = { ...s.panelLayout.zones };
      const zoneState = newZones[zone];
      const ids = zoneState.panelIds.filter((id) => id !== panelId);
      const idx = Math.min(newIndex, ids.length);
      ids.splice(idx, 0, panelId);
      newZones[zone] = { ...zoneState, panelIds: ids };

      const layout: PanelLayoutState = { ...s.panelLayout, zones: newZones };
      appService.persistLayout(layout);
      return { panelLayout: layout };
    }),

  setTextEditObjectId: (objectId) => {
    if (!objectId) {
      const prevId = useUiStore.getState().textEditObjectId;
      const prevMode = useUiStore.getState().textEditMode;
      const shouldDelete = isNewEmptyText(prevId, prevMode);
      if (!hasPendingTextEdit()) {
        set(EMPTY_TEXT_EDIT_STATE);
        if (shouldDelete && prevId) {
          void useProjectStore.getState().removeObject(prevId);
        }
        return;
      }
      void (async () => {
        if (await commitPendingTextEdit()) {
          set(EMPTY_TEXT_EDIT_STATE);
          if (shouldDelete && prevId) {
            await useProjectStore.getState().removeObject(prevId);
          }
        }
      })();
      return;
    }

    set({
      ...EMPTY_TEXT_EDIT_STATE,
      textEditObjectId: objectId,
    });
  },

  beginTextEditSession: (objectId, mode, clickPos, caretIndex) => {
    const current = useUiStore.getState().textEditObjectId;
    const currentMode = useUiStore.getState().textEditMode;
    if (current === objectId) {
      // Re-clicking same text: update mode + caret in-place, no commit/remount
      set({ textEditMode: mode, textEditCaretIndex: caretIndex ?? null });
      return;
    }
    const shouldDelete = isNewEmptyText(current, currentMode);
    if (!hasPendingTextEdit()) {
      set({
        textEditObjectId: objectId,
        textEditClickPos: clickPos ?? null,
        textEditMode: mode,
        textEditCaretIndex: caretIndex ?? null,
      });
      if (shouldDelete && current) {
        void useProjectStore.getState().removeObject(current);
      }
      return;
    }
    void (async () => {
      if (await commitPendingTextEdit()) {
        set({
          textEditObjectId: objectId,
          textEditClickPos: clickPos ?? null,
          textEditMode: mode,
          textEditCaretIndex: caretIndex ?? null,
        });
        if (shouldDelete && current) {
          await useProjectStore.getState().removeObject(current);
        }
      }
    })();
  },

  setActiveTool: (tool) => {
    const prevId = useUiStore.getState().textEditObjectId;
    const prevMode = useUiStore.getState().textEditMode;
    const shouldDelete = isNewEmptyText(prevId, prevMode);
    if (tool !== 'measure') {
      useMeasurementStore.getState().clear();
    }
    if (!hasPendingTextEdit()) {
      set((s) => {
        const panelUpdate = tool === 'measure'
          ? showMeasurementPanel(s.panelLayout, s.nextFloatingZIndex)
          : { layout: s.panelLayout, nextFloatingZIndex: s.nextFloatingZIndex };
        if (tool === 'measure') {
          appService.persistLayout(panelUpdate.layout);
        }
        return {
          activeTool: tool,
          nodeSubMode: 'select' as NodeSubMode,
          ...EMPTY_TEXT_EDIT_STATE,
          panelLayout: panelUpdate.layout,
          sidePanelsVisible: panelUpdate.layout.sidePanelsVisible,
          nextFloatingZIndex: panelUpdate.nextFloatingZIndex,
        };
      });
      if (shouldDelete && prevId) {
        void useProjectStore.getState().removeObject(prevId);
      }
      return;
    }
    void (async () => {
      if (await commitPendingTextEdit()) {
        set((s) => {
          const panelUpdate = tool === 'measure'
            ? showMeasurementPanel(s.panelLayout, s.nextFloatingZIndex)
            : { layout: s.panelLayout, nextFloatingZIndex: s.nextFloatingZIndex };
          if (tool === 'measure') {
            appService.persistLayout(panelUpdate.layout);
          }
          return {
            activeTool: tool,
            nodeSubMode: 'select' as NodeSubMode,
            ...EMPTY_TEXT_EDIT_STATE,
            panelLayout: panelUpdate.layout,
            sidePanelsVisible: panelUpdate.layout.sidePanelsVisible,
            nextFloatingZIndex: panelUpdate.nextFloatingZIndex,
          };
        });
        if (shouldDelete && prevId) {
          await useProjectStore.getState().removeObject(prevId);
        }
      }
    })();
  },

  setZoom: (zoom) => set({ zoom: clampZoom(zoom) }),
  zoomIn: () => set((s) => ({ zoom: clampZoom(s.zoom + ZOOM_STEP) })),
  zoomOut: () => set((s) => ({ zoom: clampZoom(s.zoom - ZOOM_STEP) })),
  zoomBy: (factor) => set((s) => ({ zoom: clampZoom(Math.round(s.zoom * factor)) })),

  setViewportOffset: (offset) => set({ viewportOffset: offset }),
  panBy: (dx, dy) =>
    set((s) => ({
      viewportOffset: { x: s.viewportOffset.x + dx, y: s.viewportOffset.y + dy },
    })),
  zoomToFit: (offset, zoom) => set({ viewportOffset: offset, zoom: clampZoom(zoom) }),

  toggleGrid: () => set((s) => ({ gridVisible: !s.gridVisible })),
  toggleSnap: () => set((s) => ({ snapToGrid: !s.snapToGrid })),
  toggleSnapToObjects: () => set((s) => ({ snapToObjects: !s.snapToObjects })),
  setGridSpacing: (spacing) => set({ gridSpacingMm: Math.max(0.1, spacing) }),
  setNudgeSteps: (steps) =>
    set({
      nudgeStepMm: Math.max(0, steps.normal),
      nudgeStepFineMm: Math.max(0, steps.fine),
      nudgeStepCoarseMm: Math.max(0, steps.coarse),
    }),
  updateJobOptions: (partial) =>
    set((s) => ({ jobOptions: { ...s.jobOptions, ...partial } })),

  setCursorWorldPos: (pos) => set({ cursorWorldPos: pos }),

  setViewStyle: (style) => set({ viewStyle: style }),
  toggleFilledRendering: () =>
    set((s) => {
      const isFilled = s.viewStyle.startsWith('filled_');
      const suffix = s.viewStyle.includes('_smooth') ? '_smooth' : '_coarse';
      return { viewStyle: (isFilled ? `wireframe${suffix}` : `filled${suffix}`) as ViewStyle };
    }),
  toggleSidePanels: () => {
    set((s) => {
      const updated = { ...s.panelLayout, sidePanelsVisible: !s.sidePanelsVisible };
      appService.persistLayout(updated);
      return { sidePanelsVisible: !s.sidePanelsVisible, panelLayout: updated };
    });
  },

  toggleRotary: () => set((s) => ({ rotaryEnabled: !s.rotaryEnabled })),
  togglePrintAndCut: () => set((s) => ({ printAndCutEnabled: !s.printAndCutEnabled })),
  toggleCameraWindow: () =>
    set((s) => {
      const hidden = s.panelLayout.hiddenPanelIds;
      const isHidden = hidden.includes('camera');
      const isFloating = s.panelLayout.floatingPanels.some((fp) => fp.panelId === 'camera');

      if (isHidden) {
        // If camera has a saved floating entry, unhide it and bring to front
        const wasFloating = s.panelLayout.floatingPanels.some((fp) => fp.panelId === 'camera');
        if (wasFloating) {
          const nextZ = s.nextFloatingZIndex;
          const layout: PanelLayoutState = {
            ...s.panelLayout,
            hiddenPanelIds: hidden.filter((id) => id !== 'camera'),
            floatingPanels: s.panelLayout.floatingPanels.map((fp) =>
              fp.panelId === 'camera' ? { ...fp, zIndex: nextZ } : fp,
            ),
          };
          appService.persistLayout(layout);
          return { panelLayout: layout, nextFloatingZIndex: nextZ + 1, cameraWindowOpen: true };
        }

        // If camera is still in a dock zone, unhide it and activate its tab
        const dockedZoneKey = (Object.keys(s.panelLayout.zones) as PhysicalDockZone[]).find(
          (zk) => s.panelLayout.zones[zk].panelIds.includes('camera'),
        );
        if (dockedZoneKey) {
          const zone = s.panelLayout.zones[dockedZoneKey];
          const layout: PanelLayoutState = {
            ...s.panelLayout,
            hiddenPanelIds: hidden.filter((id) => id !== 'camera'),
            zones: {
              ...s.panelLayout.zones,
              [dockedZoneKey]: { ...zone, activeTab: 'camera' },
            },
          };
          appService.persistLayout(layout);
          return { panelLayout: layout, cameraWindowOpen: true };
        }

        // Not in any dock zone and no saved float — auto-float at center of viewport
        const def = getPanelById('camera');
        const size = def?.defaultFloatSize ?? { w: 420, h: 400 };
        const nextZ = s.nextFloatingZIndex;
        const fp: FloatingPanelState = {
          panelId: 'camera',
          x: Math.max(0, (window.innerWidth - size.w) / 2),
          y: Math.max(0, (window.innerHeight - size.h) / 2),
          width: size.w,
          height: size.h,
          zIndex: nextZ,
        };
        const layout: PanelLayoutState = {
          ...s.panelLayout,
          hiddenPanelIds: hidden.filter((id) => id !== 'camera'),
          floatingPanels: [...s.panelLayout.floatingPanels, fp],
        };
        appService.persistLayout(layout);
        return { panelLayout: layout, nextFloatingZIndex: nextZ + 1, cameraWindowOpen: true };
      }

      if (isFloating) {
        // Hide: keep floating entry (position preserved), just add to hiddenPanelIds
        const layout: PanelLayoutState = {
          ...s.panelLayout,
          hiddenPanelIds: [...hidden, 'camera'],
        };
        appService.persistLayout(layout);
        return { panelLayout: layout, cameraWindowOpen: false };
      }

      // In a dock zone — hide it
      const newZones = { ...s.panelLayout.zones };
      const newHidden = [...hidden, 'camera'];
      for (const zoneKey of Object.keys(newZones) as PhysicalDockZone[]) {
        const zone = newZones[zoneKey];
        if (zone.activeTab === 'camera') {
          const firstVisible = zone.panelIds.find((id) => !newHidden.includes(id));
          newZones[zoneKey] = { ...zone, activeTab: firstVisible ?? '' };
        }
      }
      const layout: PanelLayoutState = {
        ...s.panelLayout,
        zones: newZones,
        hiddenPanelIds: newHidden,
      };
      appService.persistLayout(layout);
      return { panelLayout: layout, cameraWindowOpen: false };
    }),

  setHasClipboard: (has) => set({ hasClipboard: has }),

  setLayerSettingsClipboard: (entries) => set({ layerSettingsClipboard: entries }),

  flashLayer: (layerId) => {
    set({ flashedLayerId: layerId });
    setTimeout(() => {
      // Only clear if the same layer is still flashed; if another flash overrode this one,
      // its own timer is responsible for clearing it.
      if (useUiStore.getState().flashedLayerId === layerId) {
        set({ flashedLayerId: null });
      }
    }, FLASH_DURATION_MS);
  },
  toggleShowLastPosition: () => set((s) => ({ showLastPosition: !s.showLastPosition })),
  setLockAspect: (locked) => set({ lockAspect: locked }),
  toggleLockAspect: () => set((s) => ({ lockAspect: !s.lockAspect })),
  setDefaultCornerRadius: (r) => set({ defaultCornerRadius: Math.max(0, r) }),
  updateTextDefaults: (partial) => set((s) => ({ textDefaults: { ...s.textDefaults, ...partial } })),
  setRadiusToolValue: (v) => set({ radiusToolValue: v }),
  updateDockSettings: (partial) => set((s) => ({ dockSettings: { ...s.dockSettings, ...partial } })),
  updateNestSettings: (partial) => set((s) => ({ nestSettings: { ...s.nestSettings, ...partial } })),
  setNestingInProgress: (inProgress) => set({ nestingInProgress: inProgress }),
  setMoveWindowJogDistanceMm: (distanceMm) => set({ moveWindowJogDistanceMm: Math.max(0.001, distanceMm) }),
  setMoveWindowJogFeedRateMmMin: (feedRateMmMin) => set({ moveWindowJogFeedRateMmMin: Math.max(1, feedRateMmMin) }),
  setLastShapeSubTool: (id) => set({ lastShapeSubTool: id }),
  setLastBooleanOp: (id) => set({ lastBooleanOp: id }),
  setNodeEditNodeCount: (count) => set({ nodeEditNodeCount: count }),
  setNodeSubMode: (mode) => set({ nodeSubMode: mode }),
  setPendingStartPoint: (objectId) => set({ pendingStartPointObjectId: objectId }),
  setPendingGuidePathText: (objectId) => set({ pendingGuidePathTextId: objectId }),
  setOffsetPreview: (paths) => set({ offsetPreview: paths }),
}));

// Convenience getters derived from panelLayout
export function getActiveUpperTab(): string {
  return useUiStore.getState().panelLayout.zones['upper-right'].activeTab;
}

export function getActiveLowerTab(): string {
  return useUiStore.getState().panelLayout.zones['lower-right'].activeTab;
}

export function getRightPanelWidth(): number {
  return useUiStore.getState().panelLayout.rightPanelWidth;
}
