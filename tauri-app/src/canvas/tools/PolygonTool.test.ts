import { beforeEach, describe, expect, it, vi } from 'vitest';
import { PolygonTool } from './PolygonTool';
import { useUiStore } from '../../stores/uiStore';
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

describe('PolygonTool', () => {
  let tool: PolygonTool;

  beforeEach(() => {
    tool = new PolygonTool();
    useUiStore.setState({ lastShapeSubTool: 'polygon' });
  });

  it('creates polygon ObjectData on mouseUp with correct type/sides', () => {
    const addObject = vi.fn();
    const ctx = makeToolContext({ addObject });

    tool.onMouseDown(makeMouseEvent({ snappedX: 10, snappedY: 10, screenX: 100, screenY: 100 }), ctx);
    tool.onMouseMove(makeMouseEvent({ snappedX: 50, snappedY: 50, screenX: 500, screenY: 500 }), ctx);
    tool.onMouseUp(makeMouseEvent({ snappedX: 50, snappedY: 50, screenX: 500, screenY: 500 }), ctx);

    expect(addObject).toHaveBeenCalledWith(
      'Polygon',
      'layer1',
      expect.objectContaining({ type: 'polygon', sides: 6, radius: 20 }),
      expect.objectContaining({
        min: { x: 10, y: 10 },
        max: { x: 50, y: 50 },
      }),
    );
  });

  it('side count is configurable via shape preset', () => {
    const addObject = vi.fn();
    const ctx = makeToolContext({ addObject });

    useUiStore.setState({ lastShapeSubTool: 'octagon' });

    tool.onMouseDown(makeMouseEvent({ snappedX: 0, snappedY: 0, screenX: 0, screenY: 0 }), ctx);
    tool.onMouseMove(makeMouseEvent({ snappedX: 30, snappedY: 30, screenX: 300, screenY: 300 }), ctx);
    tool.onMouseUp(makeMouseEvent({ snappedX: 30, snappedY: 30, screenX: 300, screenY: 300 }), ctx);

    expect(addObject).toHaveBeenCalledWith(
      'Polygon',
      'layer1',
      expect.objectContaining({ type: 'polygon', sides: 8 }),
      expect.anything(),
    );
  });

  it('bounds match the drawn area', () => {
    const addObject = vi.fn();
    const ctx = makeToolContext({ addObject });

    tool.onMouseDown(makeMouseEvent({ snappedX: 5, snappedY: 15, screenX: 50, screenY: 150 }), ctx);
    tool.onMouseMove(makeMouseEvent({ snappedX: 45, snappedY: 65, screenX: 450, screenY: 650 }), ctx);
    tool.onMouseUp(makeMouseEvent({ snappedX: 45, snappedY: 65, screenX: 450, screenY: 650 }), ctx);

    expect(addObject).toHaveBeenCalledWith(
      'Polygon',
      'layer1',
      expect.anything(),
      { min: { x: 5, y: 15 }, max: { x: 45, y: 65 } },
    );
  });
});
