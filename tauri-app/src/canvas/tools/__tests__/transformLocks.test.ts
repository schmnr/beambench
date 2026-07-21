import { beforeEach, describe, expect, it, vi } from 'vitest';
import { MoveTool } from '../MoveTool';
import type { CanvasMouseEvent, ToolContext } from '../types';
import type { ProjectObject } from '../../../types/project';
import type { ViewportParams } from '../../ViewportTransform';
import { getSelectionHandles } from '../../drawSelection';
import { useNotificationStore } from '../../../stores/notificationStore';
import { makeProjectObject } from '../../../test-utils/projectFixtures';

const vp: ViewportParams = {
  offset: { x: 0, y: 0 },
  zoom: 100,
  canvasWidth: 400,
  canvasHeight: 400,
};

function makeObject(): ProjectObject {
  return makeProjectObject({
    id: 'obj-1',
    name: 'Object',
    transform: { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 },
    bounds: { min: { x: -5, y: -5 }, max: { x: 5, y: 5 } },
    layer_id: 'layer-1',
    data: { type: 'vector_path', path_data: 'M0,0 L10,0', closed: false },
  });
}

function makeMouseEvent(overrides: Partial<CanvasMouseEvent> = {}): CanvasMouseEvent {
  return {
    screenX: 200,
    screenY: 200,
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

function makeContext(object: ProjectObject, overrides: Partial<ToolContext> = {}): ToolContext {
  return {
    vp,
    objects: [object],
    selectedObjectIds: [object.id],
    selectedLayerId: 'layer-1',
    layers: [{ id: 'layer-1', enabled: true, operation: 'line' }],
    transformLocks: { move_enabled: true, size_enabled: true, rotate_enabled: true, shear_enabled: true },
    snapEnabled: false,
    snapToObjects: false,
    gridSpacingMm: 10,
    selectObjects: vi.fn(),
    toggleObjectSelection: vi.fn(),
    addObject: vi.fn(),
    updateObject: vi.fn().mockResolvedValue(undefined),
    rotateObjects: vi.fn().mockResolvedValue(undefined),
    shearObjects: vi.fn(),
    updateObjectBoundsBatch: vi.fn(),
    setCursorWorldPos: vi.fn(),
    setStatusMessage: vi.fn(),
    requestRender: vi.fn(),
    ...overrides,
  };
}

describe('transform lock enforcement', () => {
  beforeEach(() => {
    useNotificationStore.setState({ notifications: [] });
  });

  it('MoveTool blocks dragging when position lock is enabled', () => {
    const tool = new MoveTool();
    const object = makeObject();
    const ctx = makeContext(object, { transformLocks: { move_enabled: false, size_enabled: true, rotate_enabled: true, shear_enabled: true } });

    tool.onMouseDown(makeMouseEvent(), ctx);
    tool.onMouseMove(makeMouseEvent({ screenX: 240, worldX: 10, snappedX: 10 }), ctx);
    tool.onMouseUp(makeMouseEvent({ screenX: 240, worldX: 10, snappedX: 10 }), ctx);

    expect(object.bounds.min.x).toBe(-5);
    expect(ctx.updateObject).not.toHaveBeenCalled();
    const notifications = useNotificationStore.getState().notifications;
    expect(notifications[notifications.length - 1]?.message).toContain('Position is locked');
  });

  it('SelectTool hides resize handles when scale lock is enabled', () => {
    const object = makeObject();

    // Without lock, se handle exists
    const handlesUnlocked = getSelectionHandles([object], vp);
    expect(handlesUnlocked.find((h) => h.id === 'se')).toBeDefined();

    // With scale lock, resize handles are hidden
    const handlesLocked = getSelectionHandles([object], vp, { move_enabled: true, size_enabled: false, rotate_enabled: true, shear_enabled: true });
    expect(handlesLocked.find((h) => h.id === 'se')).toBeUndefined();
    expect(handlesLocked.find((h) => h.id === 'nw')).toBeUndefined();
    expect(handlesLocked.find((h) => h.id === 'n')).toBeUndefined();
    expect(handlesLocked.find((h) => h.id === 'ne')).toBeUndefined();
    expect(handlesLocked.find((h) => h.id === 'w')).toBeUndefined();
    expect(handlesLocked.find((h) => h.id === 'e')).toBeUndefined();
    expect(handlesLocked.find((h) => h.id === 'sw')).toBeUndefined();
    expect(handlesLocked.find((h) => h.id === 's')).toBeUndefined();
    // But center, rotation, and shear handles are still present
    expect(handlesLocked.find((h) => h.id === 'center')).toBeDefined();
    expect(handlesLocked.find((h) => h.id === 'rotate_nw')).toBeDefined();
    expect(handlesLocked.find((h) => h.id === 'shear_n')).toBeDefined();
  });
});
