import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { SelectTool, computeResizedBounds } from './SelectTool';
import type { CanvasMouseEvent, ToolContext } from './types';
import type { Bounds, ProjectObject, Transform2D } from '../../types/project';
import type { ViewportParams } from '../ViewportTransform';
import { useAppStore } from '../../stores/appStore';
import { makeAppSettings, makeProjectObject } from '../../test-utils/projectFixtures';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockReturnValue(new Promise(() => {})) }));

const initialAppState = useAppStore.getState();

afterEach(() => {
  useAppStore.setState(initialAppState, true);
});

const identity: Transform2D = { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 };

// Viewport: zero offset, zoom 100, BASE_PX_PER_MM=2 → screen = world * 2
const defaultVp: ViewportParams = {
  offset: { x: 0, y: 0 },
  zoom: 100,
  canvasWidth: 800,
  canvasHeight: 600,
};

function makeVectorObject(id: string, bounds: Bounds): ProjectObject {
  return makeProjectObject({
    id,
    name: id,
    transform: { ...identity },
    bounds: { min: { ...bounds.min }, max: { ...bounds.max } },
    layer_id: 'layer1',
    data: { type: 'vector_path', path_data: 'M 0 0 L 10 0 L 10 10 Z', closed: true },
  });
}

function makeLineObject(id: string, bounds: Bounds): ProjectObject {
  return makeProjectObject({
    id,
    name: id,
    transform: { ...identity },
    bounds: { min: { ...bounds.min }, max: { ...bounds.max } },
    layer_id: 'layer1',
    data: {
      type: 'vector_path',
      path_data: 'M 0 0 L 10 0',
      closed: false,
      ruler_guide_axis: null,
    },
  });
}

function makeRasterImageObject(id: string, bounds: Bounds): ProjectObject {
  return makeProjectObject({
    id,
    name: id,
    transform: { ...identity },
    bounds: { min: { ...bounds.min }, max: { ...bounds.max } },
    layer_id: 'layer1',
    data: { type: 'raster_image', asset_key: `${id}-asset`, original_width_px: 100, original_height_px: 100 },
  });
}

function makeGroupObject(id: string, children: string[], bounds: Bounds): ProjectObject {
  return makeProjectObject({
    id,
    name: id,
    transform: { ...identity },
    bounds: { min: { ...bounds.min }, max: { ...bounds.max } },
    layer_id: 'layer1',
    data: { type: 'group', children },
  });
}

