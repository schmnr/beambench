import type { PhysicalDockZone } from './panelRegistry';
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

export interface PanelLayoutState {
  zones: Record<PhysicalDockZone, ZoneState>;
  hiddenPanelIds: string[];
  floatingPanels: FloatingPanelState[];
  upperSplitRatio: number;   // 0-1, default 0.6
  rightPanelWidth: number;   // px, default 384
  leftPanelWidth: number;    // px, default 280
  bottomPanelHeight: number; // px, default 200
  sidePanelsVisible: boolean; // global side-panel collapsed/expanded state
  toolbarVisibility: ToolbarVisibility;
}

export const DEFAULT_UPPER_SPLIT_RATIO = 0.6;
export const DEFAULT_RIGHT_PANEL_WIDTH = 440;
export const DEFAULT_LEFT_PANEL_WIDTH = 280;
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
    zones: def.zones,
    hiddenPanelIds: def.hiddenPanelIds,
    floatingPanels: [],
    upperSplitRatio: DEFAULT_UPPER_SPLIT_RATIO,
    rightPanelWidth: DEFAULT_RIGHT_PANEL_WIDTH,
    leftPanelWidth: DEFAULT_LEFT_PANEL_WIDTH,
    bottomPanelHeight: DEFAULT_BOTTOM_PANEL_HEIGHT,
    sidePanelsVisible: true,
    toolbarVisibility: { ...DEFAULT_TOOLBAR_VISIBILITY },
  };
}
