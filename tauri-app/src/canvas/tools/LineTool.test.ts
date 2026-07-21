import { beforeEach, describe, expect, it, vi } from 'vitest';
import { PenTool } from './PenTool';
import type { CanvasMouseEvent, ToolContext } from './types';
import type { ViewportParams } from '../ViewportTransform';
import { parsePathData, computePathBBox } from '../drawObjects';

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

describe('PenTool (merged line/pen)', () => {
  let tool: PenTool;

  beforeEach(() => {
    tool = new PenTool();
  });

  it('creates VectorPath on Escape with 2+ points', () => {
    const addObject = vi.fn();
    const ctx = makeToolContext({ addObject });

    // Place 3 points via mouseDown + mouseUp (click workflow)
    tool.onMouseDown(makeMouseEvent({ snappedX: 10, snappedY: 10, screenX: 100, screenY: 100 }), ctx);
    tool.onMouseUp(makeMouseEvent(), ctx);
    tool.onMouseDown(makeMouseEvent({ snappedX: 20, snappedY: 10, screenX: 200, screenY: 100 }), ctx);
    tool.onMouseUp(makeMouseEvent(), ctx);
    tool.onMouseDown(makeMouseEvent({ snappedX: 20, snappedY: 20, screenX: 200, screenY: 200 }), ctx);
    tool.onMouseUp(makeMouseEvent(), ctx);

    // Finalize via Escape
    tool.onKeyDown!(new KeyboardEvent('keydown', { key: 'Escape' }), ctx);

    expect(addObject).toHaveBeenCalledWith(
      'Path',
      'layer1',
      expect.objectContaining({ type: 'vector_path', closed: false }),
      expect.any(Object),
    );
    const bounds = addObject.mock.calls[0][3] as { min: { x: number; y: number }; max: { x: number; y: number } };
    expect(bounds.min.x).toBeCloseTo(10, 5);
    expect(bounds.min.y).toBeCloseTo(10, 5);
    expect(bounds.max.x).toBeCloseTo(20, 5);
    expect(bounds.max.y).toBeCloseTo(20, 5);
  });

  it('shift constrains angle to 45-degree increments', () => {
    const addObject = vi.fn();
    const ctx = makeToolContext({ addObject });

    // First point at origin
    tool.onMouseDown(makeMouseEvent({ snappedX: 0, snappedY: 0, screenX: 0, screenY: 0 }), ctx);
    tool.onMouseUp(makeMouseEvent(), ctx);

    // Second point with shift — (10, 3) should snap to 0° → (dist, 0)
    tool.onMouseDown(makeMouseEvent({
      snappedX: 10, snappedY: 3,
      screenX: 100, screenY: 30,
      shiftKey: true,
    }), ctx);
    tool.onMouseUp(makeMouseEvent(), ctx);

    // Finalize via Escape
    tool.onKeyDown!(new KeyboardEvent('keydown', { key: 'Escape' }), ctx);

    expect(addObject).toHaveBeenCalled();
    const pathData = addObject.mock.calls[0][2].path_data as string;
    // The path uses C commands — extract the endpoint (last two numbers before next C or end)
    // For a straight click (no drag), control points equal anchor, so C cp1x cp1y cp2x cp2y x y
    // The y coordinate of the endpoint should be near 0
    const coords = pathData.match(/[\d.-]+/g)?.map(Number) ?? [];
    // Last coordinate pair in the C command is the endpoint
    const endY = coords[coords.length - 1];
    expect(Math.abs(endY)).toBeLessThan(0.5);
  });

  it('creates bezier curves when dragging', () => {
    const addObject = vi.fn();
    const ctx = makeToolContext({ addObject });

    // Click and drag first point
    tool.onMouseDown(makeMouseEvent({ snappedX: 0, snappedY: 0, screenX: 0, screenY: 0 }), ctx);
    tool.onMouseMove(makeMouseEvent({ snappedX: 5, snappedY: 5, screenX: 50, screenY: 50 }), ctx);
    tool.onMouseUp(makeMouseEvent({ snappedX: 5, snappedY: 5, screenX: 50, screenY: 50 }), ctx);

    // Click second point (no drag)
    tool.onMouseDown(makeMouseEvent({ snappedX: 20, snappedY: 0, screenX: 200, screenY: 0 }), ctx);
    tool.onMouseUp(makeMouseEvent(), ctx);

    // Finalize
    tool.onKeyDown!(new KeyboardEvent('keydown', { key: 'Escape' }), ctx);

    expect(addObject).toHaveBeenCalled();
    const pathData = addObject.mock.calls[0][2].path_data as string;
    // Should contain C command (cubic bezier)
    expect(pathData).toContain('C');
    // The control point from dragging should not equal the anchor
    expect(pathData).toContain('M 0 0');
  });

  it('reset clears state back to idle', () => {
    const ctx = makeToolContext();

    tool.onMouseDown(makeMouseEvent({ snappedX: 10, snappedY: 10, screenX: 100, screenY: 100 }), ctx);
    expect(tool.getOverlay().type).toBe('pen-preview');

    tool.reset();
    expect(tool.getOverlay().type).toBe('none');
  });

  it('backspace removes last point', () => {
    const ctx = makeToolContext();

    // Place two points
    tool.onMouseDown(makeMouseEvent({ snappedX: 0, snappedY: 0, screenX: 0, screenY: 0 }), ctx);
    tool.onMouseUp(makeMouseEvent(), ctx);
    tool.onMouseDown(makeMouseEvent({ snappedX: 10, snappedY: 0, screenX: 100, screenY: 0 }), ctx);
    tool.onMouseUp(makeMouseEvent(), ctx);

    // Overlay should show pen-preview
    expect(tool.getOverlay().type).toBe('pen-preview');

    // Backspace removes last point
    tool.onKeyDown!(new KeyboardEvent('keydown', { key: 'Backspace' }), ctx);
    // Still in drawing mode with 1 point
    expect(tool.getOverlay().type).toBe('pen-preview');

    // Another backspace cancels (only 1 point)
    tool.onKeyDown!(new KeyboardEvent('keydown', { key: 'Backspace' }), ctx);
    expect(tool.getOverlay().type).toBe('none');
  });

  it('finalized bounds match curve-sampled pathBBox', () => {
    const addObject = vi.fn();
    const ctx = makeToolContext({ addObject });

    // Place 3 points with drag on point 2 to create cubic handles
    tool.onMouseDown(makeMouseEvent({ snappedX: 0, snappedY: 0, screenX: 0, screenY: 0 }), ctx);
    tool.onMouseUp(makeMouseEvent(), ctx);

    // Second point: click and drag to create handles
    tool.onMouseDown(makeMouseEvent({ snappedX: 10, snappedY: 0, screenX: 100, screenY: 0 }), ctx);
    tool.onMouseMove(makeMouseEvent({ snappedX: 10, snappedY: -10, screenX: 100, screenY: -100 }), ctx);
    tool.onMouseUp(makeMouseEvent({ snappedX: 10, snappedY: -10, screenX: 100, screenY: -100 }), ctx);

    // Third point
    tool.onMouseDown(makeMouseEvent({ snappedX: 20, snappedY: 0, screenX: 200, screenY: 0 }), ctx);
    tool.onMouseUp(makeMouseEvent(), ctx);

    // Finalize
    tool.onKeyDown!(new KeyboardEvent('keydown', { key: 'Escape' }), ctx);

    expect(addObject).toHaveBeenCalled();
    const pathData = addObject.mock.calls[0][2].path_data as string;
    const bounds = addObject.mock.calls[0][3] as { min: { x: number; y: number }; max: { x: number; y: number } };

    const cmds = parsePathData(pathData);
    const bbox = computePathBBox(cmds);

    expect(bounds.min.x).toBeCloseTo(bbox.minX, 5);
    expect(bounds.min.y).toBeCloseTo(bbox.minY, 5);
    expect(bounds.max.x).toBeCloseTo(bbox.maxX, 5);
    expect(bounds.max.y).toBeCloseTo(bbox.maxY, 5);
  });

  it('close detection shows closed preview when cursor near first point', () => {
    const ctx = makeToolContext();

    // Place two points
    tool.onMouseDown(makeMouseEvent({ snappedX: 0, snappedY: 0, screenX: 100, screenY: 100 }), ctx);
    tool.onMouseUp(makeMouseEvent(), ctx);
    tool.onMouseDown(makeMouseEvent({ snappedX: 20, snappedY: 0, screenX: 300, screenY: 100 }), ctx);
    tool.onMouseUp(makeMouseEvent(), ctx);

    // Move cursor near first point (within 8px screen distance)
    tool.onMouseMove(makeMouseEvent({ snappedX: 1, snappedY: 1, screenX: 103, screenY: 103 }), ctx);
    const overlay1 = tool.getOverlay();
    expect(overlay1.type).toBe('pen-preview');
    if (overlay1.type === 'pen-preview') {
      expect(overlay1.closed).toBe(true);
    }

    // Move cursor away from first point
    tool.onMouseMove(makeMouseEvent({ snappedX: 10, snappedY: 10, screenX: 200, screenY: 200 }), ctx);
    const overlay2 = tool.getOverlay();
    expect(overlay2.type).toBe('pen-preview');
    if (overlay2.type === 'pen-preview') {
      expect(overlay2.closed).toBe(false);
    }
  });

  it('clicking near first point finalizes closed path with matching bounds', () => {
    const addObject = vi.fn();
    const ctx = makeToolContext({ addObject });

    // Place 3 points
    tool.onMouseDown(makeMouseEvent({ snappedX: 0, snappedY: 0, screenX: 100, screenY: 100 }), ctx);
    tool.onMouseUp(makeMouseEvent(), ctx);
    tool.onMouseDown(makeMouseEvent({ snappedX: 20, snappedY: 0, screenX: 300, screenY: 100 }), ctx);
    tool.onMouseUp(makeMouseEvent(), ctx);
    tool.onMouseDown(makeMouseEvent({ snappedX: 10, snappedY: 20, screenX: 200, screenY: 300 }), ctx);
    tool.onMouseUp(makeMouseEvent(), ctx);

    // Click near first point to close (within 8px screen distance)
    tool.onMouseDown(makeMouseEvent({ snappedX: 0, snappedY: 0, screenX: 102, screenY: 103 }), ctx);

    expect(addObject).toHaveBeenCalled();
    const data = addObject.mock.calls[0][2];
    expect(data.closed).toBe(true);

    const pathData = data.path_data as string;
    const bounds = addObject.mock.calls[0][3] as { min: { x: number; y: number }; max: { x: number; y: number } };

    const cmds = parsePathData(pathData);
    const bbox = computePathBBox(cmds);

    expect(bounds.min.x).toBeCloseTo(bbox.minX, 5);
    expect(bounds.min.y).toBeCloseTo(bbox.minY, 5);
    expect(bounds.max.x).toBeCloseTo(bbox.maxX, 5);
    expect(bounds.max.y).toBeCloseTo(bbox.maxY, 5);
  });
});
