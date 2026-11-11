import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { TabTool } from './TabTool';
import { useProjectStore } from '../../stores/projectStore';
import { vectorService } from '../../services/vectorService';
import { screenToWorldDist } from '../ViewportTransform';
import type { CanvasMouseEvent, ToolContext } from './types';
import type { ViewportParams } from '../ViewportTransform';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockReturnValue(new Promise(() => {})) }));

const defaultVp: ViewportParams = {
  offset: { x: 200, y: 200 },
  zoom: 100,
  canvasWidth: 800,
  canvasHeight: 600,
};

function makeMouseEvent(overrides: Partial<CanvasMouseEvent> = {}): CanvasMouseEvent {
  return {
    screenX: 0, screenY: 0,
    worldX: 50, worldY: 50,
    snappedX: 50, snappedY: 50,
    button: 0, shiftKey: false, ctrlKey: false, altKey: false,
    ...overrides,
  };
}

function makeToolContext(overrides: Partial<ToolContext> = {}): ToolContext {
  return {
    vp: defaultVp,
    objects: [],
    selectedObjectIds: [],
    selectedLayerId: 'layer1',
    layers: [{ id: 'layer1', enabled: true }],
    snapEnabled: false,
    snapToObjects: false,
    gridSpacingMm: 10,
    selectObjects: vi.fn(),
    toggleObjectSelection: vi.fn(),
    addObject: vi.fn(),
    updateObject: vi.fn(),
    rotateObjects: vi.fn(),
    shearObjects: vi.fn(),
    updateObjectBoundsBatch: vi.fn(),
    scaleAndRotateObjects: vi.fn().mockResolvedValue(undefined),
    setCursorWorldPos: vi.fn(),
    setStatusMessage: vi.fn(),
    requestRender: vi.fn(),
    ...overrides,
  };
}

