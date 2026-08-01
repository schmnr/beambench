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
      workspaceMode: 'design',
      cameraWindowOpen: false,
      dockSettings: { ...DEFAULT_DOCK_SETTINGS },
      nestSettings: { ...DEFAULT_NEST_SETTINGS },
      nestingInProgress: false,
    });
    useMeasurementStore.getState().clear();
    vi.restoreAllMocks();
  });

  it('accepts new tool types via setActiveTool', () => {
    const newTools = ['line', 'polygon', 'trim', 'tabs', 'warp', 'measure'] as const;
    for (const tool of newTools) {
      useUiStore.getState().setActiveTool(tool);
      expect(useUiStore.getState().activeTool).toBe(tool);
    }
  });

  it('dismisses Set Start Point when the current tool is explicitly selected again', () => {
    useUiStore.setState({
      activeTool: 'select',
      pendingStartPointObjectId: 'vector-1',
    });

    useUiStore.getState().setActiveTool('select');

    expect(useUiStore.getState().activeTool).toBe('select');
    expect(useUiStore.getState().pendingStartPointObjectId).toBeNull();
  });

  it('returns to Select when entering the read-only Run workspace', () => {
    useUiStore.getState().setActiveTool('rect');

    useUiStore.getState().setWorkspaceMode('run');

    expect(useUiStore.getState().workspaceMode).toBe('run');
    expect(useUiStore.getState().activeTool).toBe('select');
  });

  it('rejects drawing tools while the Run workspace is active', () => {
    useUiStore.getState().setWorkspaceMode('run');

    useUiStore.getState().setActiveTool('rect');

    expect(useUiStore.getState().activeTool).toBe('select');
  });

  it('allows Laser Position only in Run and clears it when returning to Design', () => {
    useUiStore.getState().setActiveTool('laser_position');
    expect(useUiStore.getState().activeTool).toBe('select');

    useUiStore.getState().setWorkspaceMode('run');
    useUiStore.getState().setActiveTool('laser_position');
    expect(useUiStore.getState().activeTool).toBe('laser_position');

    useUiStore.getState().setWorkspaceMode('design');
    expect(useUiStore.getState().activeTool).toBe('select');
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
    expect(state.panelLayout.zones['top-right'].panelIds).toContain('measurement');
    expect(state.panelLayout.zones['top-right'].activeTab).toBe('measurement');
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
        'middle-right': {
          panelIds: [...layout.zones['middle-right'].panelIds, 'variable_text'],
          activeTab: 'variable_text',
        },
      },
      hiddenPanelIds: [...layout.hiddenPanelIds, 'variable_text'],
      floatingPanels: [
        { panelId: 'variable_text', x: 10, y: 10, width: 320, height: 240, zIndex: 1 },
      ],
    });

    const next = useUiStore.getState().panelLayout;
    expect(next.zones['middle-right'].panelIds).not.toContain('variable_text');
    expect(next.zones['middle-right'].activeTab).toBe('');
    expect(next.hiddenPanelIds).not.toContain('variable_text');
    expect(next.floatingPanels).toHaveLength(0);
  });

  // --- Floating panel actions ---

  describe('panel instances', () => {
    it('keeps the bottom dock open at compact usable heights', () => {
      useUiStore.getState().setBottomPanelHeight(80);
      expect(useUiStore.getState().panelLayout.bottomPanelHeight).toBe(80);

      useUiStore.getState().setBottomPanelHeight(32);
      expect(useUiStore.getState().panelLayout.bottomPanelHeight).toBe(32);

      useUiStore.getState().setBottomPanelHeight(31);
      expect(useUiStore.getState().panelLayout.bottomPanelHeight).toBe(0);
    });

    it('allows independent instances of the same panel type', () => {
      useUiStore.getState().addPanelInstance('connection_diagnostics', 'top-right');
      useUiStore.getState().addPanelInstance('connection_diagnostics', 'middle-right');

      const layout = useUiStore.getState().panelLayout;
      expect(layout.zones['top-right'].panelIds).toContain('connection_diagnostics');
      expect(layout.zones['middle-right'].panelIds).toContain('connection_diagnostics::2');
      expect(layout.hiddenPanelIds).not.toContain('connection_diagnostics');
    });

    it('removes only the requested instance', () => {
      useUiStore.getState().addPanelInstance('connection_diagnostics', 'top-right');
      useUiStore.getState().addPanelInstance('connection_diagnostics', 'middle-right');
      useUiStore.getState().removePanelInstance('connection_diagnostics::2');

      const layout = useUiStore.getState().panelLayout;
      expect(layout.zones['top-right'].panelIds).toContain('connection_diagnostics');
      expect(layout.zones['middle-right'].panelIds).not.toContain('connection_diagnostics::2');
    });

    it('opens the bottom dock when a panel is added there', () => {
      useUiStore.getState().setBottomPanelHeight(0);

      useUiStore.getState().addPanelInstance('connection_diagnostics', 'bottom');

      const layout = useUiStore.getState().panelLayout;
      expect(layout.zones.bottom.panelIds).toContain('connection_diagnostics');
      expect(layout.bottomPanelHeight).toBe(220);
    });

    it('opens the bottom dock when an existing tab is moved there', () => {
      useUiStore.getState().setBottomPanelHeight(0);

      useUiStore.getState().movePanelBetweenZones('properties', 'top-right', 'bottom');

      const layout = useUiStore.getState().panelLayout;
      expect(layout.zones.bottom.panelIds).toContain('properties');
      expect(layout.bottomPanelHeight).toBe(220);
    });
  });

  describe('floatPanel', () => {
    it('removes panel from zone and adds to floatingPanels', () => {
      const { panelLayout } = useUiStore.getState();
      expect(panelLayout.zones['top-right'].panelIds).toContain('properties');

      useUiStore.getState().floatPanel('properties', 100, 200, 420, 300);

      const state = useUiStore.getState();
      expect(state.panelLayout.zones['top-right'].panelIds).not.toContain('properties');
      expect(state.panelLayout.floatingPanels).toHaveLength(1);
      expect(state.panelLayout.floatingPanels[0].panelId).toBe('properties');
      expect(state.panelLayout.floatingPanels[0].x).toBe(100);
      expect(state.panelLayout.floatingPanels[0].y).toBe(200);
      expect(state.panelLayout.floatingPanels[0].width).toBe(420);
      expect(state.panelLayout.floatingPanels[0].height).toBe(300);
    });

    it('fixes activeTab if floating the active tab', () => {
      useUiStore.getState().setZoneActiveTab('top-right', 'properties');
      useUiStore.getState().floatPanel('properties', 100, 200, 420, 300);

      const state = useUiStore.getState();
      expect(state.panelLayout.zones['top-right'].activeTab).not.toBe('properties');
      expect(state.panelLayout.zones['top-right'].activeTab).toBe('cuts_layers');
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

      useUiStore.getState().dockPanel('console', 'middle-right');

      const state = useUiStore.getState();
      expect(state.panelLayout.floatingPanels).toHaveLength(0);
      expect(state.panelLayout.zones['middle-right'].panelIds).toContain('console');
      expect(state.panelLayout.zones['middle-right'].activeTab).toBe('console');
    });

    it('respects insertIndex', () => {
      useUiStore.getState().floatPanel('console', 100, 200, 420, 300);
      useUiStore.getState().dockPanel('console', 'middle-right', 0);

      const ids = useUiStore.getState().panelLayout.zones['middle-right'].panelIds;
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
      expect(useUiStore.getState().panelLayout.zones['top-right'].panelIds).toContain('properties');

      useUiStore.getState().movePanelBetweenZones('properties', 'top-right', 'middle-right');

      const state = useUiStore.getState();
      expect(state.panelLayout.zones['top-right'].panelIds).not.toContain('properties');
      expect(state.panelLayout.zones['middle-right'].panelIds).toContain('properties');
      expect(state.panelLayout.zones['middle-right'].activeTab).toBe('properties');
    });

    it('fixes source active tab', () => {
      useUiStore.getState().setZoneActiveTab('top-right', 'properties');
      useUiStore.getState().movePanelBetweenZones('properties', 'top-right', 'middle-right');

      expect(useUiStore.getState().panelLayout.zones['top-right'].activeTab).not.toBe('properties');
    });
  });

  describe('reorderPanelInZone', () => {
    it('changes order within zone', () => {
      const ids = useUiStore.getState().panelLayout.zones['top-right'].panelIds;
      expect(ids.indexOf('properties')).toBe(1);

      useUiStore.getState().reorderPanelInZone('properties', 'top-right', 0);

      const newIds = useUiStore.getState().panelLayout.zones['top-right'].panelIds;
      expect(newIds[0]).toBe('properties');
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
      expect(state.panelLayout.zones['top-right'].panelIds).toEqual(['cuts_layers', 'properties']);
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
