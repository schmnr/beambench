import { beforeEach, describe, expect, it, vi } from 'vitest';
import { TabTool } from './TabTool';
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
