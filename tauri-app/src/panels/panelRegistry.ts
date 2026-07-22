/** Physical dock zones that hold tabbed panels. */
export type PhysicalDockZone = 'upper-right' | 'lower-right' | 'left' | 'bottom';

/** All possible zones a panel can live in, including floating. */
export type DockZone = PhysicalDockZone | 'floating';

export interface PanelDefinition {
  id: string;
  /** English title; also the i18n fallback when titleKey is missing. */
  title: string;
  /** i18n key resolving to the localized panel title. */
  titleKey: string;
  defaultZone: DockZone;
  defaultVisible: boolean;
  supportsClose: boolean;
  supportsFloat: boolean;
  defaultFloatSize?: { w: number; h: number };
  minFloatSize?: { w: number; h: number };
}

export const PANEL_REGISTRY: PanelDefinition[] = [
  { id: 'cuts_layers', title: 'Cuts / Layers', titleKey: 'panels.registry.cuts_layers', defaultZone: 'upper-right', defaultVisible: true, supportsClose: true, supportsFloat: true, defaultFloatSize: { w: 384, h: 300 }, minFloatSize: { w: 200, h: 150 } },
  { id: 'move', title: 'Move', titleKey: 'panels.registry.move', defaultZone: 'upper-right', defaultVisible: true, supportsClose: true, supportsFloat: true, defaultFloatSize: { w: 320, h: 260 }, minFloatSize: { w: 200, h: 150 } },
  { id: 'console', title: 'Console', titleKey: 'panels.registry.console', defaultZone: 'upper-right', defaultVisible: true, supportsClose: true, supportsFloat: true, defaultFloatSize: { w: 420, h: 300 }, minFloatSize: { w: 250, h: 150 } },
  { id: 'macros', title: 'Macros', titleKey: 'panels.registry.macros', defaultZone: 'upper-right', defaultVisible: true, supportsClose: true, supportsFloat: true, defaultFloatSize: { w: 320, h: 260 }, minFloatSize: { w: 200, h: 150 } },
  { id: 'properties', title: 'Shape Properties', titleKey: 'panels.registry.properties', defaultZone: 'upper-right', defaultVisible: true, supportsClose: true, supportsFloat: true, defaultFloatSize: { w: 384, h: 400 }, minFloatSize: { w: 250, h: 200 } },
  { id: 'measurement', title: 'Measurement', titleKey: 'panels.registry.measurement', defaultZone: 'upper-right', defaultVisible: false, supportsClose: true, supportsFloat: true, defaultFloatSize: { w: 360, h: 360 }, minFloatSize: { w: 260, h: 220 } },
  { id: 'laser', title: 'Laser Control', titleKey: 'panels.registry.laser', defaultZone: 'lower-right', defaultVisible: true, supportsClose: true, supportsFloat: true, defaultFloatSize: { w: 384, h: 360 }, minFloatSize: { w: 250, h: 200 } },
  { id: 'material', title: 'Material Library', titleKey: 'panels.registry.material', defaultZone: 'lower-right', defaultVisible: true, supportsClose: true, supportsFloat: true, defaultFloatSize: { w: 384, h: 360 }, minFloatSize: { w: 250, h: 200 } },
  { id: 'camera', title: 'Camera', titleKey: 'panels.registry.camera', defaultZone: 'floating', defaultVisible: false, supportsClose: true, supportsFloat: true, defaultFloatSize: { w: 420, h: 400 }, minFloatSize: { w: 320, h: 300 } },
  { id: 'art_library', title: 'Art Library', titleKey: 'panels.registry.art_library', defaultZone: 'lower-right', defaultVisible: false, supportsClose: true, supportsFloat: true, defaultFloatSize: { w: 420, h: 400 }, minFloatSize: { w: 300, h: 250 } },
  { id: 'connection_diagnostics', title: 'Connection Diagnostics', titleKey: 'panels.registry.connection_diagnostics', defaultZone: 'lower-right', defaultVisible: false, supportsClose: true, supportsFloat: true, defaultFloatSize: { w: 520, h: 420 }, minFloatSize: { w: 360, h: 260 } },
];

export function getPanelById(id: string): PanelDefinition | undefined {
  return PANEL_REGISTRY.find((p) => p.id === id);
}

export function getDefaultLayout() {
  const upper = PANEL_REGISTRY.filter((p) => p.defaultZone === 'upper-right' && p.defaultVisible);
  const lower = PANEL_REGISTRY.filter((p) => p.defaultZone === 'lower-right' && p.defaultVisible);
  const left = PANEL_REGISTRY.filter((p) => p.defaultZone === 'left' && p.defaultVisible);
  const bottom = PANEL_REGISTRY.filter((p) => p.defaultZone === 'bottom' && p.defaultVisible);
  const allDockedIds = new Set([
    ...upper.map((p) => p.id), ...lower.map((p) => p.id),
    ...left.map((p) => p.id), ...bottom.map((p) => p.id),
  ]);
  const hidden = PANEL_REGISTRY.filter((p) => !p.defaultVisible && !allDockedIds.has(p.id)).map((p) => p.id);
  return {
    zones: {
      'upper-right': { panelIds: upper.map((p) => p.id), activeTab: upper[0]?.id ?? '' },
      'lower-right': { panelIds: lower.map((p) => p.id), activeTab: lower[0]?.id ?? '' },
      'left': { panelIds: left.map((p) => p.id), activeTab: left[0]?.id ?? '' },
      'bottom': { panelIds: bottom.map((p) => p.id), activeTab: bottom[0]?.id ?? '' },
    },
    hiddenPanelIds: hidden,
    floatingPanels: [] as Array<{ panelId: string; x: number; y: number; width: number; height: number; zIndex: number }>,
  };
}