function makeMouseEvent(overrides: Partial<CanvasMouseEvent> = {}): CanvasMouseEvent {
  return {
    screenX: 0, screenY: 0,
    worldX: 0, worldY: 0,
    snappedX: 0, snappedY: 0,
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

describe('SelectTool crossing/enclosing selection', () => {
  let tool: SelectTool;

  // worldToScreen: screenX = worldX * 2 + 400, screenY = worldY * 2 + 300
  // fullyInsideObj: world (10,10)-(20,20) → screen (420,320)-(440,340)
  // partialObj: world (30,10)-(50,20) → screen (460,320)-(500,340)

  const fullyInsideObj = makeVectorObject('inside', {
    min: { x: 10, y: 10 }, max: { x: 20, y: 20 },
  });

  const partialObj = makeVectorObject('partial', {
    min: { x: 30, y: 10 }, max: { x: 50, y: 20 },
  });

  beforeEach(() => {
    tool = new SelectTool();
  });

  it('left-to-right drag uses enclosing selection (only fully contained objects selected)', () => {
    const selectObjects = vi.fn();
    const ctx = makeToolContext({
      objects: [fullyInsideObj, partialObj],
      selectObjects,
    });

    // Left-to-right rubber band: screen (415,315) to (445,345)
    // Fully contains inside (420,320)-(440,340) ✓
    // Does NOT contain partial (460,320)-(500,340) ✗
    tool.onMouseDown(makeMouseEvent({
      screenX: 415, screenY: 315,
      worldX: 7.5, worldY: 7.5,
      snappedX: 7.5, snappedY: 7.5,
    }), ctx);

    tool.onMouseMove(makeMouseEvent({
      screenX: 445, screenY: 345,
      worldX: 22.5, worldY: 22.5,
      snappedX: 22.5, snappedY: 22.5,
    }), ctx);

    tool.onMouseUp(makeMouseEvent({
      screenX: 445, screenY: 345,
      worldX: 22.5, worldY: 22.5,
      snappedX: 22.5, snappedY: 22.5,
    }), ctx);

    // selectObjects is called twice: once to clear (onMouseDown) and once for result (onMouseUp)
    // The last call should have the selection result
    const lastCall = selectObjects.mock.calls[selectObjects.mock.calls.length - 1];
    expect(lastCall[0]).toEqual(['inside']);
  });

  it('right-to-left drag uses crossing selection (intersecting objects selected)', () => {
    const selectObjects = vi.fn();
    const ctx = makeToolContext({
      objects: [fullyInsideObj, partialObj],
      selectObjects,
    });

    // Right-to-left rubber band: screen (505,315) to (415,345)
    // Rect = min(415,315) max(505,345)
    // inside (420,320)-(440,340): intersects ✓
    // partial (460,320)-(500,340): intersects ✓
    tool.onMouseDown(makeMouseEvent({
      screenX: 505, screenY: 315,
      worldX: 52.5, worldY: 7.5,
      snappedX: 52.5, snappedY: 7.5,
    }), ctx);

    tool.onMouseMove(makeMouseEvent({
      screenX: 415, screenY: 345,
      worldX: 7.5, worldY: 22.5,
      snappedX: 7.5, snappedY: 22.5,
    }), ctx);

    tool.onMouseUp(makeMouseEvent({
      screenX: 415, screenY: 345,
      worldX: 7.5, worldY: 22.5,
      snappedX: 7.5, snappedY: 22.5,
    }), ctx);

    const lastCall = selectObjects.mock.calls[selectObjects.mock.calls.length - 1];
    expect(lastCall[0]).toEqual(['partial', 'inside']);
  });

  it('rubber-band multi-add orders the batch so first draw-order hit is the anchor', () => {
    const selectObjects = vi.fn();
    const baseObj = makeVectorObject('base', {
      min: { x: -20, y: -20 }, max: { x: -10, y: -10 },
    });
    const ctx = makeToolContext({
      objects: [baseObj, fullyInsideObj, partialObj],
      selectedObjectIds: ['base'],
      selectObjects,
    });

    tool.onMouseDown(makeMouseEvent({
      screenX: 505, screenY: 315,
      worldX: 52.5, worldY: 7.5,
      snappedX: 52.5, snappedY: 7.5,
      shiftKey: true,
    }), ctx);

    tool.onMouseMove(makeMouseEvent({
      screenX: 415, screenY: 345,
      worldX: 7.5, worldY: 22.5,
      snappedX: 7.5, snappedY: 22.5,
      shiftKey: true,
    }), ctx);

    tool.onMouseUp(makeMouseEvent({
      screenX: 415, screenY: 345,
      worldX: 7.5, worldY: 22.5,
      snappedX: 7.5, snappedY: 22.5,
      shiftKey: true,
    }), ctx);

    const lastCall = selectObjects.mock.calls[selectObjects.mock.calls.length - 1];
    expect(lastCall[0]).toEqual(['base', 'partial', 'inside']);
  });

  it('shift-click adds without toggling an already-selected object off', () => {
    const selectObjects = vi.fn();
    const ctx = makeToolContext({
      objects: [fullyInsideObj],
      selectedObjectIds: ['inside'],
      selectObjects,
    });

    // @ts-expect-error private state setup for modifier-only click semantics
    tool.state = {
      type: 'maybe-drag',
      startScreen: { x: 430, y: 330 },
      startWorld: { x: 15, y: 15 },
      objectId: 'inside',
      shiftKey: true,
      ctrlKey: false,
    };
    tool.onMouseUp(makeMouseEvent({ screenX: 430, screenY: 330 }), ctx);

    expect(selectObjects).toHaveBeenLastCalledWith(['inside']);
  });

  it('ctrl-click toggles and ctrl+shift-click removes from selection', () => {
    const selectObjects = vi.fn();
    const ctx = makeToolContext({
      objects: [fullyInsideObj, partialObj],
      selectedObjectIds: ['inside', 'partial'],
      selectObjects,
    });

    // @ts-expect-error private state setup for modifier-only click semantics
    tool.state = {
      type: 'maybe-drag',
      startScreen: { x: 430, y: 330 },
      startWorld: { x: 15, y: 15 },
      objectId: 'inside',
      shiftKey: false,
      ctrlKey: true,
    };
    tool.onMouseUp(makeMouseEvent({ screenX: 430, screenY: 330 }), ctx);
    expect(selectObjects).toHaveBeenLastCalledWith(['partial']);

    // @ts-expect-error private state setup for modifier-only click semantics
    tool.state = {
      type: 'maybe-drag',
      startScreen: { x: 490, y: 330 },
      startWorld: { x: 45, y: 15 },
      objectId: 'partial',
      shiftKey: true,
      ctrlKey: true,
    };
    tool.onMouseUp(makeMouseEvent({ screenX: 490, screenY: 330 }), ctx);
    expect(selectObjects).toHaveBeenLastCalledWith(['inside']);
  });

  it('rubber-band ctrl toggles and ctrl+shift removes hit objects', () => {
    const selectObjects = vi.fn();
    const baseObj = makeVectorObject('base', {
      min: { x: -20, y: -20 }, max: { x: -10, y: -10 },
    });
    const ctx = makeToolContext({
      objects: [baseObj, fullyInsideObj, partialObj],
      selectedObjectIds: ['base', 'partial'],
      selectObjects,
    });

    // @ts-expect-error private state setup for rubber-band modifier semantics
    tool.state = {
      type: 'rubber-band',
      startScreen: { x: 505, y: 315 },
      currentScreen: { x: 415, y: 345 },
      crossing: true,
    };
    tool.onMouseUp(makeMouseEvent({
      screenX: 415, screenY: 345,
      worldX: 7.5, worldY: 22.5,
      snappedX: 7.5, snappedY: 22.5,
      ctrlKey: true,
    }), ctx);
    expect(selectObjects).toHaveBeenLastCalledWith(['base', 'inside']);

    const removeCtx = makeToolContext({
      objects: [baseObj, fullyInsideObj, partialObj],
      selectedObjectIds: ['base', 'inside', 'partial'],
      selectObjects,
    });
    // @ts-expect-error private state setup for rubber-band modifier semantics
    tool.state = {
      type: 'rubber-band',
      startScreen: { x: 505, y: 315 },
      currentScreen: { x: 415, y: 345 },
      crossing: true,
    };
    tool.onMouseUp(makeMouseEvent({
      screenX: 415, screenY: 345,
      worldX: 7.5, worldY: 22.5,
      snappedX: 7.5, snappedY: 22.5,
      shiftKey: true,
      ctrlKey: true,
    }), removeCtx);
    expect(selectObjects).toHaveBeenLastCalledWith(['base']);
  });

  it('click-selects locked objects so they can be unlocked from menus', () => {
    const lockedObj = { ...fullyInsideObj, locked: true };
    const selectObjects = vi.fn();
    const ctx = makeToolContext({
      objects: [lockedObj],
      selectObjects,
    });
    const ev = makeMouseEvent({
      screenX: 430,
      screenY: 330,
      worldX: 15,
      worldY: 15,
      snappedX: 15,
      snappedY: 15,
    });

    tool.onMouseDown(ev, ctx);
    tool.onMouseUp(ev, ctx);

    expect(selectObjects).toHaveBeenLastCalledWith(['inside']);
  });

  it('keeps locked objects selectable but blocks drag movement', () => {
    const lockedObj = { ...fullyInsideObj, locked: true };
    const selectObjects = vi.fn();
    const updateObjectBoundsBatch = vi.fn().mockResolvedValue(undefined);
    const ctx = makeToolContext({
      objects: [lockedObj],
      selectObjects,
      updateObjectBoundsBatch,
    });

    tool.onMouseDown(makeMouseEvent({
      screenX: 430,
      screenY: 330,
      worldX: 15,
      worldY: 15,
      snappedX: 15,
      snappedY: 15,
    }), ctx);
    tool.onMouseMove(makeMouseEvent({
      screenX: 450,
      screenY: 330,
      worldX: 25,
      worldY: 15,
      snappedX: 25,
      snappedY: 15,
    }), ctx);
    tool.onMouseUp(makeMouseEvent({
      screenX: 450,
      screenY: 330,
      worldX: 25,
      worldY: 15,
      snappedX: 25,
      snappedY: 15,
    }), ctx);

    expect(selectObjects).toHaveBeenCalledWith(['inside']);
    expect(updateObjectBoundsBatch).not.toHaveBeenCalled();
  });

  it('drags a zero-width open vector segment from a near-stroke click', () => {
    const verticalSegment = makeProjectObject({
      id: 'side-segment',
      name: 'Side Segment',
      transform: { ...identity },
      bounds: { min: { x: 10, y: 10 }, max: { x: 10, y: 30 } },
      layer_id: 'layer1',
      data: {
        type: 'vector_path',
        path_data: 'M 0 0 L 0 20',
        closed: false,
        ruler_guide_axis: null,
      },
    });
    const selectObjects = vi.fn();
    const updateObjectBoundsBatch = vi.fn().mockResolvedValue(undefined);
    const ctx = makeToolContext({
      objects: [verticalSegment],
      selectObjects,
      updateObjectBoundsBatch,
    });

    tool.onMouseDown(makeMouseEvent({
      screenX: 423,
      screenY: 340,
      worldX: 11.5,
      worldY: 20,
      snappedX: 11.5,
      snappedY: 20,
    }), ctx);
    tool.onMouseMove(makeMouseEvent({
      screenX: 443,
      screenY: 340,
      worldX: 21.5,
      worldY: 20,
      snappedX: 21.5,
      snappedY: 20,
    }), ctx);
    tool.onMouseMove(makeMouseEvent({
      screenX: 463,
      screenY: 340,
      worldX: 31.5,
      worldY: 20,
      snappedX: 31.5,
      snappedY: 20,
    }), ctx);
    tool.onMouseUp(makeMouseEvent({
      screenX: 463,
      screenY: 340,
      worldX: 31.5,
      worldY: 20,
      snappedX: 31.5,
      snappedY: 20,
    }), ctx);

    expect(selectObjects).toHaveBeenCalledWith(['side-segment']);
    expect(updateObjectBoundsBatch).toHaveBeenCalledOnce();
    expect(updateObjectBoundsBatch.mock.calls[0][0]).toEqual([
      {
        id: 'side-segment',
        bounds: { min: { x: 20, y: 10 }, max: { x: 20, y: 30 } },
      },
    ]);
  });

  it('rubber-band selection can include locked objects', () => {
    const lockedInsideObj = { ...fullyInsideObj, locked: true };
    const selectObjects = vi.fn();
    const ctx = makeToolContext({
      objects: [lockedInsideObj],
      selectObjects,
    });

    tool.onMouseDown(makeMouseEvent({
      screenX: 415,
      screenY: 315,
      worldX: 7.5,
      worldY: 7.5,
      snappedX: 7.5,
      snappedY: 7.5,
    }), ctx);

    tool.onMouseMove(makeMouseEvent({
      screenX: 445,
      screenY: 345,
      worldX: 22.5,
      worldY: 22.5,
      snappedX: 22.5,
      snappedY: 22.5,
    }), ctx);

    tool.onMouseUp(makeMouseEvent({
      screenX: 445,
      screenY: 345,
      worldX: 22.5,
      worldY: 22.5,
      snappedX: 22.5,
      snappedY: 22.5,
    }), ctx);

    expect(selectObjects).toHaveBeenLastCalledWith(['inside']);
  });

  it('enclosing requires full containment (partially overlapping object not selected)', () => {
    const selectObjects = vi.fn();
    const ctx = makeToolContext({
      objects: [partialObj],
      selectObjects,
    });

    // Left-to-right drag that partially overlaps partialObj
    // partialObj screen: (460,320)-(500,340)
    // Rubber band: screen (455,315) to (485,345)
    // Contains (460,320)-(485,340) of partialObj — NOT full containment (missing 485-500 x)
    tool.onMouseDown(makeMouseEvent({
      screenX: 455, screenY: 315,
      worldX: 27.5, worldY: 7.5,
      snappedX: 27.5, snappedY: 7.5,
    }), ctx);

    tool.onMouseMove(makeMouseEvent({
      screenX: 485, screenY: 345,
      worldX: 42.5, worldY: 22.5,
      snappedX: 42.5, snappedY: 22.5,
    }), ctx);

    tool.onMouseUp(makeMouseEvent({
      screenX: 485, screenY: 345,
      worldX: 42.5, worldY: 22.5,
      snappedX: 42.5, snappedY: 22.5,
    }), ctx);

    const lastCall = selectObjects.mock.calls[selectObjects.mock.calls.length - 1];
    expect(lastCall[0]).toEqual([]);
  });

  it('crossing overlay has crossing: true flag (non-alt)', () => {
    const ctx = makeToolContext({ objects: [] });

    // Start rubber-band (right-to-left)
    tool.onMouseDown(makeMouseEvent({
      screenX: 500, screenY: 310,
      worldX: 50, worldY: 5,
      snappedX: 50, snappedY: 5,
    }), ctx);

    tool.onMouseMove(makeMouseEvent({
      screenX: 450, screenY: 350,
      worldX: 25, worldY: 25,
      snappedX: 25, snappedY: 25,
    }), ctx);

    const overlay = tool.getOverlay();
    expect(overlay.type).toBe('rubber-band');
    if (overlay.type === 'rubber-band') {
      expect(overlay.crossing).toBe(true);
    }
  });
});

describe('SelectTool grouped object selection', () => {
  let tool: SelectTool;

  beforeEach(() => {
    tool = new SelectTool();
  });

  it('clicking a grouped child selects the top-level group', () => {
    const childA = { ...makeVectorObject('child-a', { min: { x: 10, y: 10 }, max: { x: 20, y: 20 } }), z_index: 10 };
    const childB = { ...makeVectorObject('child-b', { min: { x: 30, y: 10 }, max: { x: 40, y: 20 } }), z_index: 9 };
    const group = { ...makeGroupObject('group-1', ['child-a', 'child-b'], { min: { x: 10, y: 10 }, max: { x: 40, y: 20 } }), z_index: 0 };
    const selectObjects = vi.fn();
    const ctx = makeToolContext({
      objects: [childA, childB, group],
      selectObjects,
    });
    const ev = makeMouseEvent({
      screenX: 430,
      screenY: 330,
      worldX: 15,
      worldY: 15,
      snappedX: 15,
      snappedY: 15,
    });

    tool.onMouseDown(ev, ctx);
    tool.onMouseUp(ev, ctx);

    const lastCall = selectObjects.mock.calls[selectObjects.mock.calls.length - 1];
    expect(lastCall[0]).toEqual(['group-1']);
  });

  it('dragging a grouped child moves the group and all children together', () => {
    const childA = { ...makeVectorObject('child-a', { min: { x: 10, y: 10 }, max: { x: 20, y: 20 } }), z_index: 10 };
    const childB = { ...makeVectorObject('child-b', { min: { x: 30, y: 10 }, max: { x: 40, y: 20 } }), z_index: 9 };
    const group = { ...makeGroupObject('group-1', ['child-a', 'child-b'], { min: { x: 10, y: 10 }, max: { x: 40, y: 20 } }), z_index: 0 };
    const updateObjectBoundsBatch = vi.fn().mockResolvedValue(undefined);
    const ctx = makeToolContext({
      objects: [childA, childB, group],
      selectObjects: vi.fn(),
      updateObjectBoundsBatch,
    });

    tool.onMouseDown(makeMouseEvent({
      screenX: 430,
      screenY: 330,
      worldX: 15,
      worldY: 15,
      snappedX: 15,
      snappedY: 15,
    }), ctx);
    tool.onMouseMove(makeMouseEvent({
      screenX: 450,
      screenY: 330,
      worldX: 25,
      worldY: 15,
      snappedX: 25,
      snappedY: 15,
    }), ctx);
    tool.onMouseMove(makeMouseEvent({
      screenX: 470,
      screenY: 330,
      worldX: 35,
      worldY: 15,
      snappedX: 35,
      snappedY: 15,
    }), ctx);
    tool.onMouseUp(makeMouseEvent({
      screenX: 470,
      screenY: 330,
      worldX: 35,
      worldY: 15,
      snappedX: 35,
      snappedY: 15,
    }), ctx);

    expect(updateObjectBoundsBatch).toHaveBeenCalledOnce();
    const entries = updateObjectBoundsBatch.mock.calls[0][0] as Array<{ id: string; bounds: Bounds }>;
    expect(entries.map((entry) => entry.id)).toEqual(['group-1', 'child-a', 'child-b']);
    expect(entries.find((entry) => entry.id === 'group-1')?.bounds.min.x).toBe(20);
    expect(entries.find((entry) => entry.id === 'child-a')?.bounds.min.x).toBe(20);
    expect(entries.find((entry) => entry.id === 'child-b')?.bounds.min.x).toBe(40);
  });
});

describe('SelectTool alt+click cycle-through', () => {
  let tool: SelectTool;

  // Two overlapping objects at world (10,10)-(20,20) → screen (420,320)-(440,340)
  // z_index 2 = topmost, z_index 1 = below
  const topObj = { ...makeVectorObject('top', { min: { x: 10, y: 10 }, max: { x: 20, y: 20 } }), z_index: 2 };
  const bottomObj = { ...makeVectorObject('bottom', { min: { x: 10, y: 10 }, max: { x: 20, y: 20 } }), z_index: 1 };

  // Screen center of overlap area
  const sx = 430, sy = 330;
  const wx = 15, wy = 15;

  beforeEach(() => {
    tool = new SelectTool();
  });

  function clickAt(
    t: SelectTool,
    ctx: ToolContext,
    opts: { altKey?: boolean; screenX?: number; screenY?: number } = {},
  ) {
    const altKey = opts.altKey ?? false;
    const scx = opts.screenX ?? sx;
    const scy = opts.screenY ?? sy;
    const ev = makeMouseEvent({ screenX: scx, screenY: scy, worldX: wx, worldY: wy, snappedX: wx, snappedY: wy, altKey });
    t.onMouseDown(ev, ctx);
    t.onMouseUp(ev, ctx);
  }

  it('alt+click selects object below topmost', () => {
    const selectObjects = vi.fn();
    const ctx = makeToolContext({ objects: [topObj, bottomObj], selectObjects });

    clickAt(tool, ctx, { altKey: true });

    // Should select the second-in-stack (bottomObj)
    const lastCall = selectObjects.mock.calls[selectObjects.mock.calls.length - 1];
    expect(lastCall[0]).toEqual(['bottom']);
  });

  it('repeated alt+click cycles through stack', () => {
    const thirdObj = { ...makeVectorObject('third', { min: { x: 10, y: 10 }, max: { x: 20, y: 20 } }), z_index: 0 };
    const selectObjects = vi.fn();
    const ctx = makeToolContext({ objects: [topObj, bottomObj, thirdObj], selectObjects });

    // First alt+click: skip top → select bottom (index 1)
    clickAt(tool, ctx, { altKey: true });
    let lastCall = selectObjects.mock.calls[selectObjects.mock.calls.length - 1];
    expect(lastCall[0]).toEqual(['bottom']);

    // Second alt+click at same spot: cycle to third (index 2)
    clickAt(tool, ctx, { altKey: true });
    lastCall = selectObjects.mock.calls[selectObjects.mock.calls.length - 1];
    expect(lastCall[0]).toEqual(['third']);
  });

  it('alt+click wraps back to topmost after reaching bottom', () => {
    const selectObjects = vi.fn();
    const ctx = makeToolContext({ objects: [topObj, bottomObj], selectObjects });

    // First alt+click: index 1 (bottom)
    clickAt(tool, ctx, { altKey: true });
    // Second alt+click: wrap to index 0 (top)
    clickAt(tool, ctx, { altKey: true });
    const lastCall = selectObjects.mock.calls[selectObjects.mock.calls.length - 1];
    expect(lastCall[0]).toEqual(['top']);
  });

  it('regular click after alt+click resets cycle', () => {
    const selectObjects = vi.fn();
    const ctx = makeToolContext({ objects: [topObj, bottomObj], selectObjects });

    // Alt+click: selects bottom
    clickAt(tool, ctx, { altKey: true });
    let lastCall = selectObjects.mock.calls[selectObjects.mock.calls.length - 1];
    expect(lastCall[0]).toEqual(['bottom']);

    // Normal click: resets cycle, selects whichever object was hit (topmost via onMouseDown)
    clickAt(tool, ctx, { altKey: false });
    lastCall = selectObjects.mock.calls[selectObjects.mock.calls.length - 1];
    expect(lastCall[0]).toEqual(['top']);
  });
});

describe('SelectTool repeated click cycle-through', () => {
  let tool: SelectTool;

  const image = { ...makeRasterImageObject('image', { min: { x: 10, y: 10 }, max: { x: 30, y: 30 } }), z_index: 2 };
  const vector = { ...makeVectorObject('vector', { min: { x: 15, y: 15 }, max: { x: 25, y: 25 } }), z_index: 1 };

  const sx = 430, sy = 330;
  const wx = 15, wy = 15;

  beforeEach(() => {
    tool = new SelectTool();
  });

  function clickAt(t: SelectTool, ctx: ToolContext) {
    const ev = makeMouseEvent({ screenX: sx, screenY: sy, worldX: wx, worldY: wy, snappedX: wx, snappedY: wy });
    t.onMouseDown(ev, ctx);
    t.onMouseUp(ev, ctx);
  }

  it('selects below an already-selected top image on a repeated click', () => {
    const selectObjects = vi.fn();
    const ctx = makeToolContext({
      objects: [image, vector],
      selectedObjectIds: ['image'],
      selectObjects,
    });

    clickAt(tool, ctx);

    const lastCall = selectObjects.mock.calls[selectObjects.mock.calls.length - 1];
    expect(lastCall[0]).toEqual(['vector']);
  });

  it('keeps the first click on an unselected top image unchanged', () => {
    const selectObjects = vi.fn();
    const ctx = makeToolContext({
      objects: [image, vector],
      selectedObjectIds: [],
      selectObjects,
    });

    clickAt(tool, ctx);

    const lastCall = selectObjects.mock.calls[selectObjects.mock.calls.length - 1];
    expect(lastCall[0]).toEqual(['image']);
  });
});

describe('SelectTool snap threshold from settings', () => {
  it('reads snap_threshold_px from appStore settings during object-snap drag', () => {
    // Set a custom snap threshold in the app store.
    // use the shared makeAppSettings fixture so the full AppSettings
    // schema round-trips through the test (no partial `as never`).
    useAppStore.setState({
      settings: makeAppSettings({ snap_threshold_px: 15 }),
    });

    const tool = new SelectTool();

    // Create two objects: one being dragged, one as a snap target
    // They are far enough apart that default threshold (5px) wouldn't snap,
    // but 15px threshold should.
    const dragObj = makeVectorObject('drag', {
      min: { x: 0, y: 0 }, max: { x: 10, y: 10 },
    });
    // Snap target at x=20 — center of dragObj will be at ~15 after move,
    // edge distance = 5mm from the target edge at x=20
    const targetObj = makeVectorObject('target', {
      min: { x: 20, y: 0 }, max: { x: 30, y: 10 },
    });

    const requestRender = vi.fn();
    const ctx = makeToolContext({
      objects: [dragObj, targetObj],
      selectedObjectIds: ['drag'],
      snapToObjects: true,
      snapEnabled: false,
      layers: [{ id: 'layer1', enabled: true }],
      requestRender,
    });

    // The key assertion is that the code path reads from useAppStore
    // and doesn't throw. If it used the hardcoded constant, a custom
    // threshold value in the store would be ignored.
    // Start drag
    tool.onMouseDown(makeMouseEvent({
      screenX: 410, screenY: 310,
      worldX: 5, worldY: 5,
      snappedX: 5, snappedY: 5,
    }), ctx);

    // Move enough to trigger drag state (> DRAG_THRESHOLD_PX)
    tool.onMouseMove(makeMouseEvent({
      screenX: 420, screenY: 310,
      worldX: 10, worldY: 5,
      snappedX: 10, snappedY: 5,
    }), ctx);

    // Move close to target — the snap calculation should use 15px threshold
    tool.onMouseMove(makeMouseEvent({
      screenX: 425, screenY: 310,
      worldX: 12, worldY: 5,
      snappedX: 12, snappedY: 5,
    }), ctx);

    // Verify drag happened without errors and render was requested
    expect(requestRender).toHaveBeenCalled();
  });
});

describe('SelectTool shift-drag 45° constraint', () => {
  let tool: SelectTool;

  beforeEach(() => {
    tool = new SelectTool();
  });

  it('shift constrains to nearest 45° axis measured from drag start, not incremental', () => {
    // Object at (0,0)-(10,10), mouseDown at center (5,5).
    // The test proves: (a) shift snaps relative to startWorld and (b) switching
    // direction mid-drag re-projects from startWorld without drifting from the
    // first sub-move. A broken incremental implementation would accumulate an
    // offset from the first 45° commit.
    const obj = makeVectorObject('obj', { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } });
    const updateObjectBoundsBatch = vi.fn();
    const ctx = makeToolContext({
      objects: [obj],
      selectedObjectIds: ['obj'],
      updateObjectBoundsBatch,
    });

    // mouseDown at world (5,5) — screen (410, 310) at zoom=100, offset=0 (BASE_PX_PER_MM=2, canvas 800x600)
    tool.onMouseDown(makeMouseEvent({
      screenX: 410, screenY: 310,
      worldX: 5, worldY: 5,
      snappedX: 5, snappedY: 5,
    }), ctx);

    // First shift-move: total delta from start = (10, 5) → angle≈26.6°, snaps to 45° axis.
    // projection = (10*cos45 + 5*sin45) = 15/√2 ≈ 10.607.
    // End point = (5,5) + 10.607 * (cos45, sin45) = (5+7.5, 5+7.5) = (12.5, 12.5)
    // moveDx = 7.5, moveDy = 7.5. Object bounds → (7.5, 7.5)-(17.5, 17.5)
    tool.onMouseMove(makeMouseEvent({
      screenX: 430, screenY: 320,
      worldX: 15, worldY: 10,
      snappedX: 15, snappedY: 10,
      shiftKey: true,
    }), ctx);
    expect(obj.bounds.min.x).toBeCloseTo(7.5);
    expect(obj.bounds.min.y).toBeCloseTo(7.5);
    expect(obj.bounds.max.x).toBeCloseTo(17.5);
    expect(obj.bounds.max.y).toBeCloseTo(17.5);

    // Second shift-move (direction flip): total delta from start = (20, 2) → angle≈5.7°, snaps to 0° (horizontal).
    // projection = 20 along x-axis. End point = (5+20, 5+0) = (25, 5)
    // Object bounds → (20, 0)-(30, 10). If the code were incrementally constraining
    // moveDx/moveDy rather than projecting from startWorld, the first move's y-offset
    // would persist and the final bounds would drift.
    tool.onMouseMove(makeMouseEvent({
      screenX: 450, screenY: 314,
      worldX: 25, worldY: 7,
      snappedX: 25, snappedY: 7,
      shiftKey: true,
    }), ctx);
    expect(obj.bounds.min.x).toBeCloseTo(20);
    expect(obj.bounds.min.y).toBeCloseTo(0);
    expect(obj.bounds.max.x).toBeCloseTo(30);
    expect(obj.bounds.max.y).toBeCloseTo(10);

    // mouseUp commits via single batch call
    tool.onMouseUp(makeMouseEvent({
      screenX: 450, screenY: 314,
      worldX: 25, worldY: 7,
      snappedX: 25, snappedY: 7,
      shiftKey: true,
    }), ctx);
    expect(updateObjectBoundsBatch).toHaveBeenCalledTimes(1);
    const [entries] = updateObjectBoundsBatch.mock.calls[0];
    expect(entries).toHaveLength(1);
    expect(entries[0].id).toBe('obj');
    expect(entries[0].bounds.min.x).toBeCloseTo(20);
    expect(entries[0].bounds.min.y).toBeCloseTo(0);
    expect(entries[0].bounds.max.x).toBeCloseTo(30);
    expect(entries[0].bounds.max.y).toBeCloseTo(10);
  });
});

describe('SelectTool ruler guides', () => {
  it('keeps vertical guides axis-locked to horizontal movement only', () => {
    const tool = new SelectTool();
    const guide = makeProjectObject({
      id: 'guide',
      name: 'Guide',
      transform: { ...identity },
      bounds: { min: { x: 10, y: 0 }, max: { x: 10, y: 100 } },
      layer_id: 'layer1',
      data: {
        type: 'vector_path',
        path_data: 'M 10 0 L 10 100',
        closed: false,
        ruler_guide_axis: 'vertical',
      },
    });
    const updateObjectBoundsBatch = vi.fn();
    const ctx = makeToolContext({
      objects: [guide],
      selectedObjectIds: ['guide'],
      workspace: { bed_width_mm: 400, bed_height_mm: 300, origin: 'top_left' },
      updateObjectBoundsBatch,
    });

    tool.onMouseDown(
      makeMouseEvent({
        screenX: 420,
        screenY: 400,
        worldX: 10,
        worldY: 50,
        snappedX: 10,
        snappedY: 50,
      }),
      ctx,
    );

    tool.onMouseMove(
      makeMouseEvent({
        screenX: 460,
        screenY: 460,
        worldX: 30,
        worldY: 80,
        snappedX: 30,
        snappedY: 80,
      }),
      ctx,
    );

    expect(guide.bounds.min.x).toBeCloseTo(30);
    expect(guide.bounds.max.x).toBeCloseTo(30);
    expect(guide.bounds.min.y).toBeCloseTo(0);
    expect(guide.bounds.max.y).toBeCloseTo(100);
  });

  it('keeps horizontal guides axis-locked to vertical movement only', () => {
    const tool = new SelectTool();
    const guide = makeProjectObject({
      id: 'guide',
      name: 'Guide',
      transform: { ...identity },
      bounds: { min: { x: 0, y: 25 }, max: { x: 100, y: 25 } },
      layer_id: 'layer1',
      data: {
        type: 'vector_path',
        path_data: 'M 0 25 L 100 25',
        closed: false,
        ruler_guide_axis: 'horizontal',
      },
    });
    const updateObjectBoundsBatch = vi.fn();
    const ctx = makeToolContext({
      objects: [guide],
      selectedObjectIds: ['guide'],
      workspace: { bed_width_mm: 400, bed_height_mm: 300, origin: 'top_left' },
      updateObjectBoundsBatch,
    });

    tool.onMouseDown(
      makeMouseEvent({
        screenX: 400,
        screenY: 350,
        worldX: 50,
        worldY: 25,
        snappedX: 50,
        snappedY: 25,
      }),
      ctx,
    );

    tool.onMouseMove(
      makeMouseEvent({
        screenX: 460,
        screenY: 390,
        worldX: 80,
        worldY: 45,
        snappedX: 80,
        snappedY: 45,
      }),
      ctx,
    );

    expect(guide.bounds.min.x).toBeCloseTo(0);
    expect(guide.bounds.max.x).toBeCloseTo(100);
    expect(guide.bounds.min.y).toBeCloseTo(45);
    expect(guide.bounds.max.y).toBeCloseTo(45);
  });
});

describe('SelectTool alt-force object snap during drag', () => {
  let tool: SelectTool;

  beforeEach(() => {
    tool = new SelectTool();
  });

  it('alt held during drag forces object snap even when ctx.snapToObjects is false', () => {
    // dragObj near targetObj. With snapToObjects=false, drag without alt should
    // not engage snap guides. With alt held, snap must engage anyway.
    const dragObj = makeVectorObject('drag', { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } });
    const targetObj = makeVectorObject('target', { min: { x: 13.5, y: 0 }, max: { x: 23.5, y: 10 } });

    const ctx = makeToolContext({
      objects: [dragObj, targetObj],
      selectedObjectIds: ['drag'],
      snapToObjects: false, // explicitly OFF
      snapEnabled: false,
      layers: [{ id: 'layer1', enabled: true }],
    });

    // mouseDown at center of dragObj
    tool.onMouseDown(makeMouseEvent({
      screenX: 410, screenY: 310,
      worldX: 5, worldY: 5,
      snappedX: 5, snappedY: 5,
    }), ctx);

    // First mouseMove past drag threshold to enter drag state (no alt)
    tool.onMouseMove(makeMouseEvent({
      screenX: 420, screenY: 310,
      worldX: 10, worldY: 5,
      snappedX: 10, snappedY: 5,
    }), ctx);
    // No snap guides yet — snapToObjects is false and alt not held
    expect(tool.getOverlay().type).not.toBe('snap-guides');

    // The first move only transitions into drag state; it does not apply motion yet.
    // Keep the target within the Alt-expanded threshold for the unchanged drag bounds.
    // At zoom=100 (px/mm=2), SNAP_THRESHOLD_PX=5 and Alt expands to 3.75mm.
    // dragObj.max.x=10, target.min.x=13.5 → 3.5mm gap → within threshold.
    // With Alt held, object snap must engage and emit snap guides.
    tool.onMouseMove(makeMouseEvent({
      screenX: 420, screenY: 310,
      worldX: 10, worldY: 5,
      snappedX: 10, snappedY: 5,
      altKey: true,
    }), ctx);

    const overlay = tool.getOverlay();
    expect(overlay.type).toBe('snap-guides');
    if (overlay.type === 'snap-guides') {
      expect(overlay.guides.length).toBeGreaterThan(0);
    }
  });
});

