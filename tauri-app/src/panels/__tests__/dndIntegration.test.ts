import { describe, it, expect, beforeEach, vi } from 'vitest';
import { useUiStore } from '../../stores/uiStore';
import { createDefaultLayout, PANEL_REGISTRY } from '../../panels';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(null) }));

describe('DnD Integration — full workflows through store actions', () => {
  beforeEach(() => {
    useUiStore.setState({
      panelLayout: createDefaultLayout(),
      nextFloatingZIndex: 1,
      cameraWindowOpen: false,
    });
  });

  it('default layout has 11 panels across 4 zones and none floating', () => {
    const { panelLayout } = useUiStore.getState();
    const upperIds = panelLayout.zones['upper-right'].panelIds;
    const lowerIds = panelLayout.zones['lower-right'].panelIds;
    const leftIds = panelLayout.zones['left'].panelIds;
    const bottomIds = panelLayout.zones['bottom'].panelIds;
    expect(upperIds).toHaveLength(5);
    expect(lowerIds).toHaveLength(2);
    expect(leftIds).toHaveLength(0);
    expect(bottomIds).toHaveLength(0);
    expect(panelLayout.floatingPanels).toHaveLength(0);
    // Camera is defaultVisible: false, so it's in hiddenPanelIds
    expect(panelLayout.hiddenPanelIds).toContain('camera');
    // Total non-hidden docked panels = 7 (color palette retired for layer tabs)
    const allDocked = [...upperIds, ...lowerIds, ...leftIds, ...bottomIds];
    expect(allDocked).toHaveLength(7);
  });

  it('float a panel → leaves zone, appears in floatingPanels', () => {
    useUiStore.getState().floatPanel('console', 100, 200, 420, 300);

    const { panelLayout } = useUiStore.getState();
    expect(panelLayout.zones['upper-right'].panelIds).not.toContain('console');
    expect(panelLayout.floatingPanels).toHaveLength(1);
    expect(panelLayout.floatingPanels[0].panelId).toBe('console');
  });

  it('dock it back → leaves floatingPanels, appears in zone', () => {
    useUiStore.getState().floatPanel('console', 100, 200, 420, 300);
    useUiStore.getState().dockPanel('console', 'upper-right');

    const { panelLayout } = useUiStore.getState();
    expect(panelLayout.floatingPanels).toHaveLength(0);
    expect(panelLayout.zones['upper-right'].panelIds).toContain('console');
    expect(panelLayout.zones['upper-right'].activeTab).toBe('console');
  });

  it('move between zones (upper→lower) → correct zone membership', () => {
    useUiStore.getState().movePanelBetweenZones('console', 'upper-right', 'lower-right');

    const { panelLayout } = useUiStore.getState();
    expect(panelLayout.zones['upper-right'].panelIds).not.toContain('console');
    expect(panelLayout.zones['lower-right'].panelIds).toContain('console');
    expect(panelLayout.zones['lower-right'].activeTab).toBe('console');
  });

  it('reorder tabs within zone → order changes', () => {
    const before = useUiStore.getState().panelLayout.zones['upper-right'].panelIds;
    expect(before[0]).toBe('cuts_layers');
    expect(before[2]).toBe('console');

    useUiStore.getState().reorderPanelInZone('console', 'upper-right', 0);

    const after = useUiStore.getState().panelLayout.zones['upper-right'].panelIds;
    expect(after[0]).toBe('console');
  });

  it('close a floating panel → hidden but float entry preserved', () => {
    useUiStore.getState().floatPanel('console', 0, 0, 420, 300);
    useUiStore.getState().closeFloatingPanel('console');

    const { panelLayout } = useUiStore.getState();
    // Entry preserved so reopening restores position/size
    expect(panelLayout.floatingPanels).toHaveLength(1);
    expect(panelLayout.hiddenPanelIds).toContain('console');
  });

  it('re-show hidden panel that defaults to floating → re-floats', () => {
    // Camera defaults to floating and starts hidden
    const { panelLayout: before } = useUiStore.getState();
    expect(before.hiddenPanelIds).toContain('camera');

    useUiStore.getState().togglePanelVisibility('camera');

    const { panelLayout: after } = useUiStore.getState();
    expect(after.hiddenPanelIds).not.toContain('camera');
    expect(after.floatingPanels.some((fp) => fp.panelId === 'camera')).toBe(true);
  });

  it('reset layout → everything returns to defaults', () => {
    useUiStore.getState().floatPanel('console', 0, 0, 420, 300);
    useUiStore.getState().floatPanel('macros', 50, 50, 320, 260);
    expect(useUiStore.getState().panelLayout.floatingPanels).toHaveLength(2);

    useUiStore.getState().resetLayout();

    const { panelLayout, nextFloatingZIndex } = useUiStore.getState();
    expect(panelLayout.floatingPanels).toHaveLength(0);
    expect(nextFloatingZIndex).toBe(1);
    expect(panelLayout.zones['upper-right'].panelIds).toContain('console');
    expect(panelLayout.zones['upper-right'].panelIds).toContain('macros');
  });

  it('two floating panels → bringToFront gives correct z-order', () => {
    useUiStore.getState().floatPanel('console', 0, 0, 420, 300);
    useUiStore.getState().floatPanel('macros', 50, 50, 320, 260);

    const consoleZ1 = useUiStore.getState().panelLayout.floatingPanels.find((f) => f.panelId === 'console')!.zIndex;
    const macrosZ1 = useUiStore.getState().panelLayout.floatingPanels.find((f) => f.panelId === 'macros')!.zIndex;
    expect(macrosZ1).toBeGreaterThan(consoleZ1);

    useUiStore.getState().bringToFront('console');

    const consoleZ2 = useUiStore.getState().panelLayout.floatingPanels.find((f) => f.panelId === 'console')!.zIndex;
    expect(consoleZ2).toBeGreaterThan(macrosZ1);
  });

  it('camera panel opens as floating via toggleCameraWindow, closes correctly', () => {
    useUiStore.getState().toggleCameraWindow();

    const { panelLayout: after } = useUiStore.getState();
    expect(after.floatingPanels.some((fp) => fp.panelId === 'camera')).toBe(true);
    expect(after.hiddenPanelIds).not.toContain('camera');

    useUiStore.getState().toggleCameraWindow();

    const { panelLayout: final } = useUiStore.getState();
    // Floating entry preserved (position/size retained) but panel is hidden
    expect(final.floatingPanels.some((fp) => fp.panelId === 'camera')).toBe(true);
    expect(final.hiddenPanelIds).toContain('camera');
  });

  it('registry has 11 panels total', () => {
    expect(PANEL_REGISTRY).toHaveLength(11);
  });

  it('float → dock at specific index → appears at that position', () => {
    useUiStore.getState().floatPanel('properties', 0, 0, 384, 400);
    useUiStore.getState().dockPanel('properties', 'lower-right', 0);

    const ids = useUiStore.getState().panelLayout.zones['lower-right'].panelIds;
    expect(ids[0]).toBe('properties');
  });

  it('floating panel survives move + resize', () => {
    useUiStore.getState().floatPanel('console', 100, 200, 420, 300);
    useUiStore.getState().moveFloatingPanel('console', 500, 600);
    useUiStore.getState().resizeFloatingPanel('console', 500, 400);

    const fp = useUiStore.getState().panelLayout.floatingPanels.find((f) => f.panelId === 'console')!;
    expect(fp.x).toBe(500);
    expect(fp.y).toBe(600);
    expect(fp.width).toBe(500);
    expect(fp.height).toBe(400);
  });
});
