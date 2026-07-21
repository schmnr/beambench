import { describe, expect, it, vi } from 'vitest';
import { RectTool } from './RectTool';
import type { CanvasMouseEvent, ToolContext } from './types';
import type { ViewportParams } from '../ViewportTransform';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockReturnValue(new Promise(() => {})) }));

const defaultVp: ViewportParams = {
  offset: { x: 0, y: 0 },
  zoom: 100,
  canvasWidth: 800,
  canvasHeight: 600,
};

function makeMouseEvent(overrides: Partial<CanvasMouseEvent> = {}): CanvasMouseEvent {
  return {
    screenX: 0,
    screenY: 0,
    worldX: 0,
    worldY: 0,
    snappedX: 0,
    snappedY: 0,
    button: 0,
    shiftKey: false,
    ctrlKey: false,
    altKey: false,
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

describe('RectTool', () => {
  it('previews the same square bounds that Shift-drag commits', () => {
    const addObject = vi.fn();
    const ctx = makeToolContext({ addObject });
    const tool = new RectTool();

    tool.onMouseDown(makeMouseEvent({ snappedX: 10, snappedY: 10, screenX: 420, screenY: 320 }), ctx);
    tool.onMouseMove(makeMouseEvent({ snappedX: 30, snappedY: 50, screenX: 460, screenY: 400, shiftKey: true }), ctx);

    const overlay = tool.getOverlay();
    expect(overlay.type).toBe('shape-preview');
    if (overlay.type === 'shape-preview') {
      // World-coordinate preview: identical to the bounds the commit sends,
      // and rescaled by the renderer with the live viewport (mid-draw zoom).
      expect(overlay.startWorld).toEqual({ x: 10, y: 10 });
      expect(overlay.endWorld).toEqual({ x: 50, y: 50 });
    }

    tool.onMouseUp(makeMouseEvent({ snappedX: 30, snappedY: 50, screenX: 460, screenY: 400, shiftKey: true }), ctx);
    expect(addObject).toHaveBeenCalledWith(
      'Rectangle',
      'layer1',
      expect.objectContaining({ type: 'shape', kind: 'rectangle' }),
      { min: { x: 10, y: 10 }, max: { x: 50, y: 50 } },
    );
  });
});
