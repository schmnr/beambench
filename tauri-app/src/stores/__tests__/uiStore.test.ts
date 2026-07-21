import { describe, it, expect, beforeEach, vi } from 'vitest';
import {
  DEFAULT_DOCK_SETTINGS,
  DEFAULT_NEST_SETTINGS,
  renderOptionsFromViewStyle,
  useUiStore,
  viewStyleFromRenderOptions,
} from '../uiStore';
import { createDefaultLayout, DEFAULT_TOOLBAR_VISIBILITY } from '../../panels';
import { appService } from '../../services/appService';
import { useMeasurementStore } from '../measurementStore';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(null) }));

describe('uiStore', () => {
  beforeEach(() => {
    useUiStore.setState({
      panelLayout: createDefaultLayout(),
      nextFloatingZIndex: 1,
      activeTool: 'select',
      viewStyle: 'wireframe_smooth',
      sidePanelsVisible: true,
      cameraWindowOpen: false,
      dockSettings: { ...DEFAULT_DOCK_SETTINGS },
      nestSettings: { ...DEFAULT_NEST_SETTINGS },
      nestingInProgress: false,
    });
    useMeasurementStore.getState().clear();
    vi.restoreAllMocks();
  });

  it('accepts new tool types via setActiveTool', () => {
    const newTools = ['line', 'polygon', 'trim', 'tabs', 'measure', 'laser_position'] as const;
    for (const tool of newTools) {
      useUiStore.getState().setActiveTool(tool);
      expect(useUiStore.getState().activeTool).toBe(tool);
    }
  });

  it('shows and persists the Measurement panel when Measure is activated or re-activated', () => {
    const persistLayout = vi.spyOn(appService, 'persistLayout').mockImplementation(() => undefined);
    useUiStore.setState({
      activeTool: 'measure',
      sidePanelsVisible: false,
      panelLayout: {
        ...createDefaultLayout(),
        sidePanelsVisible: false,
      },
    });

    useUiStore.getState().setActiveTool('measure');

    const state = useUiStore.getState();
    expect(state.activeTool).toBe('measure');
    expect(state.sidePanelsVisible).toBe(true);
    expect(state.panelLayout.hiddenPanelIds).not.toContain('measurement');
    expect(state.panelLayout.zones['upper-right'].panelIds).toContain('measurement');
    expect(state.panelLayout.zones['upper-right'].activeTab).toBe('measurement');
    expect(persistLayout).toHaveBeenCalledWith(state.panelLayout);
  });

  it('clears measurement state when switching away from Measure', () => {
    useUiStore.setState({ activeTool: 'measure' });
    useMeasurementStore.getState().setDrag({
      start: { x: 0, y: 0 },
      end: { x: 10, y: 0 },
      dxMm: 10,
      dyMm: 0,
      lengthMm: 10,
      angleDeg: 0,
    });

    useUiStore.getState().setActiveTool('select');

    expect(useMeasurementStore.getState().state.type).toBe('idle');
  });

  it('changes viewStyle', () => {
    useUiStore.getState().setViewStyle('filled_smooth');
    expect(useUiStore.getState().viewStyle).toBe('filled_smooth');

    useUiStore.getState().setViewStyle('wireframe_coarse');
    expect(useUiStore.getState().viewStyle).toBe('wireframe_coarse');
  });

  it('maps view styles to canvas render options', () => {
    expect(renderOptionsFromViewStyle('wireframe_coarse')).toEqual({
      antialiasing: false,
      filledRendering: false,
    });
    expect(renderOptionsFromViewStyle('wireframe_smooth')).toEqual({
      antialiasing: true,
      filledRendering: false,
    });
    expect(renderOptionsFromViewStyle('filled_coarse')).toEqual({
      antialiasing: false,
      filledRendering: true,
    });
    expect(renderOptionsFromViewStyle('filled_smooth')).toEqual({
      antialiasing: true,
      filledRendering: true,
    });
  });

  it('maps persisted render options to view styles', () => {
    expect(viewStyleFromRenderOptions({ antialiasing: false, filledRendering: false })).toBe('wireframe_coarse');
    expect(viewStyleFromRenderOptions({ antialiasing: true, filledRendering: false })).toBe('wireframe_smooth');
    expect(viewStyleFromRenderOptions({ antialiasing: false, filledRendering: true })).toBe('filled_coarse');
    expect(viewStyleFromRenderOptions({ antialiasing: true, filledRendering: true })).toBe('filled_smooth');
  });

  it('toggles sidePanelsVisible', () => {
    expect(useUiStore.getState().sidePanelsVisible).toBe(true);

    useUiStore.getState().toggleSidePanels();
    expect(useUiStore.getState().sidePanelsVisible).toBe(false);

    useUiStore.getState().toggleSidePanels();
    expect(useUiStore.getState().sidePanelsVisible).toBe(true);
  });

  it('has correct defaults', () => {
    const state = useUiStore.getState();
    expect(state.viewStyle).toBe('wireframe_smooth');
    expect(state.sidePanelsVisible).toBe(true);
    expect(state.activeTool).toBe('select');
    expect(state.panelLayout.toolbarVisibility).toEqual(DEFAULT_TOOLBAR_VISIBILITY);
    expect(state.nestSettings).toEqual({
      paddingMm: 0,
      allowRotation: true,
      allowMirror: false,
      lockInnerObjects: false,
      timeLimitMs: 15000,
      rotationStepDeg: 15,
    });
    expect(state.nestingInProgress).toBe(false);
    expect(state.nestSettings).not.toBe(state.dockSettings);
  });

  it('keeps nest settings separate from dock settings', () => {
    useUiStore.getState().updateDockSettings({ paddingMm: 4 });
    useUiStore.getState().updateNestSettings({ paddingMm: 2, allowRotation: false, allowMirror: true });

    expect(useUiStore.getState().dockSettings.paddingMm).toBe(4);
    expect(useUiStore.getState().nestSettings).toMatchObject({
      paddingMm: 2,
      allowRotation: false,
      allowMirror: true,
    });
  });

  it('tracks nesting pending state', () => {
    useUiStore.getState().setNestingInProgress(true);
    expect(useUiStore.getState().nestingInProgress).toBe(true);
    useUiStore.getState().setNestingInProgress(false);
    expect(useUiStore.getState().nestingInProgress).toBe(false);
  });

  it('toggles toolbar visibility and reset restores defaults', () => {
    useUiStore.getState().toggleToolbarVisibility('docking');
    expect(useUiStore.getState().panelLayout.toolbarVisibility.docking).toBe(false);

    useUiStore.getState().resetLayout();
    expect(useUiStore.getState().panelLayout.toolbarVisibility).toEqual(DEFAULT_TOOLBAR_VISIBILITY);
    expect(useUiStore.getState().sidePanelsVisible).toBe(true);
  });

  it('normalizes root sidePanelsVisible when replacing panel layout', () => {
    useUiStore.getState().setPanelLayout({
      ...createDefaultLayout(),
      sidePanelsVisible: false,
    });

    expect(useUiStore.getState().sidePanelsVisible).toBe(false);
    expect(useUiStore.getState().panelLayout.sidePanelsVisible).toBe(false);
  });

  it('drops removed panels when replacing panel layout', () => {
    const layout = createDefaultLayout();
    useUiStore.getState().setPanelLayout({
      ...layout,
      zones: {
        ...layout.zones,
        'lower-right': {
          panelIds: [...layout.zones['lower-right'].panelIds, 'variable_text'],
          activeTab: 'variable_text',
        },
      },
      hiddenPanelIds: [...layout.hiddenPanelIds, 'variable_text'],
      floatingPanels: [
        { panelId: 'variable_text', x: 10, y: 10, width: 320, height: 240, zIndex: 1 },
      ],
    });

    const next = useUiStore.getState().panelLayout;
    expect(next.zones['lower-right'].panelIds).not.toContain('variable_text');
    expect(next.zones['lower-right'].activeTab).toBe('laser');
    expect(next.hiddenPanelIds).not.toContain('variable_text');
    expect(next.floatingPanels).toHaveLength(0);
  });

  // --- Floating panel actions ---

  describe('floatPanel', () => {
    it('removes panel from zone and adds to floatingPanels', () => {
      const { panelLayout } = useUiStore.getState();
      expect(panelLayout.zones['upper-right'].panelIds).toContain('console');

      useUiStore.getState().floatPanel('console', 100, 200, 420, 300);

      const state = useUiStore.getState();
      expect(state.panelLayout.zones['upper-right'].panelIds).not.toContain('console');
      expect(state.panelLayout.floatingPanels).toHaveLength(1);
      expect(state.panelLayout.floatingPanels[0].panelId).toBe('console');
      expect(state.panelLayout.floatingPanels[0].x).toBe(100);
      expect(state.panelLayout.floatingPanels[0].y).toBe(200);
      expect(state.panelLayout.floatingPanels[0].width).toBe(420);
      expect(state.panelLayout.floatingPanels[0].height).toBe(300);
    });

    it('fixes activeTab if floating the active tab', () => {
      useUiStore.getState().setZoneActiveTab('upper-right', 'console');
      useUiStore.getState().floatPanel('console', 100, 200, 420, 300);

      const state = useUiStore.getState();
      expect(state.panelLayout.zones['upper-right'].activeTab).not.toBe('console');
      expect(state.panelLayout.zones['upper-right'].activeTab).toBe('cuts_layers');
    });

    it('assigns incrementing z-index', () => {
      useUiStore.getState().floatPanel('console', 0, 0, 420, 300);
      useUiStore.getState().floatPanel('macros', 50, 50, 320, 260);

      const fps = useUiStore.getState().panelLayout.floatingPanels;
      expect(fps[0].zIndex).toBeLessThan(fps[1].zIndex);
    });
  });

  describe('dockPanel', () => {
    it('removes from floating and adds to zone', () => {
      useUiStore.getState().floatPanel('console', 100, 200, 420, 300);
      expect(useUiStore.getState().panelLayout.floatingPanels).toHaveLength(1);

      useUiStore.getState().dockPanel('console', 'lower-right');

      const state = useUiStore.getState();
      expect(state.panelLayout.floatingPanels).toHaveLength(0);
      expect(state.panelLayout.zones['lower-right'].panelIds).toContain('console');
      expect(state.panelLayout.zones['lower-right'].activeTab).toBe('console');
    });

    it('respects insertIndex', () => {
      useUiStore.getState().floatPanel('console', 100, 200, 420, 300);
      useUiStore.getState().dockPanel('console', 'lower-right', 0);

      const ids = useUiStore.getState().panelLayout.zones['lower-right'].panelIds;
      expect(ids[0]).toBe('console');
    });
  });

  describe('moveFloatingPanel', () => {
    it('updates position', () => {
      useUiStore.getState().floatPanel('console', 100, 200, 420, 300);
      useUiStore.getState().moveFloatingPanel('console', 300, 400);

      const fp = useUiStore.getState().panelLayout.floatingPanels.find((f) => f.panelId === 'console');
      expect(fp?.x).toBe(300);
      expect(fp?.y).toBe(400);
    });
  });

  describe('resizeFloatingPanel', () => {
    it('updates dimensions', () => {
      useUiStore.getState().floatPanel('console', 100, 200, 420, 300);
      useUiStore.getState().resizeFloatingPanel('console', 500, 400);

      const fp = useUiStore.getState().panelLayout.floatingPanels.find((f) => f.panelId === 'console');
      expect(fp?.width).toBe(500);
      expect(fp?.height).toBe(400);
    });

    it('enforces minimum dimensions', () => {
      useUiStore.getState().floatPanel('console', 100, 200, 420, 300);
      useUiStore.getState().resizeFloatingPanel('console', 10, 10);

      const fp = useUiStore.getState().panelLayout.floatingPanels.find((f) => f.panelId === 'console');
      // console minFloatSize is { w: 250, h: 150 }
      expect(fp?.width).toBe(250);
      expect(fp?.height).toBe(150);
    });
  });

  describe('bringToFront', () => {
    it('increments z-index', () => {
      useUiStore.getState().floatPanel('console', 0, 0, 420, 300);
      useUiStore.getState().floatPanel('macros', 50, 50, 320, 260);

      const before = useUiStore.getState().panelLayout.floatingPanels.find((f) => f.panelId === 'console')!.zIndex;
      useUiStore.getState().bringToFront('console');
      const after = useUiStore.getState().panelLayout.floatingPanels.find((f) => f.panelId === 'console')!.zIndex;

      expect(after).toBeGreaterThan(before);
      const macrosZ = useUiStore.getState().panelLayout.floatingPanels.find((f) => f.panelId === 'macros')!.zIndex;
      expect(after).toBeGreaterThan(macrosZ);
    });
  });

  describe('closeFloatingPanel', () => {
    it('preserves floating entry and adds to hiddenPanelIds', () => {
      useUiStore.getState().floatPanel('console', 50, 75, 420, 300);
      expect(useUiStore.getState().panelLayout.floatingPanels).toHaveLength(1);

      useUiStore.getState().closeFloatingPanel('console');

      const state = useUiStore.getState();
      // Entry preserved so reopening restores position/size
      expect(state.panelLayout.floatingPanels).toHaveLength(1);
      expect(state.panelLayout.floatingPanels[0].x).toBe(50);
      expect(state.panelLayout.floatingPanels[0].y).toBe(75);
      expect(state.panelLayout.hiddenPanelIds).toContain('console');
    });
  });

  describe('movePanelBetweenZones', () => {
    it('transfers panel correctly', () => {
      expect(useUiStore.getState().panelLayout.zones['upper-right'].panelIds).toContain('console');

      useUiStore.getState().movePanelBetweenZones('console', 'upper-right', 'lower-right');

      const state = useUiStore.getState();
      expect(state.panelLayout.zones['upper-right'].panelIds).not.toContain('console');
      expect(state.panelLayout.zones['lower-right'].panelIds).toContain('console');
      expect(state.panelLayout.zones['lower-right'].activeTab).toBe('console');
    });

    it('fixes source active tab', () => {
      useUiStore.getState().setZoneActiveTab('upper-right', 'console');
      useUiStore.getState().movePanelBetweenZones('console', 'upper-right', 'lower-right');

      expect(useUiStore.getState().panelLayout.zones['upper-right'].activeTab).not.toBe('console');
    });
  });

  describe('reorderPanelInZone', () => {
    it('changes order within zone', () => {
      const ids = useUiStore.getState().panelLayout.zones['upper-right'].panelIds;
      expect(ids.indexOf('console')).toBe(2); // default index

      useUiStore.getState().reorderPanelInZone('console', 'upper-right', 0);

      const newIds = useUiStore.getState().panelLayout.zones['upper-right'].panelIds;
      expect(newIds[0]).toBe('console');
    });
  });

  describe('resetLayout', () => {
    it('clears floatingPanels and resets nextFloatingZIndex', () => {
      useUiStore.getState().floatPanel('console', 0, 0, 420, 300);
      expect(useUiStore.getState().panelLayout.floatingPanels).toHaveLength(1);

      useUiStore.getState().resetLayout();

      const state = useUiStore.getState();
      expect(state.panelLayout.floatingPanels).toHaveLength(0);
      expect(state.nextFloatingZIndex).toBe(1);
      expect(state.panelLayout.zones['upper-right'].panelIds).toContain('console');
    });
  });

  describe('togglePanelVisibility for floating panel', () => {
    it('preserves floatingPanels entry when hiding and restores on show', () => {
      useUiStore.getState().floatPanel('console', 50, 75, 420, 300);
      expect(useUiStore.getState().panelLayout.floatingPanels).toHaveLength(1);

      // Hide — entry preserved, panel added to hiddenPanelIds
      useUiStore.getState().togglePanelVisibility('console');
      let state = useUiStore.getState();
      expect(state.panelLayout.floatingPanels).toHaveLength(1);
      expect(state.panelLayout.hiddenPanelIds).toContain('console');

      // Show — entry still there, panel removed from hiddenPanelIds, position preserved
      useUiStore.getState().togglePanelVisibility('console');
      state = useUiStore.getState();
      expect(state.panelLayout.floatingPanels).toHaveLength(1);
      expect(state.panelLayout.hiddenPanelIds).not.toContain('console');
      expect(state.panelLayout.floatingPanels[0].x).toBe(50);
      expect(state.panelLayout.floatingPanels[0].y).toBe(75);
    });

    it('persists panel visibility changes from the store action', () => {
      const persistLayout = vi.spyOn(appService, 'persistLayout').mockImplementation(() => undefined);

      useUiStore.getState().togglePanelVisibility('console');

      expect(persistLayout).toHaveBeenCalledOnce();
      expect(persistLayout).toHaveBeenCalledWith(useUiStore.getState().panelLayout);
    });
  });
});