describe('TabTool', () => {
  let tool: TabTool;

  beforeEach(() => {
    tool = new TabTool();
    vi.clearAllMocks();
  });

  afterEach(() => vi.restoreAllMocks());

  it.each([10, 20, 100, 800])('removes a marker on the first click at %s%% zoom without a prior hover event', async (zoom) => {
    const markers = [{ subpathIndex: 0, position: 0.5, worldX: 50, worldY: 0 }];
    vi.spyOn(vectorService, 'resolveTabMarkers').mockResolvedValue(markers);
    const place = vi.spyOn(useProjectStore.getState(), 'placeTab').mockResolvedValue(undefined);
    const remove = vi.spyOn(useProjectStore.getState(), 'removeTab').mockResolvedValue(undefined);
    await tool.refreshMarkers('obj1');
    tool.onMouseDown(makeMouseEvent({ worldX: 50 + screenToWorldDist(4, zoom), worldY: 0 }),
      makeToolContext({ selectedObjectIds: ['obj1'], vp: { ...defaultVp, zoom } }));
    await vi.waitFor(() => expect(remove).toHaveBeenCalledWith('obj1', 50, 0));
    expect(place).not.toHaveBeenCalled();
  });

  it.each(['reset', 'selection change'])('does not revive an older action after %s', async (change) => {
    let finishPlace!: () => void;
    vi.spyOn(useProjectStore.getState(), 'placeTab').mockImplementation(() => new Promise<void>(resolve => { finishPlace = resolve; }));
    const resolveMarkers = vi.spyOn(vectorService, 'resolveTabMarkers').mockResolvedValue([
      { subpathIndex: 0, position: 0.5, worldX: 10, worldY: 0 },
    ]);
    const ctx = makeToolContext({ selectedObjectIds: ['obj1'] });
    tool.onMouseDown(makeMouseEvent({ worldX: 50, worldY: 0 }), ctx);
    if (change === 'reset') tool.reset();
    else await tool.refreshMarkers('new-selection');
    finishPlace();
    await vi.waitFor(() => expect(ctx.requestRender).toHaveBeenCalled());
    expect(resolveMarkers.mock.calls.some(([id]) => id === 'obj1')).toBe(false);
    const overlay = tool.getOverlay();
    if (change === 'reset') expect(overlay.type).toBe('none');
    else expect(overlay).toMatchObject({ type: 'tab-markers', objectId: 'new-selection' });
  });

  it('ignores a stale marker response after a newer refresh', async () => {
    let finishOld!: (markers: { subpathIndex: number; position: number; worldX: number; worldY: number }[]) => void;
    vi.spyOn(vectorService, 'resolveTabMarkers')
      .mockImplementationOnce(() => new Promise(resolve => { finishOld = resolve; }))
      .mockResolvedValue([{ subpathIndex: 0, position: 0.5, worldX: 10, worldY: 0 }]);
    const old = tool.refreshMarkers('old-selection');
    await tool.refreshMarkers('new-selection');
    finishOld([{ subpathIndex: 0, position: 0.5, worldX: 50, worldY: 0 }]);
    await old;
    expect(tool.getOverlay()).toMatchObject({ type: 'tab-markers', objectId: 'new-selection' });
  });

  it('clears old marker positions while geometry is being refreshed', async () => {
    let finish!: (markers: []) => void;
    const resolve = vi.spyOn(vectorService, 'resolveTabMarkers').mockResolvedValue([
      { subpathIndex: 0, position: 0.5, worldX: 50, worldY: 0 },
    ]);
    await tool.refreshMarkers('obj1');
    resolve.mockImplementationOnce(() => new Promise(done => { finish = done; }));
    const pending = tool.refreshMarkers('obj1');
    expect(tool.getOverlay().type).toBe('none');
    finish([]);
    await pending;
  });

  it('waits for current markers before deciding whether a click adds or removes a tab', async () => {
    let finish!: (markers: { subpathIndex: number; position: number; worldX: number; worldY: number }[]) => void;
    vi.spyOn(vectorService, 'resolveTabMarkers').mockImplementationOnce(() => new Promise(resolve => { finish = resolve; }));
    const place = vi.spyOn(useProjectStore.getState(), 'placeTab').mockResolvedValue(undefined);
    const remove = vi.spyOn(useProjectStore.getState(), 'removeTab').mockResolvedValue(undefined);
    const pending = tool.refreshMarkers('obj1');
    const ctx = makeToolContext({ selectedObjectIds: ['obj1'] });
    const click = makeMouseEvent({ worldX: 50, worldY: 0 });
    tool.onMouseDown(click, ctx);
    expect(place).not.toHaveBeenCalled();
    expect(remove).not.toHaveBeenCalled();
    finish([{ subpathIndex: 0, position: 0.5, worldX: 50, worldY: 0 }]);
    await pending;
    tool.onMouseDown(click, ctx);
    await vi.waitFor(() => expect(remove).toHaveBeenCalledWith('obj1', 50, 0));
    expect(place).not.toHaveBeenCalled();
  });

  it.each([{ ids: [] }, { ids: ['first', 'second'] }])('does not edit tabs with an ambiguous selection $ids', ({ ids }) => {
    const place = vi.spyOn(useProjectStore.getState(), 'placeTab').mockResolvedValue(undefined);
    const remove = vi.spyOn(useProjectStore.getState(), 'removeTab').mockResolvedValue(undefined);
    tool.onMouseDown(makeMouseEvent(), makeToolContext({ selectedObjectIds: ids }));
    expect(place).not.toHaveBeenCalled();
    expect(remove).not.toHaveBeenCalled();
  });

  it('does not use cached markers from a previously selected object', async () => {
    const state = useProjectStore.getState();
    const place = vi.spyOn(state, 'placeTab').mockResolvedValue(undefined);
    const remove = vi.spyOn(state, 'removeTab').mockResolvedValue(undefined);
    // @ts-expect-error accessing private field
    tool.tabMarkers = { objectId: 'previous', markers: [{ subpathIndex: 0, position: 0.5, worldX: 50, worldY: 0 }] };
    // @ts-expect-error accessing private field
    tool.hoveredTabIndex = 0;
    tool.onMouseDown(makeMouseEvent({ worldX: 50, worldY: 0 }), makeToolContext({ selectedObjectIds: ['current'] }));
    await new Promise(r => setTimeout(r, 0));
    expect(remove).not.toHaveBeenCalled();
    expect(place).toHaveBeenCalledWith('current', 50, 0);
  });

  it('uses the click position after the pointer has left the previously hovered marker', async () => {
    const state = useProjectStore.getState();
    const place = vi.spyOn(state, 'placeTab').mockResolvedValue(undefined);
    const remove = vi.spyOn(state, 'removeTab').mockResolvedValue(undefined);
    // @ts-expect-error accessing private field
    tool.tabMarkers = { objectId: 'obj1', markers: [{ subpathIndex: 0, position: 0.5, worldX: 50, worldY: 0 }] };
    // @ts-expect-error accessing private field
    tool.hoveredTabIndex = 0;
    tool.onMouseDown(makeMouseEvent({ worldX: 0, worldY: 50 }), makeToolContext({ selectedObjectIds: ['obj1'] }));
    await new Promise(r => setTimeout(r, 0));
    expect(remove).not.toHaveBeenCalled();
    expect(place).toHaveBeenCalledWith('obj1', 0, 50);
  });

  it('removes the highlighted marker at its actual position when zoomed out', async () => {
    const remove = vi.spyOn(useProjectStore.getState(), 'removeTab').mockResolvedValue(undefined);
    // @ts-expect-error accessing private field
    tool.tabMarkers = { objectId: 'obj1', markers: [{ subpathIndex: 0, position: 0.5, worldX: 50, worldY: 0 }] };
    const ctx = makeToolContext({
      vp: { ...defaultVp, zoom: 20 }, selectedObjectIds: ['obj1'],
    });
    const click = makeMouseEvent({ worldX: 56, worldY: 0 });
    tool.onMouseMove(click, ctx);
    expect(tool.getCursor()).toBe('pointer');
    tool.onMouseDown(click, ctx);
    await new Promise(r => setTimeout(r, 0));
    // The highlighted marker is within the 8px hit target. Sending the raw
    // click instead is 6 mm off the path and the backend rejects removal.
    expect(remove).toHaveBeenCalledWith('obj1', 50, 0);
  });

  it('click with selected object calls place_tab', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    vi.mocked(invoke).mockResolvedValue({
      id: 'obj1', name: 'test', visible: true, locked: false,
      transform: { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 },
      bounds: { min: { x: 0, y: 0 }, max: { x: 100, y: 100 } },
      layer_id: 'layer1', z_index: 0,
      data: { type: 'vector_path', path_data: 'M 0 0 L 100 0', closed: true },
      tabs: [{ subpath_index: 0, position: 0.5 }],
    });

    const ctx = makeToolContext({ selectedObjectIds: ['obj1'] });
    tool.onMouseDown(makeMouseEvent({ worldX: 50, worldY: 0 }), ctx);

    // Wait for async operations
    await new Promise(r => setTimeout(r, 10));

    expect(invoke).toHaveBeenCalledWith('place_tab', {
      objectId: 'obj1',
      worldX: 50,
      worldY: 0,
    });
  });

  it('does nothing when no object is selected', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    vi.mocked(invoke).mockClear();

    const ctx = makeToolContext({ selectedObjectIds: [] });
    tool.onMouseDown(makeMouseEvent(), ctx);

    await new Promise(r => setTimeout(r, 10));
    expect(invoke).not.toHaveBeenCalled();
  });

  it('click near existing marker calls remove_tab', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    vi.mocked(invoke).mockResolvedValue({
      id: 'obj1', name: 'test', visible: true, locked: false,
      transform: { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 },
      bounds: { min: { x: 0, y: 0 }, max: { x: 100, y: 100 } },
      layer_id: 'layer1', z_index: 0,
      data: { type: 'vector_path', path_data: 'M 0 0 L 100 0', closed: true },
      tabs: [],
    });

    // Set up markers that simulate an existing tab at (50, 0)
    // @ts-expect-error accessing private field
    tool.tabMarkers = {
      objectId: 'obj1',
      markers: [{ subpathIndex: 0, position: 0.5, worldX: 50, worldY: 0 }],
    };
    // @ts-expect-error accessing private field
    tool.hoveredTabIndex = 0;

    const ctx = makeToolContext({ selectedObjectIds: ['obj1'] });
    tool.onMouseDown(makeMouseEvent({ worldX: 50, worldY: 0 }), ctx);

    await new Promise(r => setTimeout(r, 10));

    expect(invoke).toHaveBeenCalledWith('remove_tab', {
      objectId: 'obj1',
      worldX: 50,
      worldY: 0,
    });
  });

  it('getOverlay returns tab-markers when markers exist', () => {
    // @ts-expect-error accessing private field
    tool.tabMarkers = {
      objectId: 'obj1',
      markers: [
        { subpathIndex: 0, position: 0.25, worldX: 25, worldY: 0 },
        { subpathIndex: 0, position: 0.75, worldX: 75, worldY: 0 },
      ],
    };

    const overlay = tool.getOverlay();
    expect(overlay.type).toBe('tab-markers');
    if (overlay.type === 'tab-markers') {
      expect(overlay.markers).toHaveLength(2);
      expect(overlay.objectId).toBe('obj1');
    }
  });

  it('getOverlay returns none when no markers', () => {
    const overlay = tool.getOverlay();
    expect(overlay.type).toBe('none');
  });

  it('hover near marker changes cursor to pointer', () => {
    // @ts-expect-error accessing private field
    tool.tabMarkers = {
      objectId: 'obj1',
      markers: [{ subpathIndex: 0, position: 0.5, worldX: 50, worldY: 0 }],
    };

    const ctx = makeToolContext({ selectedObjectIds: ['obj1'] });
    // Move near the marker (within 3mm threshold at zoom 100)
    tool.onMouseMove(makeMouseEvent({ worldX: 50, worldY: 0.5 }), ctx);

    expect(tool.getCursor()).toBe('pointer');
  });

  it('hover away from marker keeps crosshair cursor', () => {
    // @ts-expect-error accessing private field
    tool.tabMarkers = {
      objectId: 'obj1',
      markers: [{ subpathIndex: 0, position: 0.5, worldX: 50, worldY: 0 }],
    };

    const ctx = makeToolContext({ selectedObjectIds: ['obj1'] });
    // Move far from the marker
    tool.onMouseMove(makeMouseEvent({ worldX: 0, worldY: 100 }), ctx);

    expect(tool.getCursor()).toBe('crosshair');
  });

  it('reset clears markers and hover state', () => {
    // @ts-expect-error accessing private field
    tool.tabMarkers = {
      objectId: 'obj1',
      markers: [{ subpathIndex: 0, position: 0.5, worldX: 50, worldY: 0 }],
    };
    // @ts-expect-error accessing private field
    tool.hoveredTabIndex = 0;

    tool.reset();

    // @ts-expect-error accessing private field
    expect(tool.tabMarkers).toBeNull();
    // @ts-expect-error accessing private field
    expect(tool.hoveredTabIndex).toBeNull();
    expect(tool.getOverlay().type).toBe('none');
  });
});
