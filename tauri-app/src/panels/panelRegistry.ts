export type PanelColumnSide = 'left' | 'right';
export type PanelColumnPosition = 'top' | 'middle' | 'bottom';

/** Vertical dock zones. Legacy left/bottom keys remain readable for migrations. */
export type ColumnDockZone = `${PanelColumnPosition}-${PanelColumnSide}`;
export type PhysicalDockZone = ColumnDockZone | 'left' | 'bottom';

export const COLUMN_ZONES: Record<PanelColumnSide, readonly ColumnDockZone[]> = {
  left: ['top-left', 'middle-left', 'bottom-left'],
  right: ['top-right', 'middle-right', 'bottom-right'],
};

export const ALL_PHYSICAL_DOCK_ZONES: readonly PhysicalDockZone[] = [
  ...COLUMN_ZONES.left,
  ...COLUMN_ZONES.right,
  'left',
  'bottom',
];

export function getColumnForZone(zone: PhysicalDockZone): PanelColumnSide | null {
  if (COLUMN_ZONES.left.includes(zone as ColumnDockZone)) return 'left';
  if (COLUMN_ZONES.right.includes(zone as ColumnDockZone)) return 'right';
  return null;
}

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

const PANEL_INSTANCE_SEPARATOR = '::';

/** Existing layouts use the panel type as their first instance ID. */
export function getPanelTypeId(panelInstanceId: string): string {
  return panelInstanceId.split(PANEL_INSTANCE_SEPARATOR, 1)[0];
}

/** Mint a stable, readable ID for another instance of a registered panel type. */
export function createPanelInstanceId(panelTypeId: string, occupiedIds: Iterable<string>): string {
  const occupied = new Set(occupiedIds);
  if (!occupied.has(panelTypeId)) return panelTypeId;
  let sequence = 2;
  while (occupied.has(`${panelTypeId}${PANEL_INSTANCE_SEPARATOR}${sequence}`)) sequence += 1;
  return `${panelTypeId}${PANEL_INSTANCE_SEPARATOR}${sequence}`;
}

const DESIGN_DEFAULT_ZONES: Record<PhysicalDockZone, string[]> = {
  'top-left': [],
  'middle-left': [],
  'bottom-left': [],
  'top-right': ['cuts_layers', 'properties'],
  'middle-right': [],
  'bottom-right': [],
  left: [],
  bottom: [],
};

const RUN_DEFAULT_ZONES: Record<PhysicalDockZone, string[]> = {
  'top-left': ['move'],
  'middle-left': [],
  'bottom-left': [],
  'top-right': ['laser'],
  'middle-right': ['camera', 'macros', 'console'],
  'bottom-right': [],
  left: [],
  bottom: [],
};

export const PANEL_REGISTRY: PanelDefinition[] = [
  { id: 'cuts_layers', title: 'Layers', titleKey: 'panels.registry.cuts_layers', defaultZone: 'top-right', defaultVisible: true, supportsClose: true, supportsFloat: true, defaultFloatSize: { w: 384, h: 300 }, minFloatSize: { w: 200, h: 150 } },
  { id: 'move', title: 'Move', titleKey: 'panels.registry.move', defaultZone: 'top-left', defaultVisible: true, supportsClose: true, supportsFloat: true, defaultFloatSize: { w: 320, h: 260 }, minFloatSize: { w: 200, h: 150 } },
  { id: 'console', title: 'Console', titleKey: 'panels.registry.console', defaultZone: 'middle-right', defaultVisible: true, supportsClose: true, supportsFloat: true, defaultFloatSize: { w: 420, h: 300 }, minFloatSize: { w: 250, h: 150 } },
  { id: 'macros', title: 'Macros', titleKey: 'panels.registry.macros', defaultZone: 'middle-right', defaultVisible: true, supportsClose: true, supportsFloat: true, defaultFloatSize: { w: 320, h: 260 }, minFloatSize: { w: 200, h: 150 } },
  { id: 'properties', title: 'Properties', titleKey: 'panels.registry.properties', defaultZone: 'top-right', defaultVisible: true, supportsClose: true, supportsFloat: true, defaultFloatSize: { w: 384, h: 400 }, minFloatSize: { w: 250, h: 200 } },
  { id: 'laser', title: 'Laser Control', titleKey: 'panels.registry.laser', defaultZone: 'top-right', defaultVisible: true, supportsClose: true, supportsFloat: true, defaultFloatSize: { w: 384, h: 360 }, minFloatSize: { w: 250, h: 200 } },
  { id: 'material', title: 'Material Library', titleKey: 'panels.registry.material', defaultZone: 'middle-right', defaultVisible: true, supportsClose: true, supportsFloat: true, defaultFloatSize: { w: 384, h: 360 }, minFloatSize: { w: 250, h: 200 } },
  { id: 'camera', title: 'Camera', titleKey: 'panels.registry.camera', defaultZone: 'floating', defaultVisible: false, supportsClose: true, supportsFloat: true, defaultFloatSize: { w: 420, h: 400 }, minFloatSize: { w: 320, h: 300 } },
  { id: 'art_library', title: 'Art Library', titleKey: 'panels.registry.art_library', defaultZone: 'middle-right', defaultVisible: false, supportsClose: true, supportsFloat: true, defaultFloatSize: { w: 420, h: 400 }, minFloatSize: { w: 300, h: 250 } },
  { id: 'connection_diagnostics', title: 'Connection Diagnostics', titleKey: 'panels.registry.connection_diagnostics', defaultZone: 'middle-right', defaultVisible: false, supportsClose: true, supportsFloat: true, defaultFloatSize: { w: 520, h: 420 }, minFloatSize: { w: 360, h: 260 } },
  { id: 'notes', title: 'Project Notes', titleKey: 'dialog.notes.title', defaultZone: 'bottom', defaultVisible: false, supportsClose: true, supportsFloat: true, defaultFloatSize: { w: 520, h: 260 }, minFloatSize: { w: 320, h: 180 } },
];

export function getPanelById(id: string): PanelDefinition | undefined {
  const panelTypeId = getPanelTypeId(id);
  return PANEL_REGISTRY.find((p) => p.id === panelTypeId);
}

export function getDefaultLayout() {
  const buildZones = (defaults: Record<PhysicalDockZone, string[]>) =>
    Object.fromEntries(
      (Object.keys(defaults) as PhysicalDockZone[]).map((zone) => {
        const panelIds = defaults[zone];
        return [zone, { panelIds: [...panelIds], activeTab: panelIds[0] ?? '' }];
      }),
    ) as Record<PhysicalDockZone, { panelIds: string[]; activeTab: string }>;
  const hiddenFor = (defaults: Record<PhysicalDockZone, string[]>) => {
    const defaultIds = new Set(Object.values(defaults).flat());
    return PANEL_REGISTRY.filter((panel) => !defaultIds.has(panel.id)).map((panel) => panel.id);
  };

  return {
    zones: buildZones(DESIGN_DEFAULT_ZONES),
    hiddenPanelIds: hiddenFor(DESIGN_DEFAULT_ZONES),
    runZones: buildZones(RUN_DEFAULT_ZONES),
    runHiddenPanelIds: hiddenFor(RUN_DEFAULT_ZONES),
    floatingPanels: [] as Array<{ panelId: string; x: number; y: number; width: number; height: number; zIndex: number }>,
    runFloatingPanels: [] as Array<{ panelId: string; x: number; y: number; width: number; height: number; zIndex: number }>,
  };
}
