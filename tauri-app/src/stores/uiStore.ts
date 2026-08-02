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
import type { OffsetPreviewSourceFrame } from '../canvas/offsetPreview';
import type {
  ColumnSplitRatios,
  PanelColumnSide,
  PhysicalDockZone,
  PanelLayoutState,
  FloatingPanelState,
  ToolbarId,
  WorkspacePanelLayout,
} from '../panels';
import {
  COLUMN_ZONES,
  createPanelInstanceId,
  createDefaultLayout,
  getColumnForZone,
  getPanelById,
  getPanelTypeId,
  getWorkspacePanelLayout,
  normalizeToolbarVisibility,
  setWorkspacePanelLayout,
} from '../panels';
import { DEFAULT_GRID_SPACING_MM, MIN_ZOOM, MAX_ZOOM, ZOOM_STEP } from '../canvas/constants';
import { appService } from '../services/appService';
import { commitPendingTextEdit, hasPendingTextEdit, isNewEmptyText } from '../canvas/textEditSession';
import { useProjectStore } from './projectStore';
import { useMeasurementStore } from './measurementStore';

export type ToolType = 'select' | 'rect' | 'ellipse' | 'star' | 'text' | 'node' | 'line' | 'polygon' | 'trim' | 'tabs' | 'radius' | 'measure' | 'laser_position' | 'two_point_rotate_scale' | 'warp';

export type MeshDeformMode = 'warp' | 'mesh';

export type ModifierPropertiesKind = 'offset' | 'grid_array' | 'circular_array';

export interface ModifierPropertiesSession {
  kind: ModifierPropertiesKind;
  objectIds: string[];
}

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
  max_width: number | null;
  squeeze: boolean;
  rtl: boolean;
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
  max_width: null,
  squeeze: false,
  rtl: false,
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
  meshDeformMode: MeshDeformMode;
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
  /** Library drawer (Art / Materials) next to the tool rail. */
  libraryDrawerOpen: boolean;
  libraryDrawerTab: 'art' | 'materials';
  /** Design (draw/arrange) vs Run (machine/job) workspace. */
  workspaceMode: 'design' | 'run';

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

  // Contextual modifier controls shown at the bottom of Properties.
  modifierPropertiesSession: ModifierPropertiesSession | null;
  openModifierProperties: (kind: ModifierPropertiesKind, objectIds: string[]) => void;
  closeModifierProperties: () => void;

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

  // App-level dialogs whose state must be visible to native menu state sync
  showNotesDialog: boolean;
  setShowNotesDialog: (show: boolean) => void;
  toggleNotesDialog: () => void;

  // Node editing info (displayed in StatusBar)
  nodeEditNodeCount: number;
  /** Active node-edit object when its loaded geometry contains an open subpath. */
  nodeEditOpenPathObjectId: string | null;
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
  offsetPreviewSourceFrame: OffsetPreviewSourceFrame | null;
  setOffsetPreview: (
    paths: OffsetPreviewPath[] | null,
    sourceFrame?: OffsetPreviewSourceFrame | null,
  ) => void;

  // Panel layout actions
  setPanelLayout: (layout: PanelLayoutState) => void;
  setZoneActiveTab: (zone: PhysicalDockZone, tabId: string) => void;
  showPanel: (panelId: string) => void;
  togglePanelVisibility: (panelId: string) => void;
  setToolbarVisibility: (toolbarId: ToolbarId, visible: boolean) => void;
  toggleToolbarVisibility: (toolbarId: ToolbarId) => void;
  setUpperSplitRatio: (ratio: number) => void;
  swapRightPanelZones: () => void;
  setColumnBoundary: (
    side: PanelColumnSide,
    upperIndex: 0 | 1,
    boundaryRatio: number,
    lowerIndex?: 1 | 2,
  ) => void;
  revealColumnEdge: (side: PanelColumnSide, edge: 'top' | 'bottom') => void;
  setRightPanelWidth: (width: number) => void;
  setLeftPanelWidth: (width: number) => void;
  setBottomPanelHeight: (height: number) => void;
  resetLayout: () => void;

  // Floating panel actions
  addPanelInstance: (panelTypeId: string, targetZone: PhysicalDockZone) => void;
  removePanelInstance: (panelId: string) => void;
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
  setMeshDeformMode: (mode: MeshDeformMode) => void;

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
  toggleLibraryDrawer: () => void;
  setLibraryDrawerTab: (tab: 'art' | 'materials') => void;
  setWorkspaceMode: (mode: 'design' | 'run') => void;

  // Mode toggle actions
  toggleRotary: () => void;
  togglePrintAndCut: () => void;

  // Camera window actions
  toggleCameraWindow: () => void;

  // Node editing actions
  setNodeEditNodeCount: (count: number) => void;
  setNodeEditOpenPathObjectId: (objectId: string | null) => void;
  setNodeSubMode: (mode: NodeSubMode) => void;
}


/** Keep a floating panel's title bar reachable inside the window. */
const clampFloatX = (x: number) =>
  Math.max(0, Math.min(typeof window !== 'undefined' ? window.innerWidth - 100 : x, x));
const clampFloatY = (y: number) =>
  Math.max(0, Math.min(typeof window !== 'undefined' ? window.innerHeight - 40 : y, y));

