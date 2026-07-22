import { describe, it, expect } from 'vitest';
import { PANEL_REGISTRY, getPanelById, getDefaultLayout } from '../panelRegistry';

describe('panelRegistry', () => {
  it('has 11 registered panels', () => {
    expect(PANEL_REGISTRY).toHaveLength(11);
  });

  it('all panel ids are unique', () => {
    const ids = PANEL_REGISTRY.map((p) => p.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it('getPanelById returns correct panel', () => {
    const panel = getPanelById('cuts_layers');
    expect(panel).toBeDefined();
    expect(panel!.title).toBe('Cuts / Layers');
    expect(panel!.defaultZone).toBe('upper-right');
  });

  it('getPanelById returns undefined for unknown id', () => {
    expect(getPanelById('nonexistent')).toBeUndefined();
  });

  it('getDefaultLayout produces valid state', () => {
    const layout = getDefaultLayout();
    expect(layout.zones['upper-right'].panelIds).toHaveLength(5);
    expect(layout.zones['lower-right'].panelIds).toHaveLength(2);
    expect(layout.zones['left'].panelIds).toHaveLength(0);
    expect(layout.zones['bottom'].panelIds).toHaveLength(0);
    expect(layout.zones['upper-right'].activeTab).toBe('cuts_layers');
    expect(layout.zones['lower-right'].activeTab).toBe('laser');
    expect(layout.hiddenPanelIds).toEqual(['measurement', 'camera', 'art_library', 'connection_diagnostics']);
  });

  it('upper-right zone contains correct panels', () => {
    const layout = getDefaultLayout();
    expect(layout.zones['upper-right'].panelIds).toEqual([
      'cuts_layers', 'move', 'console', 'macros', 'properties',
    ]);
  });

  it('lower-right zone contains correct panels', () => {
    const layout = getDefaultLayout();
    expect(layout.zones['lower-right'].panelIds).toEqual(['laser', 'material']);
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