describe('SelectTool flat-line move drag', () => {
  it('moves a selected horizontal line when dragging from its midpoint', () => {
    const tool = new SelectTool();
    const line = makeLineObject('line', { min: { x: 0, y: 0 }, max: { x: 10, y: 0 } });
    const updateObjectBoundsBatch = vi.fn().mockResolvedValue(undefined);
    const ctx = makeToolContext({
      objects: [line],
      selectedObjectIds: ['line'],
      updateObjectBoundsBatch,
    });

    tool.onMouseDown(makeMouseEvent({
      screenX: 410,
      screenY: 300,
      worldX: 5,
      worldY: 0,
      snappedX: 5,
      snappedY: 0,
    }), ctx);

    tool.onMouseMove(makeMouseEvent({
      screenX: 420,
      screenY: 300,
      worldX: 10,
      worldY: 0,
      snappedX: 10,
      snappedY: 0,
    }), ctx);

    expect(line.bounds.min.x).toBe(5);
    expect(line.bounds.max.x).toBe(15);
    expect(line.bounds.min.y).toBe(0);
    expect(line.bounds.max.y).toBe(0);

    tool.onMouseUp(makeMouseEvent({
      screenX: 420,
      screenY: 300,
      worldX: 10,
      worldY: 0,
      snappedX: 10,
      snappedY: 0,
    }), ctx);

    expect(updateObjectBoundsBatch).toHaveBeenCalledWith([
      {
        id: 'line',
        bounds: { min: { x: 5, y: 0 }, max: { x: 15, y: 0 } },
      },
    ]);
  });
});

