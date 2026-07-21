import { beforeEach, describe, expect, it, vi } from 'vitest';
import { MoveTool } from './MoveTool';
import type { CanvasMouseEvent, ToolContext } from './types';
import type { Bounds, ProjectObject, Transform2D } from '../../types/project';
import type { ViewportParams } from '../ViewportTransform';
import { makeProjectObject } from '../../test-utils/projectFixtures';

const identity: Transform2D = { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 };

const defaultVp: ViewportParams = {
  offset: { x: 200, y: 200 },
  zoom: 100,
  canvasWidth: 800,
  canvasHeight: 600,
};

function makeVectorObject(id: string, bounds: Bounds): ProjectObject {
  return makeProjectObject({
    id,
    name: id,
    transform: { ...identity },
    bounds: {
      min: { ...bounds.min },
      max: { ...bounds.max },
    },
    layer_id: 'layer1',
    data: {
      type: 'vector_path',
      path_data: 'M 0 0 L 10 0 L 10 10 Z',
      closed: true,
    },
  });
}

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

describe('MoveTool', () => {
  let tool: MoveTool;

  beforeEach(() => {
    tool = new MoveTool();
  });

  it('starts dragging the clicked object even if selection state is stale', () => {
    const obj = makeVectorObject('svg1', {
      min: { x: 195, y: 195 },
      max: { x: 205, y: 205 },
    });
    const ctx = makeToolContext({
      objects: [obj],
      selectedObjectIds: [],
      snapToObjects: true,
    });

    tool.onMouseDown(makeMouseEvent({
      screenX: 400,
      screenY: 300,
      worldX: 200,
      worldY: 200,
      snappedX: 200,
      snappedY: 200,
    }), ctx);

    tool.onMouseMove(makeMouseEvent({
      screenX: 410,
      screenY: 300,
      worldX: 205,
      worldY: 200,
      snappedX: 210,
      snappedY: 200,
    }), ctx);

    tool.onMouseMove(makeMouseEvent({
      screenX: 420,
      screenY: 300,
      worldX: 210,
      worldY: 200,
      snappedX: 210,
      snappedY: 200,
    }), ctx);

    expect(obj.bounds.min.x).toBe(205);
    expect(obj.bounds.max.x).toBe(215);
    // Bounds-relative rendering: move only updates bounds, not transform
    expect(obj.transform.tx).toBe(0);
  });

  it('falls back to grid-snapped movement when object snap is enabled but no snap target is found', () => {
    const obj = makeVectorObject('svg1', {
      min: { x: 195, y: 195 },
      max: { x: 205, y: 205 },
    });
    const ctx = makeToolContext({
      objects: [obj],
      selectedObjectIds: ['svg1'],
      snapEnabled: true,
      snapToObjects: true,
      gridSpacingMm: 10,
    });

    tool.onMouseDown(makeMouseEvent({
      screenX: 400,
      screenY: 300,
      worldX: 200,
      worldY: 200,
      snappedX: 200,
      snappedY: 200,
    }), ctx);

    tool.onMouseMove(makeMouseEvent({
      screenX: 410,
      screenY: 300,
      worldX: 205,
      worldY: 200,
      snappedX: 210,
      snappedY: 200,
    }), ctx);

    tool.onMouseMove(makeMouseEvent({
      screenX: 412,
      screenY: 300,
      worldX: 206,
      worldY: 200,
      snappedX: 210,
      snappedY: 200,
    }), ctx);

    expect(obj.bounds.min.x).toBe(205);
    expect(obj.bounds.max.x).toBe(215);
    // Bounds-relative rendering: move only updates bounds, not transform
    expect(obj.transform.tx).toBe(0);
  });

  it('commits bounds-only update on mouse up for vector paths', () => {
    const obj = makeVectorObject('svg1', {
      min: { x: 195, y: 195 },
      max: { x: 205, y: 205 },
    });
    const updateObject = vi.fn();
    const ctx = makeToolContext({
      objects: [obj],
      selectedObjectIds: ['svg1'],
      updateObject,
    });

    tool.onMouseDown(makeMouseEvent({
      screenX: 400,
      screenY: 300,
      worldX: 200,
      worldY: 200,
      snappedX: 200,
      snappedY: 200,
    }), ctx);

    tool.onMouseMove(makeMouseEvent({
      screenX: 410,
      screenY: 300,
      worldX: 205,
      worldY: 200,
      snappedX: 205,
      snappedY: 200,
    }), ctx);

    tool.onMouseMove(makeMouseEvent({
      screenX: 420,
      screenY: 300,
      worldX: 210,
      worldY: 200,
      snappedX: 210,
      snappedY: 200,
    }), ctx);

    tool.onMouseUp(makeMouseEvent(), ctx);

    expect(updateObject).toHaveBeenCalledWith('svg1', {
      bounds: { min: { x: 205, y: 195 }, max: { x: 215, y: 205 } },
    });
  });

  it('promotes grouped child hits to the group and drags the full group', () => {
    const childA = makeVectorObject('child-a', {
      min: { x: 195, y: 195 },
      max: { x: 205, y: 205 },
    });
    childA.z_index = 10;
    const childB = makeVectorObject('child-b', {
      min: { x: 215, y: 195 },
      max: { x: 225, y: 205 },
    });
    childB.z_index = 11;
    const group = makeProjectObject({
      id: 'group-1',
      name: 'Group',
      bounds: { min: { x: 195, y: 195 }, max: { x: 225, y: 205 } },
      z_index: 0,
      layer_id: 'layer1',
      data: { type: 'group', children: ['child-a', 'child-b'] },
    });
    const selectObjects = vi.fn();
    const updateObject = vi.fn();
    const ctx = makeToolContext({
      objects: [group, childA, childB],
      selectedObjectIds: [],
      selectObjects,
      updateObject,
    });

    tool.onMouseDown(makeMouseEvent({
      screenX: 400,
      screenY: 300,
      worldX: 200,
      worldY: 200,
      snappedX: 200,
      snappedY: 200,
    }), ctx);

    tool.onMouseMove(makeMouseEvent({
      screenX: 410,
      screenY: 300,
      worldX: 205,
      worldY: 200,
      snappedX: 205,
      snappedY: 200,
    }), ctx);

    tool.onMouseMove(makeMouseEvent({
      screenX: 420,
      screenY: 300,
      worldX: 210,
      worldY: 200,
      snappedX: 210,
      snappedY: 200,
    }), ctx);
    tool.onMouseUp(makeMouseEvent(), ctx);

    expect(selectObjects).toHaveBeenCalledWith(['group-1']);
    expect(group.bounds.min.x).toBe(205);
    expect(childA.bounds.min.x).toBe(205);
    expect(childB.bounds.min.x).toBe(225);
    expect(updateObject).toHaveBeenCalledWith('group-1', {
      bounds: { min: { x: 205, y: 195 }, max: { x: 235, y: 205 } },
    });
    expect(updateObject).toHaveBeenCalledWith('child-a', {
      bounds: { min: { x: 205, y: 195 }, max: { x: 215, y: 205 } },
    });
    expect(updateObject).toHaveBeenCalledWith('child-b', {
      bounds: { min: { x: 225, y: 195 }, max: { x: 235, y: 205 } },
    });
  });
});
