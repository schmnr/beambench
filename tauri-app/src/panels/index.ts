export { PANEL_REGISTRY, getPanelById, getDefaultLayout } from './panelRegistry';
export type { DockZone, PhysicalDockZone, PanelDefinition } from './panelRegistry';
export {
  createDefaultLayout,
  normalizeToolbarVisibility,
  DEFAULT_UPPER_SPLIT_RATIO,
  DEFAULT_RIGHT_PANEL_WIDTH,
  DEFAULT_LEFT_PANEL_WIDTH,
  DEFAULT_BOTTOM_PANEL_HEIGHT,
  DEFAULT_TOOLBAR_VISIBILITY,
} from './layoutState';
export type { ZoneState, FloatingPanelState, PanelLayoutState, ToolbarId, ToolbarVisibility } from './layoutState';
export { PANEL_COMPONENTS } from './panelComponents';