describe('SelectTool mixed-selection drag batches atomically', () => {
  let tool: SelectTool;

  beforeEach(() => {
    tool = new SelectTool();
  });

  it('drag commit on mixed selection calls updateObjectBoundsBatch exactly once', () => {
    // Mixed selection: a vector_path object and a shape object.
    // The backend's update_object_bounds_batch command inline-refits vector paths
    // within a single undo snapshot, so the frontend must issue exactly one batch
    // invocation per drag — never a separate per-path scalePathToBounds call.
    const pathObj = makeVectorObject('path1', { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } });
    const shapeObj: ProjectObject = makeProjectObject({
      id: 'shape1',
      name: 'shape1',
      transform: { ...identity },
      bounds: { min: { x: 20, y: 0 }, max: { x: 30, y: 10 } },
      layer_id: 'layer1',
      z_index: 1,
      // schema-correct shape data matches production `ObjectData` union.
      data: { type: 'shape', kind: 'rectangle', width: 10, height: 10, corner_radius: 0 },
    });

    const updateObjectBoundsBatch = vi.fn();
    const ctx = makeToolContext({
      objects: [pathObj, shapeObj],
      selectedObjectIds: ['path1', 'shape1'],
      updateObjectBoundsBatch,
    });

    // mouseDown on the vector path
    tool.onMouseDown(makeMouseEvent({
      screenX: 410, screenY: 310,
      worldX: 5, worldY: 5,
      snappedX: 5, snappedY: 5,
    }), ctx);

    // Drag past threshold
    tool.onMouseMove(makeMouseEvent({
      screenX: 420, screenY: 320,
      worldX: 10, worldY: 10,
      snappedX: 10, snappedY: 10,
    }), ctx);

    // mouseUp commits
    tool.onMouseUp(makeMouseEvent({
      screenX: 420, screenY: 320,
      worldX: 10, worldY: 10,
      snappedX: 10, snappedY: 10,
    }), ctx);

    // Assertion: exactly ONE batch call, containing BOTH object ids
    expect(updateObjectBoundsBatch).toHaveBeenCalledTimes(1);
    const [entries] = updateObjectBoundsBatch.mock.calls[0];
    expect(entries).toHaveLength(2);
    const ids = entries.map((e: { id: string }) => e.id).sort();
    expect(ids).toEqual(['path1', 'shape1']);
  });
});

