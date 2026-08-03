import type { PanelColumnSide, PhysicalDockZone } from './panelRegistry';
import { getDefaultLayout } from './panelRegistry';

export interface ZoneState {
  panelIds: string[];
  activeTab: string;
}

export interface FloatingPanelState {
  panelId: string;
  x: number;
  y: number;
  width: number;
  height: number;
  zIndex: number;
  originZone?: string;
  originIndex?: number;
}

export type PanelWorkspaceMode = 'design' | 'run';
export type ColumnSplitRatios = [number, number, number];
export type WorkspaceColumnRatios = Record<PanelColumnSide, ColumnSplitRatios>;

export interface WorkspacePanelLayout {
  zones: Record<PhysicalDockZone, ZoneState>;
  hiddenPanelIds: string[];
  floatingPanels: FloatingPanelState[];
  upperSplitRatio: number;
  columnRatios: WorkspaceColumnRatios;
}

export interface PanelLayoutState {
  layoutVersion: number;
  zones: Record<PhysicalDockZone, ZoneState>;
  hiddenPanelIds: string[];
  floatingPanels: FloatingPanelState[];
  upperSplitRatio: number;   // 0-1; 1 collapses the lower Design dock
  runZones: Record<PhysicalDockZone, ZoneState>;
  runHiddenPanelIds: string[];
  runFloatingPanels: FloatingPanelState[];
  runUpperSplitRatio: number;
  columnRatios: WorkspaceColumnRatios;
  runColumnRatios: WorkspaceColumnRatios;
  rightPanelWidth: number;   // px, default 384
  leftPanelWidth: number;    // px, default 280
  bottomPanelHeight: number; // px, default 200
  sidePanelsVisible: boolean; // global side-panel collapsed/expanded state
  toolbarVisibility: ToolbarVisibility;
}

export function getWorkspacePanelLayout(
  layout: PanelLayoutState,
  workspace: PanelWorkspaceMode,
): WorkspacePanelLayout {
  return workspace === 'run'
    ? {
        zones: layout.runZones,
        hiddenPanelIds: layout.runHiddenPanelIds,
        floatingPanels: layout.runFloatingPanels,
        upperSplitRatio: layout.runUpperSplitRatio,
        columnRatios: layout.runColumnRatios,
      }
    : {
        zones: layout.zones,
        hiddenPanelIds: layout.hiddenPanelIds,
        floatingPanels: layout.floatingPanels,
        upperSplitRatio: layout.upperSplitRatio,
        columnRatios: layout.columnRatios,
      };
}

export function setWorkspacePanelLayout(
  layout: PanelLayoutState,
  workspace: PanelWorkspaceMode,
  next: WorkspacePanelLayout,
): PanelLayoutState {
  return workspace === 'run'
    ? {
        ...layout,
        runZones: next.zones,
        runHiddenPanelIds: next.hiddenPanelIds,
        runFloatingPanels: next.floatingPanels,
        runUpperSplitRatio: next.upperSplitRatio,
        runColumnRatios: next.columnRatios,
      }
    : {
        ...layout,
        zones: next.zones,
        hiddenPanelIds: next.hiddenPanelIds,
        floatingPanels: next.floatingPanels,
        upperSplitRatio: next.upperSplitRatio,
        columnRatios: next.columnRatios,
      };
}

export const PANEL_LAYOUT_VERSION = 6;
export const DEFAULT_UPPER_SPLIT_RATIO = 1;
export const DEFAULT_RUN_UPPER_SPLIT_RATIO = 0.58;
export const DEFAULT_DESIGN_COLUMN_RATIOS: WorkspaceColumnRatios = {
  left: [1, 0, 0],
  right: [1, 0, 0],
};
export const DEFAULT_RUN_COLUMN_RATIOS: WorkspaceColumnRatios = {
  left: [1, 0, 0],
  right: [0.58, 0.42, 0],
};
export const DEFAULT_RIGHT_PANEL_WIDTH = 440;
export const DEFAULT_LEFT_PANEL_WIDTH = 280;
export const DEFAULT_RUN_LEFT_PANEL_WIDTH = 360;
export const DEFAULT_BOTTOM_PANEL_HEIGHT = 36;
export const DEFAULT_TOOLBAR_VISIBILITY = {
  main: true,
  arrange: true,
  arrangeLong: false,
  modifiers: true,
  docking: true,
  numericEdits: true,
  textOptions: true,
  tools: true,
} as const;

export type ToolbarId = keyof typeof DEFAULT_TOOLBAR_VISIBILITY;
export type ToolbarVisibility = Record<ToolbarId, boolean>;

export function normalizeToolbarVisibility(
  visibility?: Partial<Record<string, boolean>> | null,
): ToolbarVisibility {
  const normalized: ToolbarVisibility = { ...DEFAULT_TOOLBAR_VISIBILITY };
  if (!visibility) return normalized;
  for (const key of Object.keys(DEFAULT_TOOLBAR_VISIBILITY) as ToolbarId[]) {
    const value = visibility[key];
    if (typeof value === 'boolean') {
      normalized[key] = value;
    }
  }
  return normalized;
}

export function createDefaultLayout(): PanelLayoutState {
  const def = getDefaultLayout();
  return {
    layoutVersion: PANEL_LAYOUT_VERSION,
    zones: def.zones,
    hiddenPanelIds: def.hiddenPanelIds,
    floatingPanels: [],
    upperSplitRatio: DEFAULT_UPPER_SPLIT_RATIO,
    columnRatios: {
      left: [...DEFAULT_DESIGN_COLUMN_RATIOS.left],
      right: [...DEFAULT_DESIGN_COLUMN_RATIOS.right],
    },
    runZones: def.runZones,
    runHiddenPanelIds: def.runHiddenPanelIds,
    runFloatingPanels: [],
    runUpperSplitRatio: DEFAULT_RUN_UPPER_SPLIT_RATIO,
    runColumnRatios: {
      left: [...DEFAULT_RUN_COLUMN_RATIOS.left],
      right: [...DEFAULT_RUN_COLUMN_RATIOS.right],
    },
    rightPanelWidth: DEFAULT_RIGHT_PANEL_WIDTH,
    leftPanelWidth: DEFAULT_LEFT_PANEL_WIDTH,
    bottomPanelHeight: DEFAULT_BOTTOM_PANEL_HEIGHT,
    sidePanelsVisible: true,
    toolbarVisibility: { ...DEFAULT_TOOLBAR_VISIBILITY },
  };
}
