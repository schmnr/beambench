import { describe, it, expect } from 'vitest';
import {
  createDefaultLayout,
  normalizeToolbarVisibility,
  DEFAULT_TOOLBAR_VISIBILITY,
  DEFAULT_UPPER_SPLIT_RATIO,
  DEFAULT_RIGHT_PANEL_WIDTH,
} from '../layoutState';
import type { FloatingPanelState } from '../layoutState';

describe('layoutState', () => {
  it('createDefaultLayout has correct defaults', () => {
    const layout = createDefaultLayout();
    expect(layout.upperSplitRatio).toBe(DEFAULT_UPPER_SPLIT_RATIO);
    expect(layout.rightPanelWidth).toBe(DEFAULT_RIGHT_PANEL_WIDTH);
    expect(layout.hiddenPanelIds).toEqual(['measurement', 'camera', 'art_library', 'connection_diagnostics']);
    expect(layout.toolbarVisibility).toEqual(DEFAULT_TOOLBAR_VISIBILITY);
  });

  it('DEFAULT_UPPER_SPLIT_RATIO is 0.6', () => {
    expect(DEFAULT_UPPER_SPLIT_RATIO).toBe(0.6);
  });

  it('DEFAULT_RIGHT_PANEL_WIDTH is 440', () => {
    expect(DEFAULT_RIGHT_PANEL_WIDTH).toBe(440);
  });

  it('createDefaultLayout zones have correct panels', () => {
    const layout = createDefaultLayout();
    expect(layout.zones['upper-right'].panelIds).toHaveLength(5);
    expect(layout.zones['lower-right'].panelIds).toHaveLength(2);
    expect(layout.zones['left'].panelIds).toHaveLength(0);
    expect(layout.zones['bottom'].panelIds).toHaveLength(0);
  });

  it('upper zone default active tab is cuts_layers', () => {
    const layout = createDefaultLayout();
    expect(layout.zones['upper-right'].activeTab).toBe('cuts_layers');
  });

  it('lower zone default active tab is laser', () => {
    const layout = createDefaultLayout();
    expect(layout.zones['lower-right'].activeTab).toBe('laser');
  });

  it('createDefaultLayout has empty floatingPanels', () => {
    const layout = createDefaultLayout();
    expect(layout.floatingPanels).toEqual([]);
  });

  it('normalizes partial toolbar visibility over defaults', () => {
    expect(normalizeToolbarVisibility({ arrangeLong: true, docking: false, unknown: true })).toEqual({
      ...DEFAULT_TOOLBAR_VISIBILITY,
      arrangeLong: true,
      docking: false,
    });
  });

  it('FloatingPanelState is constructible', () => {
    const fp: FloatingPanelState = {
      panelId: 'test',
      x: 100,
      y: 200,
      width: 300,
      height: 400,
      zIndex: 1,
    };
    expect(fp.panelId).toBe('test');
    expect(fp.zIndex).toBe(1);
  });
});