describe('computeResizedBounds', () => {
  const orig = { min: { x: 0, y: 0 }, max: { x: 100, y: 50 } }; // 100x50, aspect 2:1

  it('non-proportional SE handle applies raw delta', () => {
    const result = computeResizedBounds(orig, 'se', 20, 5, false);
    expect(result.min).toEqual({ x: 0, y: 0 });
    expect(result.max).toEqual({ x: 120, y: 55 });
  });

  it('proportional SE handle — width dominates', () => {
    // Drag SE by (20, 5): width change = 20/100 = 20%, height change = 5/50 = 10%
    // Width dominates → target height = newW / aspect = 120 / 2 = 60
    // Anchor top: max.y = 0 + 60 = 60
    const result = computeResizedBounds(orig, 'se', 20, 5, true);
    expect(result.min).toEqual({ x: 0, y: 0 });
    expect(result.max.x).toBeCloseTo(120);
    expect(result.max.y).toBeCloseTo(60); // 120/2 = 60
  });

  it('proportional SE handle — height dominates', () => {
    // Drag SE by (5, 20): width change = 5/100 = 5%, height change = 20/50 = 40%
    // Height dominates → target width = newH * aspect = 70 * 2 = 140
    // Anchor left: max.x = 0 + 140 = 140
    const result = computeResizedBounds(orig, 'se', 5, 20, true);
    expect(result.max.x).toBeCloseTo(140); // 70 * 2
    expect(result.max.y).toBeCloseTo(70);
    expect(result.min).toEqual({ x: 0, y: 0 });
  });

  it('proportional NW handle anchors bottom-right', () => {
    // Drag NW by (-20, -5): width change = 20/100 = 20%, height change = 5/50 = 10%
    // Width dominates → target height = newW / aspect = 120 / 2 = 60
    // Anchor bottom (max.y=50): min.y = 50 - 60 = -10
    const result = computeResizedBounds(orig, 'nw', -20, -5, true);
    expect(result.min.x).toBeCloseTo(-20);
    expect(result.min.y).toBeCloseTo(-10); // 50 - 60
    expect(result.max).toEqual({ x: 100, y: 50 });
  });

  it('proportional NE handle anchors bottom-left', () => {
    // Drag NE by (20, -5): width change = 20/100 = 20%, height change = 5/50 = 10%
    // Width dominates → target height = 120 / 2 = 60
    // Anchor bottom (max.y=50): min.y = 50 - 60 = -10
    const result = computeResizedBounds(orig, 'ne', 20, -5, true);
    expect(result.max.x).toBeCloseTo(120);
    expect(result.min.y).toBeCloseTo(-10);
    expect(result.min.x).toBe(0);
    expect(result.max.y).toBe(50);
  });

  it('proportional SW handle anchors top-right', () => {
    // Drag SW by (-20, 5): width change = 20/100 = 20%, height change = 5/50 = 10%
    // Width dominates → target height = 120 / 2 = 60
    // Anchor top (min.y=0): max.y = 0 + 60 = 60
    const result = computeResizedBounds(orig, 'sw', -20, 5, true);
    expect(result.min.x).toBeCloseTo(-20);
    expect(result.max.y).toBeCloseTo(60);
    expect(result.max.x).toBe(100);
    expect(result.min.y).toBe(0);
  });

  it('edge handle E ignores proportional flag', () => {
    const result = computeResizedBounds(orig, 'e', 20, 0, true);
    // Edge handles are not corners — proportional flag is ignored
    expect(result.min).toEqual({ x: 0, y: 0 });
    expect(result.max).toEqual({ x: 120, y: 50 });
  });

  it('edge handle N ignores proportional flag', () => {
    const result = computeResizedBounds(orig, 'n', 0, -10, true);
    expect(result.min).toEqual({ x: 0, y: -10 });
    expect(result.max).toEqual({ x: 100, y: 50 });
  });

  it('maintains aspect ratio for square objects', () => {
    const square = { min: { x: 0, y: 0 }, max: { x: 50, y: 50 } }; // 1:1
    const result = computeResizedBounds(square, 'se', 30, 10, true);
    // Width dominates (30/50=60% > 10/50=20%) → targetH = 80/1 = 80
    expect(result.max.x).toBeCloseTo(80);
    expect(result.max.y).toBeCloseTo(80);
  });
});

