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
    expect(layout.layoutVersion).toBe(6);
    expect(layout.upperSplitRatio).toBe(DEFAULT_UPPER_SPLIT_RATIO);
    expect(layout.rightPanelWidth).toBe(DEFAULT_RIGHT_PANEL_WIDTH);
    expect(layout.hiddenPanelIds).toEqual([
      'move',
      'console',
      'macros',
      'laser',
      'material',
      'camera',
      'art_library',
      'connection_diagnostics',
      'notes',
    ]);
    expect(layout.toolbarVisibility).toEqual(DEFAULT_TOOLBAR_VISIBILITY);
  });

  it('collapses the secondary Design dock by default', () => {
    expect(DEFAULT_UPPER_SPLIT_RATIO).toBe(1);
  });

  it('DEFAULT_RIGHT_PANEL_WIDTH is 440', () => {
    expect(DEFAULT_RIGHT_PANEL_WIDTH).toBe(440);
  });

  it('createDefaultLayout zones have correct panels', () => {
    const layout = createDefaultLayout();
    expect(layout.zones['top-right'].panelIds).toEqual(['cuts_layers', 'properties']);
    expect(layout.zones['middle-right'].panelIds).toHaveLength(0);
    expect(layout.zones['bottom-right'].panelIds).toHaveLength(0);
    expect(layout.zones['top-left'].panelIds).toHaveLength(0);
    expect(layout.zones['left'].panelIds).toHaveLength(0);
    expect(layout.zones['bottom'].panelIds).toHaveLength(0);
  });

  it('upper zone default active tab is cuts_layers', () => {
    const layout = createDefaultLayout();
    expect(layout.zones['top-right'].activeTab).toBe('cuts_layers');
  });

  it('Run has its own machine-oriented defaults', () => {
    const layout = createDefaultLayout();
    expect(layout.runZones['top-left']).toEqual({ panelIds: ['move'], activeTab: 'move' });
    expect(layout.runZones['top-right']).toEqual({ panelIds: ['laser'], activeTab: 'laser' });
    expect(layout.runZones['middle-right']).toEqual({
      panelIds: ['camera', 'macros', 'console'],
      activeTab: 'camera',
    });
    expect(layout.runColumnRatios).toEqual({ left: [1, 0, 0], right: [0.58, 0.42, 0] });
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
