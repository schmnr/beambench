import { beforeEach, describe, expect, it, vi } from 'vitest';
import { PenTool } from '../PenTool';
import type { CanvasMouseEvent, ToolContext } from '../types';
import type { ViewportParams } from '../../ViewportTransform';

const defaultVp: ViewportParams = {
  offset: { x: 200, y: 200 },
  zoom: 100,
  canvasWidth: 800,
  canvasHeight: 600,
};

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
    transformLocks: { move_enabled: true, size_enabled: true, rotate_enabled: true, shear_enabled: true },
    snapEnabled: false,
    snapToObjects: false,
    gridSpacingMm: 10,
    selectObjects: vi.fn(),
    toggleObjectSelection: vi.fn(),
    addObject: vi.fn().mockResolvedValue(undefined),
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

describe('PenTool', () => {
  let tool: PenTool;

  beforeEach(() => {
    tool = new PenTool();
  });

  it('starts in idle state with no overlay', () => {
    expect(tool.getOverlay().type).toBe('none');
  });

  it('click places first point and enters drawing state', () => {
    const ctx = makeToolContext();

    tool.onMouseDown(makeMouseEvent({ snappedX: 10, snappedY: 20, screenX: 100, screenY: 200 }), ctx);
    tool.onMouseUp(makeMouseEvent({ screenX: 100, screenY: 200 }), ctx);

    const overlay = tool.getOverlay();
    expect(overlay.type).toBe('pen-preview');
    if (overlay.type === 'pen-preview') {
      expect(overlay.screenPoints).toHaveLength(1);
      expect(overlay.screenPoints[0].anchor).toEqual({ x: 100, y: 200 });
    }
  });

  it('click-and-drag creates symmetric handles', () => {
    const ctx = makeToolContext();

    // Click at origin
    tool.onMouseDown(makeMouseEvent({ snappedX: 0, snappedY: 0, screenX: 400, screenY: 300 }), ctx);

    // Drag right
    tool.onMouseMove(makeMouseEvent({ snappedX: 50, snappedY: 0, screenX: 500, screenY: 300 }), ctx);

    const overlay = tool.getOverlay();
    expect(overlay.type).toBe('pen-preview');
    if (overlay.type === 'pen-preview') {
      expect(overlay.dragging).toBe(true);
      const pt = overlay.points[0];
      // handleOut = cursor position
      expect(pt.handleOut).toEqual({ x: 50, y: 0 });
      // handleIn = mirror through anchor
      expect(pt.handleIn).toEqual({ x: -50, y: 0 });
    }

    // Release
    tool.onMouseUp(makeMouseEvent({ screenX: 500, screenY: 300 }), ctx);

    const overlay2 = tool.getOverlay();
    if (overlay2.type === 'pen-preview') {
      expect(overlay2.dragging).toBe(false);
      expect(overlay2.points[0].handleOut).toEqual({ x: 50, y: 0 });
    }
  });

  it('multiple points build path correctly', () => {
    const ctx = makeToolContext();

    // Point 1
    tool.onMouseDown(makeMouseEvent({ snappedX: 0, snappedY: 0, screenX: 100, screenY: 100 }), ctx);
    tool.onMouseUp(makeMouseEvent({ screenX: 100, screenY: 100 }), ctx);

    // Point 2
    tool.onMouseDown(makeMouseEvent({ snappedX: 50, snappedY: 50, screenX: 200, screenY: 200 }), ctx);
    tool.onMouseUp(makeMouseEvent({ screenX: 200, screenY: 200 }), ctx);

    // Point 3
    tool.onMouseDown(makeMouseEvent({ snappedX: 100, snappedY: 0, screenX: 300, screenY: 100 }), ctx);
    tool.onMouseUp(makeMouseEvent({ screenX: 300, screenY: 100 }), ctx);

    const overlay = tool.getOverlay();
    if (overlay.type === 'pen-preview') {
      expect(overlay.screenPoints).toHaveLength(3);
    }
  });

  it('double-click finalizes path with C commands', () => {
    const addObject = vi.fn().mockResolvedValue(undefined);
    const ctx = makeToolContext({ addObject });

    // Point 1 with handles
    tool.onMouseDown(makeMouseEvent({ snappedX: 0, snappedY: 0, screenX: 100, screenY: 100 }), ctx);
    tool.onMouseMove(makeMouseEvent({ snappedX: 10, snappedY: 0, screenX: 110, screenY: 100 }), ctx);
    tool.onMouseUp(makeMouseEvent({ screenX: 110, screenY: 100 }), ctx);

    // Point 2
    tool.onMouseDown(makeMouseEvent({ snappedX: 50, snappedY: 50, screenX: 200, screenY: 200 }), ctx);
    tool.onMouseUp(makeMouseEvent({ screenX: 200, screenY: 200 }), ctx);

    // Point 3
    tool.onMouseDown(makeMouseEvent({ snappedX: 100, snappedY: 0, screenX: 300, screenY: 100 }), ctx);
    tool.onMouseUp(makeMouseEvent({ screenX: 300, screenY: 100 }), ctx);

    // Double-click finalizes (the double-click mouseDown would add a 4th point, then onDoubleClick pops it)
    tool.onMouseDown(makeMouseEvent({ snappedX: 120, snappedY: 10, screenX: 320, screenY: 110 }), ctx);
    tool.onDoubleClick!(makeMouseEvent({ snappedX: 120, snappedY: 10, screenX: 320, screenY: 110 }), ctx);

    expect(addObject).toHaveBeenCalledTimes(1);
    const [name, layerId, data, bounds] = addObject.mock.calls[0];
    expect(name).toBe('Path');
    expect(layerId).toBe('layer1');
    expect(data.type).toBe('vector_path');
    expect(data.closed).toBe(false);
    expect(data.path_data).toContain('M ');
    expect(data.path_data).toContain('C ');
    expect(bounds.min).toBeDefined();
    expect(bounds.max).toBeDefined();
  });

  it('Escape finalizes partial path', () => {
    const addObject = vi.fn().mockResolvedValue(undefined);
    const ctx = makeToolContext({ addObject });

    // Two points
    tool.onMouseDown(makeMouseEvent({ snappedX: 0, snappedY: 0, screenX: 100, screenY: 100 }), ctx);
    tool.onMouseUp(makeMouseEvent({ screenX: 100, screenY: 100 }), ctx);
    tool.onMouseDown(makeMouseEvent({ snappedX: 50, snappedY: 50, screenX: 200, screenY: 200 }), ctx);
    tool.onMouseUp(makeMouseEvent({ screenX: 200, screenY: 200 }), ctx);

    tool.onKeyDown!(new KeyboardEvent('keydown', { key: 'Escape' }), ctx);

    expect(addObject).toHaveBeenCalledTimes(1);
    expect(tool.getOverlay().type).toBe('none');
  });

  it('Backspace removes last point', () => {
    const ctx = makeToolContext();

    // Three points
    tool.onMouseDown(makeMouseEvent({ snappedX: 0, snappedY: 0, screenX: 100, screenY: 100 }), ctx);
    tool.onMouseUp(makeMouseEvent({ screenX: 100, screenY: 100 }), ctx);
    tool.onMouseDown(makeMouseEvent({ snappedX: 50, snappedY: 50, screenX: 200, screenY: 200 }), ctx);
    tool.onMouseUp(makeMouseEvent({ screenX: 200, screenY: 200 }), ctx);
    tool.onMouseDown(makeMouseEvent({ snappedX: 100, snappedY: 0, screenX: 300, screenY: 100 }), ctx);
    tool.onMouseUp(makeMouseEvent({ screenX: 300, screenY: 100 }), ctx);

    const overlay1 = tool.getOverlay();
    if (overlay1.type === 'pen-preview') {
      expect(overlay1.screenPoints).toHaveLength(3);
    }

    tool.onKeyDown!(new KeyboardEvent('keydown', { key: 'Backspace' }), ctx);

    const overlay2 = tool.getOverlay();
    if (overlay2.type === 'pen-preview') {
      expect(overlay2.screenPoints).toHaveLength(2);
    }
  });

  it('Backspace with one point cancels', () => {
    const ctx = makeToolContext();

    tool.onMouseDown(makeMouseEvent({ snappedX: 0, snappedY: 0, screenX: 100, screenY: 100 }), ctx);
    tool.onMouseUp(makeMouseEvent({ screenX: 100, screenY: 100 }), ctx);

    tool.onKeyDown!(new KeyboardEvent('keydown', { key: 'Backspace' }), ctx);
    expect(tool.getOverlay().type).toBe('none');
  });

  it('click near first point closes path', () => {
    const addObject = vi.fn().mockResolvedValue(undefined);
    const ctx = makeToolContext({ addObject });

    // Point 1 at screen (100, 100)
    tool.onMouseDown(makeMouseEvent({ snappedX: 0, snappedY: 0, screenX: 100, screenY: 100 }), ctx);
    tool.onMouseUp(makeMouseEvent({ screenX: 100, screenY: 100 }), ctx);

    // Point 2
    tool.onMouseDown(makeMouseEvent({ snappedX: 50, snappedY: 50, screenX: 200, screenY: 200 }), ctx);
    tool.onMouseUp(makeMouseEvent({ screenX: 200, screenY: 200 }), ctx);

    // Point 3
    tool.onMouseDown(makeMouseEvent({ snappedX: 50, snappedY: 0, screenX: 200, screenY: 100 }), ctx);
    tool.onMouseUp(makeMouseEvent({ screenX: 200, screenY: 100 }), ctx);

    // Click near first point (within 8px)
    tool.onMouseDown(makeMouseEvent({ snappedX: 1, snappedY: 1, screenX: 103, screenY: 103 }), ctx);

    expect(addObject).toHaveBeenCalledTimes(1);
    const data = addObject.mock.calls[0][2];
    expect(data.closed).toBe(true);
    expect(data.path_data).toContain('Z');
  });

  it('less than 2 points on finalize = cancelled', () => {
    const addObject = vi.fn().mockResolvedValue(undefined);
    const ctx = makeToolContext({ addObject });

    // One point only
    tool.onMouseDown(makeMouseEvent({ snappedX: 0, snappedY: 0, screenX: 100, screenY: 100 }), ctx);
    tool.onMouseUp(makeMouseEvent({ screenX: 100, screenY: 100 }), ctx);

    tool.onKeyDown!(new KeyboardEvent('keydown', { key: 'Escape' }), ctx);

    expect(addObject).not.toHaveBeenCalled();
    expect(tool.getOverlay().type).toBe('none');
  });

  it('getOverlay returns pen-preview during drawing, none when idle', () => {
    const ctx = makeToolContext();

    expect(tool.getOverlay().type).toBe('none');

    tool.onMouseDown(makeMouseEvent({ snappedX: 0, snappedY: 0, screenX: 100, screenY: 100 }), ctx);
    tool.onMouseUp(makeMouseEvent({ screenX: 100, screenY: 100 }), ctx);

    expect(tool.getOverlay().type).toBe('pen-preview');

    tool.reset();
    expect(tool.getOverlay().type).toBe('none');
  });

  it('getCursor returns crosshair', () => {
    const ctx = makeToolContext();
    expect(tool.getCursor(ctx)).toBe('crosshair');
  });

  it('negligible drag does not create handles', () => {
    const ctx = makeToolContext();

    tool.onMouseDown(makeMouseEvent({ snappedX: 0, snappedY: 0, screenX: 400, screenY: 300 }), ctx);
    // Tiny drag — less than 0.5mm
    tool.onMouseMove(makeMouseEvent({ snappedX: 0.1, snappedY: 0.1, screenX: 401, screenY: 301 }), ctx);
    tool.onMouseUp(makeMouseEvent({ screenX: 401, screenY: 301 }), ctx);

    const overlay = tool.getOverlay();
    if (overlay.type === 'pen-preview') {
      expect(overlay.points[0].handleOut).toBeUndefined();
      expect(overlay.points[0].handleIn).toBeUndefined();
    }
  });
});