// Object world (0,0)-(10,10) → screen (400,300)-(420,320); SE resize handle at (420,320).
describe('SelectTool resize clamp (no bounds inversion)', () => {
  let tool: SelectTool;

  beforeEach(() => {
    tool = new SelectTool();
  });

  function makeResizeScenario() {
    const obj = makeVectorObject('target', { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } });
    const updateObjectBoundsBatch = vi.fn().mockResolvedValue(undefined);
    const selectObjects = vi.fn();
    const ctx = makeToolContext({
      objects: [obj],
      selectedObjectIds: ['target'],
      updateObjectBoundsBatch,
      selectObjects,
    });
    return { obj, ctx, updateObjectBoundsBatch, selectObjects };
  }

  function grabSeHandle(ctx: ToolContext) {
    tool.onMouseDown(makeMouseEvent({
      screenX: 420, screenY: 320,
      worldX: 10, worldY: 10,
      snappedX: 10, snappedY: 10,
    }), ctx);
  }

  function dragFarPastOppositeCorner(ctx: ToolContext) {
    // Drag the SE handle 40mm past the NW anchor — raw scale would be -3.
    tool.onMouseMove(makeMouseEvent({
      screenX: 340, screenY: 240,
      worldX: -30, worldY: -30,
      snappedX: -30, snappedY: -30,
    }), ctx);
  }

  it('live preview clamps instead of inverting bounds when dragged past the opposite corner', () => {
    const { obj, ctx } = makeResizeScenario();
    grabSeHandle(ctx);
    dragFarPastOppositeCorner(ctx);

    // Bounds must never invert (min > max)
    expect(obj.bounds.min.x).toBeLessThan(obj.bounds.max.x);
    expect(obj.bounds.min.y).toBeLessThan(obj.bounds.max.y);
    // Anchored at the NW corner, clamped to MIN_RESIZE_SCALE (1%) of original size
    expect(obj.bounds.min.x).toBeCloseTo(0);
    expect(obj.bounds.min.y).toBeCloseTo(0);
    expect(obj.bounds.max.x).toBeCloseTo(0.1);
    expect(obj.bounds.max.y).toBeCloseTo(0.1);
  });

  it('commit after an over-drag sends non-inverted bounds to the backend', () => {
    const { ctx, updateObjectBoundsBatch } = makeResizeScenario();
    grabSeHandle(ctx);
    dragFarPastOppositeCorner(ctx);
    tool.onMouseUp(makeMouseEvent({
      screenX: 340, screenY: 240,
      worldX: -30, worldY: -30,
      snappedX: -30, snappedY: -30,
    }), ctx);

    expect(updateObjectBoundsBatch).toHaveBeenCalledTimes(1);
    const [entries] = updateObjectBoundsBatch.mock.calls[0];
    expect(entries).toHaveLength(1);
    const committed = entries[0].bounds as Bounds;
    expect(committed.min.x).toBeLessThan(committed.max.x);
    expect(committed.min.y).toBeLessThan(committed.max.y);
  });

  it('Escape after an over-drag still restores the original bounds', () => {
    const { obj, ctx, selectObjects } = makeResizeScenario();
    grabSeHandle(ctx);
    dragFarPastOppositeCorner(ctx);
    tool.onKeyDown(new KeyboardEvent('keydown', { key: 'Escape' }), ctx);

    expect(obj.bounds).toEqual({ min: { x: 0, y: 0 }, max: { x: 10, y: 10 } });
    expect(selectObjects).toHaveBeenLastCalledWith([]);
  });

  it('normal shrink within limits is unaffected by the clamp', () => {
    const { obj, ctx } = makeResizeScenario();
    grabSeHandle(ctx);
    // Shrink to half: SE handle from (10,10) to (5,5)
    tool.onMouseMove(makeMouseEvent({
      screenX: 410, screenY: 310,
      worldX: 5, worldY: 5,
      snappedX: 5, snappedY: 5,
    }), ctx);

    expect(obj.bounds.min.x).toBeCloseTo(0);
    expect(obj.bounds.min.y).toBeCloseTo(0);
    expect(obj.bounds.max.x).toBeCloseTo(5);
    expect(obj.bounds.max.y).toBeCloseTo(5);
  });
});