const clampZoom = (z: number) => Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, z));

/** M4: how long a flash highlight stays visible before auto-clearing. */
const FLASH_DURATION_MS = 600;
const DEFAULT_OPEN_BOTTOM_DOCK_HEIGHT = 220;
const BOTTOM_DOCK_COLLAPSE_THRESHOLD = 32;

function revealPhysicalDock(layout: PanelLayoutState, zone: PhysicalDockZone): PanelLayoutState {
  if (zone !== 'bottom' || layout.bottomPanelHeight >= BOTTOM_DOCK_COLLAPSE_THRESHOLD) return layout;
  return { ...layout, bottomPanelHeight: DEFAULT_OPEN_BOTTOM_DOCK_HEIGHT };
}

/** Fix active tab after removing a panel from a zone. */
function fixActiveTab(zone: { panelIds: string[]; activeTab: string }, removedId: string, hidden: string[]): { panelIds: string[]; activeTab: string } {
  if (zone.activeTab !== removedId) return zone;
  const firstVisible = zone.panelIds.filter((id) => id !== removedId).find((id) => !hidden.includes(id));
  return { ...zone, activeTab: firstVisible ?? '' };
}

function sanitizePanelLayout(layout: PanelLayoutState): PanelLayoutState {
  const normalizeRatios = (ratios: ColumnSplitRatios): ColumnSplitRatios => {
    const safe = ratios.map((ratio) => Number.isFinite(ratio) ? Math.max(0, ratio) : 0);
    const total = safe.reduce((sum, ratio) => sum + ratio, 0);
    if (total <= 0) return [1, 0, 0];
    return safe.map((ratio) => ratio / total) as ColumnSplitRatios;
  };
  const sanitizeWorkspace = (workspace: WorkspacePanelLayout): WorkspacePanelLayout => {
    const hiddenPanelIds = [...new Set(workspace.hiddenPanelIds.filter((id) => getPanelById(id)))];
    const zones = { ...workspace.zones };
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
      zones,
      hiddenPanelIds,
      floatingPanels: workspace.floatingPanels.filter((fp) => getPanelById(fp.panelId)),
      upperSplitRatio: Math.max(0, Math.min(1, workspace.upperSplitRatio)),
      columnRatios: {
        left: normalizeRatios(workspace.columnRatios.left),
        right: normalizeRatios(workspace.columnRatios.right),
      },
    };
  };
  const design = sanitizeWorkspace(getWorkspacePanelLayout(layout, 'design'));
  const run = sanitizeWorkspace(getWorkspacePanelLayout(layout, 'run'));
  const withDesign = setWorkspacePanelLayout(layout, 'design', design);

  return setWorkspacePanelLayout({
    ...withDesign,
    layoutVersion: layout.layoutVersion,
    toolbarVisibility: normalizeToolbarVisibility(layout.toolbarVisibility),
  }, 'run', run);
}

function revealWorkspaceZone(workspace: WorkspacePanelLayout, zone: PhysicalDockZone): WorkspacePanelLayout {
  const side = getColumnForZone(zone);
  if (!side) return workspace;
  const zoneIndex = COLUMN_ZONES[side].indexOf(zone as never);
  const ratios = [...workspace.columnRatios[side]] as ColumnSplitRatios;
  if (zoneIndex < 0 || ratios[zoneIndex] > 0) return workspace;
  const donorIndex = ratios.indexOf(Math.max(...ratios));
  const revealSize = ratios[donorIndex] >= 0.5 ? 0.32 : ratios[donorIndex] / 2;
  ratios[donorIndex] -= revealSize;
  ratios[zoneIndex] = revealSize;
  return {
    ...workspace,
    upperSplitRatio: side === 'right' ? ratios[0] : workspace.upperSplitRatio,
    columnRatios: { ...workspace.columnRatios, [side]: ratios },
  };
}

