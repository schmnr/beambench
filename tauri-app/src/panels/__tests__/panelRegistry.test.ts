import { describe, it, expect } from 'vitest';
import { PANEL_REGISTRY, getPanelById, getDefaultLayout } from '../panelRegistry';

describe('panelRegistry', () => {
  it('has 10 registered panels', () => {
    expect(PANEL_REGISTRY).toHaveLength(10);
  });

  it('all panel ids are unique', () => {
    const ids = PANEL_REGISTRY.map((p) => p.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it('getPanelById returns correct panel', () => {
    const panel = getPanelById('cuts_layers');
    expect(panel).toBeDefined();
    expect(panel!.title).toBe('Layers');
    expect(panel!.defaultZone).toBe('top-right');
  });

  it('resolves duplicated panel instance ids to their panel type', () => {
    expect(getPanelById('connection_diagnostics::2')?.id).toBe('connection_diagnostics');
  });

  it('getPanelById returns undefined for unknown id', () => {
    expect(getPanelById('nonexistent')).toBeUndefined();
  });

  it('getDefaultLayout produces valid state', () => {
    const layout = getDefaultLayout();
    expect(layout.zones['top-right'].panelIds).toHaveLength(2);
    expect(layout.zones['middle-right'].panelIds).toHaveLength(0);
    expect(layout.zones['left'].panelIds).toHaveLength(0);
    expect(layout.zones['bottom'].panelIds).toHaveLength(0);
    expect(layout.zones['top-right'].activeTab).toBe('cuts_layers');
    expect(layout.zones['middle-right'].activeTab).toBe('');
    expect(layout.runZones['top-right'].activeTab).toBe('laser');
    expect(layout.hiddenPanelIds).toEqual([
      'move',
      'console',
      'macros',
      'laser',
      'material',
      'camera',
      'art_library',
      'connection_diagnostics',
    ]);
  });

  it('top-right zone contains correct panels', () => {
    const layout = getDefaultLayout();
    expect(layout.zones['top-right'].panelIds).toEqual(['cuts_layers', 'properties']);
  });

  it('middle-right zone contains correct panels', () => {
    const layout = getDefaultLayout();
    expect(layout.zones['middle-right'].panelIds).toEqual([]);
  });

  it('uses run-focused defaults in Run mode', () => {
    const layout = getDefaultLayout();
    expect(layout.runZones['top-left'].panelIds).toEqual(['move']);
    expect(layout.runZones['top-right'].panelIds).toEqual(['laser']);
    expect(layout.runZones['middle-right'].panelIds).toEqual([
      'camera', 'macros', 'console',
    ]);
  });

  it('bottom zone starts empty (color palette retired for layer tabs)', () => {
    const layout = getDefaultLayout();
    expect(layout.zones['bottom'].panelIds).toEqual([]);
  });

  it('all panels have supportsFloat defined', () => {
    for (const panel of PANEL_REGISTRY) {
      expect(typeof panel.supportsFloat).toBe('boolean');
    }
  });

  it('panels with supportsFloat have defaultFloatSize', () => {
    for (const panel of PANEL_REGISTRY) {
      if (panel.supportsFloat) {
        expect(panel.defaultFloatSize).toBeDefined();
        expect(panel.defaultFloatSize!.w).toBeGreaterThan(0);
        expect(panel.defaultFloatSize!.h).toBeGreaterThan(0);
      }
    }
  });

  it('getDefaultLayout includes empty floatingPanels', () => {
    const layout = getDefaultLayout();
    expect(layout.floatingPanels).toEqual([]);
  });
});