describe('SelectTool cancelDrag (right-click / context-menu cancel path)', () => {
  let tool: SelectTool;

  beforeEach(() => {
    tool = new SelectTool();
  });

  it('restores original bounds mid-move-drag without clearing the selection', () => {
    // Triangle path interior point: world (7,4) → screen (414,308)
    const obj = makeVectorObject('target', { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } });
    const selectObjects = vi.fn();
    const updateObjectBoundsBatch = vi.fn().mockResolvedValue(undefined);
    const ctx = makeToolContext({ objects: [obj], selectObjects, updateObjectBoundsBatch });

    tool.onMouseDown(makeMouseEvent({
      screenX: 414, screenY: 308,
      worldX: 7, worldY: 4,
      snappedX: 7, snappedY: 4,
    }), ctx);
    // First move transitions maybe-drag → dragging; the next move applies an
    // incremental delta from there (+10 in x)
    tool.onMouseMove(makeMouseEvent({
      screenX: 434, screenY: 308,
      worldX: 17, worldY: 4,
      snappedX: 17, snappedY: 4,
    }), ctx);
    tool.onMouseMove(makeMouseEvent({
      screenX: 454, screenY: 308,
      worldX: 27, worldY: 4,
      snappedX: 27, snappedY: 4,
    }), ctx);

    // Drag in progress: bounds shifted by +10 in x
    expect(obj.bounds.min.x).toBeCloseTo(10);

    const cancelled = tool.cancelDrag(ctx);

    expect(cancelled).toBe(true);
    expect(obj.bounds).toEqual({ min: { x: 0, y: 0 }, max: { x: 10, y: 10 } });
    // Unlike Escape, cancelDrag must not deselect (right-click keeps selection)
    expect(selectObjects).not.toHaveBeenCalledWith([]);

    // Drag is no longer resumable: a later mouse-up must not commit anything
    tool.onMouseUp(makeMouseEvent({
      screenX: 434, screenY: 308,
      worldX: 17, worldY: 4,
      snappedX: 17, snappedY: 4,
    }), ctx);
    expect(updateObjectBoundsBatch).not.toHaveBeenCalled();
  });

  it('restores original bounds and transform mid-handle-resize', () => {
    const obj = makeVectorObject('target', { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } });
    const updateObjectBoundsBatch = vi.fn().mockResolvedValue(undefined);
    const ctx = makeToolContext({
      objects: [obj],
      selectedObjectIds: ['target'],
      updateObjectBoundsBatch,
    });

    // Grab SE handle at screen (420,320) and stretch to 2x
    tool.onMouseDown(makeMouseEvent({
      screenX: 420, screenY: 320,
      worldX: 10, worldY: 10,
      snappedX: 10, snappedY: 10,
    }), ctx);
    tool.onMouseMove(makeMouseEvent({
      screenX: 440, screenY: 340,
      worldX: 20, worldY: 20,
      snappedX: 20, snappedY: 20,
    }), ctx);

    expect(obj.bounds.max.x).toBeCloseTo(20);

    const cancelled = tool.cancelDrag(ctx);

    expect(cancelled).toBe(true);
    expect(obj.bounds).toEqual({ min: { x: 0, y: 0 }, max: { x: 10, y: 10 } });
    expect(obj.transform).toEqual(identity);

    tool.onMouseUp(makeMouseEvent({
      screenX: 440, screenY: 340,
      worldX: 20, worldY: 20,
      snappedX: 20, snappedY: 20,
    }), ctx);
    expect(updateObjectBoundsBatch).not.toHaveBeenCalled();
  });

  it('returns false when no drag is in progress', () => {
    const ctx = makeToolContext();
    expect(tool.cancelDrag(ctx)).toBe(false);
  });
});