function collapseWorkspaceZone(workspace: WorkspacePanelLayout, zone: PhysicalDockZone): WorkspacePanelLayout {
  const side = getColumnForZone(zone);
  if (!side) return workspace;
  const zoneIndex = COLUMN_ZONES[side].indexOf(zone as never);
  if (zoneIndex < 0) return workspace;

  const ratios = [...workspace.columnRatios[side]] as ColumnSplitRatios;
  const released = ratios[zoneIndex];
  ratios[zoneIndex] = 0;
  const remainingTotal = ratios.reduce((sum, ratio) => sum + ratio, 0);
  if (remainingTotal <= 0) {
    ratios[0] = 1;
  } else if (released > 0) {
    for (let index = 0; index < ratios.length; index += 1) {
      if (index !== zoneIndex && ratios[index] > 0) {
        ratios[index] += released * (ratios[index] / remainingTotal);
      }
    }
  }

  return {
    ...workspace,
    upperSplitRatio: side === 'right' ? ratios[0] : workspace.upperSplitRatio,
    columnRatios: { ...workspace.columnRatios, [side]: ratios },
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

  const targetZone: PhysicalDockZone = 'top-right';
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
  meshDeformMode: 'warp',
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
  libraryDrawerOpen: false,
  libraryDrawerTab: 'art' as const,
  workspaceMode: 'design' as const,

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
  modifierPropertiesSession: null,
  dockSettings: { ...DEFAULT_DOCK_SETTINGS },
  nestSettings: { ...DEFAULT_NEST_SETTINGS },
  nestingInProgress: false,
  moveWindowJogDistanceMm: 10,
  moveWindowJogFeedRateMmMin: 1000,
  lastShapeSubTool: 'rect',
  showNotesDialog: false,
  nodeEditNodeCount: 0,
  nodeEditOpenPathObjectId: null,
  nodeSubMode: 'select' as NodeSubMode,
  textEditObjectId: null,
  textEditClickPos: null,
  textEditMode: null,
  textEditCaretIndex: null,
  pendingStartPointObjectId: null,
  pendingGuidePathTextId: null,
  offsetPreview: null,
  offsetPreviewSourceFrame: null,

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
    set((s) => {
      const workspace = getWorkspacePanelLayout(s.panelLayout, s.workspaceMode);
      const nextWorkspace = {
        ...workspace,
        zones: {
          ...workspace.zones,
          [zone]: { ...workspace.zones[zone], activeTab: tabId },
        },
      };
      return { panelLayout: setWorkspacePanelLayout(s.panelLayout, s.workspaceMode, nextWorkspace) };
    }),

  showPanel: (panelId) =>
    set((s) => {
      const workspace = getWorkspacePanelLayout(s.panelLayout, s.workspaceMode);
      const hidden = workspace.hiddenPanelIds;
      const isHidden = hidden.includes(panelId);
      let nextWorkspace: WorkspacePanelLayout = {
        ...workspace,
        hiddenPanelIds: isHidden ? hidden.filter((id) => id !== panelId) : hidden,
      };
      const floatingIndex = nextWorkspace.floatingPanels.findIndex((fp) => fp.panelId === panelId);
      if (floatingIndex >= 0) {
        const nextZ = s.nextFloatingZIndex;
        nextWorkspace.floatingPanels = nextWorkspace.floatingPanels.map((fp, index) =>
          index === floatingIndex ? { ...fp, zIndex: nextZ } : fp,
        );
        const newLayout = setWorkspacePanelLayout(s.panelLayout, s.workspaceMode, nextWorkspace);
        appService.persistLayout(newLayout);
        return { panelLayout: newLayout, nextFloatingZIndex: nextZ + 1 };
      }

      const def = getPanelById(panelId);
      if (def) {
        const existingZoneKey = (Object.keys(nextWorkspace.zones) as PhysicalDockZone[]).find(
          (zk) => nextWorkspace.zones[zk].panelIds.includes(panelId),
        );
        if (!existingZoneKey) {
          if (def.defaultZone === 'floating') {
            const size = def.defaultFloatSize ?? { w: 384, h: 300 };
            const nextZ = s.nextFloatingZIndex;
            nextWorkspace.floatingPanels = [
              ...nextWorkspace.floatingPanels,
              { panelId, x: 100, y: 100, width: size.w, height: size.h, zIndex: nextZ },
            ];
            const newLayout = setWorkspacePanelLayout(s.panelLayout, s.workspaceMode, nextWorkspace);
            appService.persistLayout(newLayout);
            return { panelLayout: newLayout, nextFloatingZIndex: nextZ + 1 };
          }
          const targetZone = def.defaultZone as PhysicalDockZone;
          const zone = nextWorkspace.zones[targetZone];
          if (zone) {
            nextWorkspace.zones = {
              ...nextWorkspace.zones,
              [targetZone]: { ...zone, panelIds: [...zone.panelIds, panelId], activeTab: panelId },
            };
            nextWorkspace = revealWorkspaceZone(nextWorkspace, targetZone);
          }
        } else {
          const zone = nextWorkspace.zones[existingZoneKey];
          nextWorkspace.zones = {
            ...nextWorkspace.zones,
            [existingZoneKey]: { ...zone, activeTab: panelId },
          };
          nextWorkspace = revealWorkspaceZone(nextWorkspace, existingZoneKey);
        }
      }

      const newLayout = setWorkspacePanelLayout(s.panelLayout, s.workspaceMode, nextWorkspace);
      appService.persistLayout(newLayout);
      return { panelLayout: newLayout };
    }),

  togglePanelVisibility: (panelId) =>
    set((s) => {
      const workspace = getWorkspacePanelLayout(s.panelLayout, s.workspaceMode);
      const hidden = workspace.hiddenPanelIds;
      const isHidden = hidden.includes(panelId);
      const newHidden = isHidden ? hidden.filter((id) => id !== panelId) : [...hidden, panelId];

      // Keep floating entries on hide — position/size is preserved in floatingPanels.
      // FloatingPanelLayer filters out hidden panels before rendering.
      let nextWorkspace: WorkspacePanelLayout = { ...workspace, hiddenPanelIds: newHidden };

      if (isHidden) {
        // Showing: if the panel is already in floatingPanels, unhide it and bring to front
        const wasFloating = nextWorkspace.floatingPanels.some((fp) => fp.panelId === panelId);
        if (wasFloating) {
          const nextZ = s.nextFloatingZIndex;
          nextWorkspace.floatingPanels = nextWorkspace.floatingPanels.map((fp) =>
            fp.panelId === panelId ? { ...fp, zIndex: nextZ } : fp,
          );
          const newLayout = setWorkspacePanelLayout(s.panelLayout, s.workspaceMode, nextWorkspace);
          appService.persistLayout(newLayout);
          return { panelLayout: newLayout, nextFloatingZIndex: nextZ + 1 };
        }

        const def = getPanelById(panelId);
        // If it defaults to floating (and has no saved float entry), create a new one
        if (def?.defaultZone === 'floating') {
          const size = def.defaultFloatSize ?? { w: 384, h: 300 };
          const nextZ = s.nextFloatingZIndex;
          nextWorkspace.floatingPanels = [
            ...nextWorkspace.floatingPanels,
            { panelId, x: 100, y: 100, width: size.w, height: size.h, zIndex: nextZ },
          ];
          const newLayout = setWorkspacePanelLayout(s.panelLayout, s.workspaceMode, nextWorkspace);
          appService.persistLayout(newLayout);
          return { panelLayout: newLayout, nextFloatingZIndex: nextZ + 1 };
        }

        // If unhiding a docked panel that is not in any zone, insert it into its defaultZone.
        // If it IS already in a zone, activate its tab so the user actually sees it.
        if (def) {
          const existingZoneKey = (Object.keys(nextWorkspace.zones) as PhysicalDockZone[]).find(
            (zk) => nextWorkspace.zones[zk].panelIds.includes(panelId),
          );
          if (!existingZoneKey) {
            const targetZone = def.defaultZone as PhysicalDockZone;
            const zone = nextWorkspace.zones[targetZone];
            if (zone) {
              nextWorkspace.zones = {
                ...nextWorkspace.zones,
                [targetZone]: { ...zone, panelIds: [...zone.panelIds, panelId], activeTab: panelId },
              };
              nextWorkspace = revealWorkspaceZone(nextWorkspace, targetZone);
            }
          } else {
            const zone = nextWorkspace.zones[existingZoneKey];
            nextWorkspace.zones = {
              ...nextWorkspace.zones,
              [existingZoneKey]: { ...zone, activeTab: panelId },
            };
            nextWorkspace = revealWorkspaceZone(nextWorkspace, existingZoneKey);
          }
        }
      }

      // If hiding the active tab in a zone, switch to the first visible tab in that zone
      const newZones = { ...nextWorkspace.zones };
      if (!isHidden) {
        for (const zoneKey of Object.keys(newZones) as PhysicalDockZone[]) {
          const zone = newZones[zoneKey];
          if (zone.activeTab === panelId) {
            const firstVisible = zone.panelIds.find((id) => !newHidden.includes(id));
            newZones[zoneKey] = { ...zone, activeTab: firstVisible ?? '' };
          }
        }
      }

      nextWorkspace = {
        ...nextWorkspace,
        zones: newZones,
      };
      const layout = setWorkspacePanelLayout(s.panelLayout, s.workspaceMode, nextWorkspace);
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
    set((s) => {
      const workspace = getWorkspacePanelLayout(s.panelLayout, s.workspaceMode);
      const clamped = Math.max(0, Math.min(1, ratio));
      const snapped = clamped < 0.06 ? 0 : clamped > 0.94 ? 1 : clamped;
      const rightRatios: ColumnSplitRatios = [snapped, 1 - snapped, 0];
      return {
        panelLayout: setWorkspacePanelLayout(s.panelLayout, s.workspaceMode, {
          ...workspace,
          upperSplitRatio: snapped,
          columnRatios: { ...workspace.columnRatios, right: rightRatios },
        }),
      };
    }),

  swapRightPanelZones: () =>
    set((s) => {
      const workspace = getWorkspacePanelLayout(s.panelLayout, s.workspaceMode);
      const nextWorkspace: WorkspacePanelLayout = {
        ...workspace,
        zones: {
          ...workspace.zones,
          'top-right': workspace.zones['middle-right'],
          'middle-right': workspace.zones['top-right'],
        },
        upperSplitRatio: 1 - workspace.upperSplitRatio,
        columnRatios: {
          ...workspace.columnRatios,
          right: [
            workspace.columnRatios.right[1],
            workspace.columnRatios.right[0],
            workspace.columnRatios.right[2],
          ],
        },
      };
      return { panelLayout: setWorkspacePanelLayout(s.panelLayout, s.workspaceMode, nextWorkspace) };
    }),

  setColumnBoundary: (side, upperIndex, boundaryRatio, requestedLowerIndex) =>
    set((s) => {
      const workspace = getWorkspacePanelLayout(s.panelLayout, s.workspaceMode);
      const ratios = [...workspace.columnRatios[side]] as ColumnSplitRatios;
      const lowerIndex = requestedLowerIndex ?? upperIndex + 1;
      const prefix = ratios.slice(0, upperIndex).reduce((sum, ratio) => sum + ratio, 0);
      const pairTotal = ratios[upperIndex] + ratios[lowerIndex];
      if (pairTotal <= 0) return {};
      const clamped = Math.max(prefix, Math.min(prefix + pairTotal, boundaryRatio));
      let first = clamped - prefix;
      let second = pairTotal - first;
      if (first < 0.06) {
        first = 0;
        second = pairTotal;
      } else if (second < 0.06) {
        first = pairTotal;
        second = 0;
      }
      ratios[upperIndex] = first;
      ratios[lowerIndex] = second;
      const nextWorkspace: WorkspacePanelLayout = {
        ...workspace,
        upperSplitRatio: side === 'right' && upperIndex === 0 ? ratios[0] : workspace.upperSplitRatio,
        columnRatios: { ...workspace.columnRatios, [side]: ratios },
      };
      return { panelLayout: setWorkspacePanelLayout(s.panelLayout, s.workspaceMode, nextWorkspace) };
    }),

  revealColumnEdge: (side, edge) =>
    set((s) => {
      const workspace = getWorkspacePanelLayout(s.panelLayout, s.workspaceMode);
      const zoneIds = COLUMN_ZONES[side];
      const ratios = [...workspace.columnRatios[side]] as ColumnSplitRatios;
      const activeCount = ratios.filter((ratio) => ratio > 0).length;
      if (activeCount >= 3) return {};
      const zones = { ...workspace.zones };
      const emptyZone = { panelIds: [], activeTab: '' };

      // If an interior section was collapsed, either outer reveal handle restores
      // that same section instead of shifting zones and orphaning its tabs.
      if (activeCount === 2) {
        const collapsedIndex = ratios.findIndex((ratio) => ratio <= 0);
        if (collapsedIndex >= 0) {
          const revealSize = 0.28;
          const occupiedTotal = ratios.reduce((sum, ratio) => sum + ratio, 0);
          for (let index = 0; index < ratios.length; index += 1) {
            if (index !== collapsedIndex && ratios[index] > 0) {
              ratios[index] = ratios[index] / occupiedTotal * (1 - revealSize);
            }
          }
          ratios[collapsedIndex] = revealSize;
        }
      } else if (edge === 'top') {
        if (ratios[0] <= 0) {
          const remaining = ratios[1] + ratios[2];
          ratios[0] = 0.28;
          if (remaining > 0) {
            ratios[1] = ratios[1] / remaining * 0.72;
            ratios[2] = ratios[2] / remaining * 0.72;
          }
        } else {
          zones[zoneIds[2]] = workspace.zones[zoneIds[1]];
          zones[zoneIds[1]] = workspace.zones[zoneIds[0]];
          zones[zoneIds[0]] = emptyZone;
          ratios[2] = ratios[1] * 0.72;
          ratios[1] = ratios[0] * 0.72;
          ratios[0] = 0.28;
        }
      } else if (ratios[1] <= 0 && ratios[2] <= 0) {
        ratios[0] = 0.72;
        ratios[1] = 0.28;
      } else if (ratios[2] <= 0) {
        const occupied = ratios[0] + ratios[1];
        ratios[0] = ratios[0] / occupied * 0.72;
        ratios[1] = ratios[1] / occupied * 0.72;
        ratios[2] = 0.28;
      } else {
        zones[zoneIds[0]] = workspace.zones[zoneIds[1]];
        zones[zoneIds[1]] = workspace.zones[zoneIds[2]];
        zones[zoneIds[2]] = emptyZone;
        ratios[0] = ratios[1] * 0.72;
        ratios[1] = ratios[2] * 0.72;
        ratios[2] = 0.28;
      }

      const nextWorkspace: WorkspacePanelLayout = {
        ...workspace,
        zones,
        upperSplitRatio: side === 'right' ? ratios[0] : workspace.upperSplitRatio,
        columnRatios: { ...workspace.columnRatios, [side]: ratios },
      };
      return { panelLayout: setWorkspacePanelLayout(s.panelLayout, s.workspaceMode, nextWorkspace) };
    }),

  setRightPanelWidth: (width) =>
    set((s) => ({
      panelLayout: {
        ...s.panelLayout,
        rightPanelWidth: width < 180 ? 0 : Math.min(600, width),
      },
    })),

  setLeftPanelWidth: (width) =>
    set((s) => ({
      panelLayout: { ...s.panelLayout, leftPanelWidth: width < 150 ? 0 : Math.min(600, width) },
    })),

  setBottomPanelHeight: (height) =>
    set((s) => ({
      panelLayout: {
        ...s.panelLayout,
        bottomPanelHeight: height < BOTTOM_DOCK_COLLAPSE_THRESHOLD ? 0 : Math.min(500, height),
      },
    })),

  resetLayout: () => {
    const layout = createDefaultLayout();
    appService.persistLayout(layout);
    set({ panelLayout: layout, nextFloatingZIndex: 1, sidePanelsVisible: true, cameraWindowOpen: false });
  },

  // --- Floating panel actions ---

  addPanelInstance: (panelTypeId, targetZone) =>
    set((s) => {
      const definition = getPanelById(panelTypeId);
      if (!definition) return {};
      const workspace = getWorkspacePanelLayout(s.panelLayout, s.workspaceMode);
      const occupiedIds = [
        ...Object.values(workspace.zones).flatMap((zone) => zone.panelIds),
        ...workspace.floatingPanels.map((panel) => panel.panelId),
      ];
      const panelId = createPanelInstanceId(definition.id, occupiedIds);
      const target = workspace.zones[targetZone];
      if (!target) return {};
      const zones = {
        ...workspace.zones,
        [targetZone]: {
          ...target,
          panelIds: [...target.panelIds, panelId],
          activeTab: panelId,
        },
      };
      const nextWorkspace = revealWorkspaceZone({
        ...workspace,
        zones,
        hiddenPanelIds: workspace.hiddenPanelIds.filter((id) => id !== panelId),
      }, targetZone);
      const layout = revealPhysicalDock(
        setWorkspacePanelLayout(s.panelLayout, s.workspaceMode, nextWorkspace),
        targetZone,
      );
      appService.persistLayout(layout);
      return { panelLayout: layout };
    }),

  removePanelInstance: (panelId) =>
    set((s) => {
      const workspace = getWorkspacePanelLayout(s.panelLayout, s.workspaceMode);
      const hiddenPanelIds = workspace.hiddenPanelIds.filter((id) => id !== panelId);
      let emptiedZone: PhysicalDockZone | null = null;
      const zones = { ...workspace.zones };
      for (const zoneKey of Object.keys(zones) as PhysicalDockZone[]) {
        const zone = zones[zoneKey];
        if (!zone.panelIds.includes(panelId)) continue;
        const panelIds = zone.panelIds.filter((id) => id !== panelId);
        zones[zoneKey] = fixActiveTab({ ...zone, panelIds }, panelId, hiddenPanelIds);
        if (!panelIds.some((id) => !hiddenPanelIds.includes(id))) emptiedZone = zoneKey;
      }

      let nextWorkspace: WorkspacePanelLayout = {
        ...workspace,
        zones,
        hiddenPanelIds,
        floatingPanels: workspace.floatingPanels.filter((panel) => panel.panelId !== panelId),
      };
      if (emptiedZone) nextWorkspace = collapseWorkspaceZone(nextWorkspace, emptiedZone);
      const layout = setWorkspacePanelLayout(s.panelLayout, s.workspaceMode, nextWorkspace);
      appService.persistLayout(layout);
      return {
        panelLayout: layout,
        ...(getPanelTypeId(panelId) === 'camera' ? { cameraWindowOpen: false } : {}),
      };
    }),

  floatPanel: (panelId, x, y, w, h) =>
    set((s) => {
      const workspace = getWorkspacePanelLayout(s.panelLayout, s.workspaceMode);
      const newZones = { ...workspace.zones };
      const hidden = workspace.hiddenPanelIds;

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
      const fp: FloatingPanelState = { panelId, x: clampFloatX(x), y: clampFloatY(y), width: w, height: h, zIndex: nextZ, originZone, originIndex };
      const newFloating = [...workspace.floatingPanels.filter((f) => f.panelId !== panelId), fp];

      // Remove from hidden if present
      const newHidden = hidden.filter((id) => id !== panelId);

      const nextWorkspace: WorkspacePanelLayout = {
        ...workspace,
        zones: newZones,
        floatingPanels: newFloating,
        hiddenPanelIds: newHidden,
      };
      const layout = setWorkspacePanelLayout(s.panelLayout, s.workspaceMode, nextWorkspace);
      appService.persistLayout(layout);
      return { panelLayout: layout, nextFloatingZIndex: nextZ + 1 };
    }),

  dockPanel: (panelId, targetZone, insertIndex) =>
    set((s) => {
      const workspace = getWorkspacePanelLayout(s.panelLayout, s.workspaceMode);
      // Remove from floatingPanels
      const newFloating = workspace.floatingPanels.filter((fp) => fp.panelId !== panelId);

      // Remove from hidden
      const newHidden = workspace.hiddenPanelIds.filter((id) => id !== panelId);

      // Add to target zone
      const newZones = { ...workspace.zones };
      const zone = newZones[targetZone];
      const panelIds = zone.panelIds.filter((id) => id !== panelId);
      const idx = insertIndex !== undefined ? Math.min(insertIndex, panelIds.length) : panelIds.length;
      panelIds.splice(idx, 0, panelId);
      newZones[targetZone] = { panelIds, activeTab: panelId };

      const nextWorkspace = revealWorkspaceZone({
        ...workspace,
        zones: newZones,
        floatingPanels: newFloating,
        hiddenPanelIds: newHidden,
      }, targetZone);
      const layout = revealPhysicalDock(
        setWorkspacePanelLayout(s.panelLayout, s.workspaceMode, nextWorkspace),
        targetZone,
      );
      appService.persistLayout(layout);
      return { panelLayout: layout };
    }),

  moveFloatingPanel: (panelId, x, y) =>
    set((s) => {
      const workspace = getWorkspacePanelLayout(s.panelLayout, s.workspaceMode);
      const newFloating = workspace.floatingPanels.map((fp) =>
        fp.panelId === panelId ? { ...fp, x: clampFloatX(x), y: clampFloatY(y) } : fp,
      );
      const layout = setWorkspacePanelLayout(s.panelLayout, s.workspaceMode, {
        ...workspace,
        floatingPanels: newFloating,
      });
      appService.persistLayout(layout);
      return { panelLayout: layout };
    }),

  resizeFloatingPanel: (panelId, w, h) =>
    set((s) => {
      const workspace = getWorkspacePanelLayout(s.panelLayout, s.workspaceMode);
      const def = getPanelById(panelId);
      const minW = def?.minFloatSize?.w ?? 200;
      const minH = def?.minFloatSize?.h ?? 150;
      const newFloating = workspace.floatingPanels.map((fp) =>
        fp.panelId === panelId ? { ...fp, width: Math.max(minW, w), height: Math.max(minH, h) } : fp,
      );
      const layout = setWorkspacePanelLayout(s.panelLayout, s.workspaceMode, {
        ...workspace,
        floatingPanels: newFloating,
      });
      appService.persistLayout(layout);
      return { panelLayout: layout };
    }),

  bringToFront: (panelId) =>
    set((s) => {
      const workspace = getWorkspacePanelLayout(s.panelLayout, s.workspaceMode);
      const nextZ = s.nextFloatingZIndex;
      const newFloating = workspace.floatingPanels.map((fp) =>
        fp.panelId === panelId ? { ...fp, zIndex: nextZ } : fp,
      );
      const layout = setWorkspacePanelLayout(s.panelLayout, s.workspaceMode, {
        ...workspace,
        floatingPanels: newFloating,
      });
      appService.persistLayout(layout);
      return { panelLayout: layout, nextFloatingZIndex: nextZ + 1 };
    }),

  closeFloatingPanel: (panelId) =>
    set((s) => {
      const workspace = getWorkspacePanelLayout(s.panelLayout, s.workspaceMode);
      // Keep floating entry (position/size preserved) — just hide the panel
      const newHidden = workspace.hiddenPanelIds.includes(panelId)
        ? workspace.hiddenPanelIds
        : [...workspace.hiddenPanelIds, panelId];
      const layout = setWorkspacePanelLayout(s.panelLayout, s.workspaceMode, {
        ...workspace,
        hiddenPanelIds: newHidden,
      });
      appService.persistLayout(layout);
      return {
        panelLayout: layout,
        ...(panelId === 'camera' ? { cameraWindowOpen: false } : {}),
      };
    }),

  movePanelBetweenZones: (panelId, fromZone, toZone, insertIndex) =>
    set((s) => {
      const workspace = getWorkspacePanelLayout(s.panelLayout, s.workspaceMode);
      const newZones = { ...workspace.zones };
      const hidden = workspace.hiddenPanelIds;

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

      const nextWorkspace = revealWorkspaceZone({ ...workspace, zones: newZones }, toZone);
      const layout = revealPhysicalDock(
        setWorkspacePanelLayout(s.panelLayout, s.workspaceMode, nextWorkspace),
        toZone,
      );
      appService.persistLayout(layout);
      return { panelLayout: layout };
    }),

  reorderPanelInZone: (panelId, zone, newIndex) =>
    set((s) => {
      const workspace = getWorkspacePanelLayout(s.panelLayout, s.workspaceMode);
      const newZones = { ...workspace.zones };
      const zoneState = newZones[zone];
      const ids = zoneState.panelIds.filter((id) => id !== panelId);
      const idx = Math.min(newIndex, ids.length);
      ids.splice(idx, 0, panelId);
      newZones[zone] = { ...zoneState, panelIds: ids };

      const layout = setWorkspacePanelLayout(s.panelLayout, s.workspaceMode, {
        ...workspace,
        zones: newZones,
      });
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
    // Run is an inspection/execution workspace. Tool commands can originate
    // from global shortcuts and native-menu actions even though the creation
    // toolbar is hidden, so reject drawing/editing tools at the store boundary.
    // Laser Position is the one Run-owned canvas tool; reject it in Design too
    // so shortcuts and native menu commands cannot bypass workspace ownership.
    const workspaceMode = useUiStore.getState().workspaceMode;
    if (workspaceMode === 'run' && tool !== 'select' && tool !== 'laser_position') return;
    if (workspaceMode === 'design' && tool === 'laser_position') return;

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
          modifierPropertiesSession: null,
          nodeSubMode: 'select' as NodeSubMode,
          // Set Start Point is a modal canvas overlay, not an active ToolType.
          // Any explicit tool choice must dismiss it, even when the user
          // re-selects the tool that is already active (most often Select).
          pendingStartPointObjectId: null,
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
            modifierPropertiesSession: null,
            nodeSubMode: 'select' as NodeSubMode,
            pendingStartPointObjectId: null,
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

  setMeshDeformMode: (mode) => set({ meshDeformMode: mode }),

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
  toggleLibraryDrawer: () => set((s) => ({ libraryDrawerOpen: !s.libraryDrawerOpen })),
  setWorkspaceMode: (mode) => set((s) => ({
    workspaceMode: mode,
    libraryDrawerOpen: false,
    modifierPropertiesSession: null,
    ...(mode === 'run' || s.activeTool === 'laser_position'
      ? { activeTool: 'select' as const }
      : {}),
  })),
  setLibraryDrawerTab: (tab) => set({ libraryDrawerTab: tab }),
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
      const workspace = getWorkspacePanelLayout(s.panelLayout, s.workspaceMode);
      const hidden = workspace.hiddenPanelIds;
      const isHidden = hidden.includes('camera');
      const isFloating = workspace.floatingPanels.some((fp) => fp.panelId === 'camera');

      if (isHidden) {
        // If camera has a saved floating entry, unhide it and bring to front
        const wasFloating = workspace.floatingPanels.some((fp) => fp.panelId === 'camera');
        if (wasFloating) {
          const nextZ = s.nextFloatingZIndex;
          const nextWorkspace: WorkspacePanelLayout = {
            ...workspace,
            hiddenPanelIds: hidden.filter((id) => id !== 'camera'),
            floatingPanels: workspace.floatingPanels.map((fp) =>
              fp.panelId === 'camera' ? { ...fp, zIndex: nextZ } : fp,
            ),
          };
          const layout = setWorkspacePanelLayout(s.panelLayout, s.workspaceMode, nextWorkspace);
          appService.persistLayout(layout);
          return { panelLayout: layout, nextFloatingZIndex: nextZ + 1, cameraWindowOpen: true };
        }

        // If camera is still in a dock zone, unhide it and activate its tab
        const dockedZoneKey = (Object.keys(workspace.zones) as PhysicalDockZone[]).find(
          (zk) => workspace.zones[zk].panelIds.includes('camera'),
        );
        if (dockedZoneKey) {
          const zone = workspace.zones[dockedZoneKey];
          const nextWorkspace = revealWorkspaceZone({
            ...workspace,
            hiddenPanelIds: hidden.filter((id) => id !== 'camera'),
            zones: {
              ...workspace.zones,
              [dockedZoneKey]: { ...zone, activeTab: 'camera' },
            },
          }, dockedZoneKey);
          const layout = setWorkspacePanelLayout(s.panelLayout, s.workspaceMode, nextWorkspace);
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
        const nextWorkspace: WorkspacePanelLayout = {
          ...workspace,
          hiddenPanelIds: hidden.filter((id) => id !== 'camera'),
          floatingPanels: [...workspace.floatingPanels, fp],
        };
        const layout = setWorkspacePanelLayout(s.panelLayout, s.workspaceMode, nextWorkspace);
        appService.persistLayout(layout);
        return { panelLayout: layout, nextFloatingZIndex: nextZ + 1, cameraWindowOpen: true };
      }

      if (isFloating) {
        // Hide: keep floating entry (position preserved), just add to hiddenPanelIds
        const nextWorkspace: WorkspacePanelLayout = {
          ...workspace,
          hiddenPanelIds: [...hidden, 'camera'],
        };
        const layout = setWorkspacePanelLayout(s.panelLayout, s.workspaceMode, nextWorkspace);
        appService.persistLayout(layout);
        return { panelLayout: layout, cameraWindowOpen: false };
      }

      // In a dock zone — hide it
      const newZones = { ...workspace.zones };
      const newHidden = [...hidden, 'camera'];
      for (const zoneKey of Object.keys(newZones) as PhysicalDockZone[]) {
        const zone = newZones[zoneKey];
        if (zone.activeTab === 'camera') {
          const firstVisible = zone.panelIds.find((id) => !newHidden.includes(id));
          newZones[zoneKey] = { ...zone, activeTab: firstVisible ?? '' };
        }
      }
      const nextWorkspace: WorkspacePanelLayout = {
        ...workspace,
        zones: newZones,
        hiddenPanelIds: newHidden,
      };
      const layout = setWorkspacePanelLayout(s.panelLayout, s.workspaceMode, nextWorkspace);
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
  openModifierProperties: (kind, objectIds) => {
    // Modifier panels are selection actions, not canvas tools. Leaving an
    // active drawing/editing tool also prevents its overlays from competing
    // with the contextual controls.
    useUiStore.getState().setActiveTool('select');
    set({ modifierPropertiesSession: { kind, objectIds: [...objectIds] } });
    const ui = useUiStore.getState();
    if (!ui.sidePanelsVisible) ui.toggleSidePanels();
    useUiStore.getState().showPanel('properties');
  },
  closeModifierProperties: () => set({ modifierPropertiesSession: null }),
  updateDockSettings: (partial) => set((s) => ({ dockSettings: { ...s.dockSettings, ...partial } })),
  updateNestSettings: (partial) => set((s) => ({ nestSettings: { ...s.nestSettings, ...partial } })),
  setNestingInProgress: (inProgress) => set({ nestingInProgress: inProgress }),
  setMoveWindowJogDistanceMm: (distanceMm) => set({ moveWindowJogDistanceMm: Math.max(0.001, distanceMm) }),
  setMoveWindowJogFeedRateMmMin: (feedRateMmMin) => set({ moveWindowJogFeedRateMmMin: Math.max(1, feedRateMmMin) }),
  setLastShapeSubTool: (id) => set({ lastShapeSubTool: id }),
  setNodeEditNodeCount: (count) => set({ nodeEditNodeCount: count }),
  setNodeEditOpenPathObjectId: (objectId) => set({ nodeEditOpenPathObjectId: objectId }),
  setNodeSubMode: (mode) => set({ nodeSubMode: mode }),
  setPendingStartPoint: (objectId) => set({ pendingStartPointObjectId: objectId }),
  setPendingGuidePathText: (objectId) => set({ pendingGuidePathTextId: objectId }),
  setOffsetPreview: (paths, sourceFrame = null) => set({
    offsetPreview: paths,
    offsetPreviewSourceFrame: paths ? sourceFrame : null,
  }),
}));

// Convenience getters derived from panelLayout
export function getActiveUpperTab(): string {
  return useUiStore.getState().panelLayout.zones['top-right'].activeTab;
}

export function getActiveLowerTab(): string {
  return useUiStore.getState().panelLayout.zones['middle-right'].activeTab;
}

export function getRightPanelWidth(): number {
  return useUiStore.getState().panelLayout.rightPanelWidth;
}
