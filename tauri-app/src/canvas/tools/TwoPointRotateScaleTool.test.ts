import { describe, expect, it, vi } from 'vitest';
import { TwoPointRotateScaleTool } from './TwoPointRotateScaleTool';
import type { CanvasMouseEvent, ToolContext } from './types';
import type { Transform2D } from '../../types/project';
import type { ViewportParams } from '../ViewportTransform';
import { makeProjectObject } from '../../test-utils/projectFixtures';

const identity: Transform2D = { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 };

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
  const object = makeProjectObject({
    id: 'obj',
    transform: { ...identity },
    bounds: { min: { x: 10, y: 0 }, max: { x: 20, y: 10 } },
    layer_id: 'layer1',
    data: { type: 'vector_path', path_data: 'M 0 0 L 10 0 L 10 10 Z', closed: true },
  });
  return {
    vp: defaultVp,
    objects: [object],
    selectedObjectIds: ['obj'],
    selectedLayerId: 'layer1',
    layers: [{ id: 'layer1', enabled: true }],
    snapEnabled: false,
    snapToObjects: false,
    gridSpacingMm: 10,
    selectObjects: vi.fn(),
    toggleObjectSelection: vi.fn(),
    addObject: vi.fn(),
    updateObject: vi.fn(),
    rotateObjects: vi.fn().mockResolvedValue(undefined),
    shearObjects: vi.fn().mockResolvedValue(undefined),
    updateObjectBoundsBatch: vi.fn().mockResolvedValue(undefined),
    setCursorWorldPos: vi.fn(),
    setStatusMessage: vi.fn(),
    requestRender: vi.fn(),
    ...overrides,
  };
}

async function flushToolPromises(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
}

describe('TwoPointRotateScaleTool', () => {
  it('uses click pivot plus drag second point to rotate selected objects', async () => {
    const ctx = makeToolContext();
    const tool = new TwoPointRotateScaleTool();

    tool.onMouseDown(makeMouseEvent({ snappedX: 0, snappedY: 0 }), ctx);
    tool.onMouseDown(makeMouseEvent({ snappedX: 10, snappedY: 0 }), ctx);
    tool.onMouseMove(makeMouseEvent({ snappedX: 0, snappedY: 10 }), ctx);
    tool.onMouseUp(makeMouseEvent({ snappedX: 0, snappedY: 10 }), ctx);
    await flushToolPromises();

    expect(ctx.rotateObjects).toHaveBeenCalledWith(['obj'], 90, { x: 0, y: 0 });
  });

  it('scales around the pivot when Shift is held during release', async () => {
    const ctx = makeToolContext();
    const tool = new TwoPointRotateScaleTool();

    tool.onMouseDown(makeMouseEvent({ snappedX: 0, snappedY: 0 }), ctx);
    tool.onMouseDown(makeMouseEvent({ snappedX: 10, snappedY: 0 }), ctx);
    tool.onMouseMove(makeMouseEvent({ snappedX: 20, snappedY: 0, shiftKey: true }), ctx);
    tool.onMouseUp(makeMouseEvent({ snappedX: 20, snappedY: 0, shiftKey: true }), ctx);
    await flushToolPromises();

    expect(ctx.updateObjectBoundsBatch).toHaveBeenCalledWith([
      { id: 'obj', bounds: { min: { x: 20, y: 0 }, max: { x: 40, y: 20 } } },
    ]);
    expect(ctx.rotateObjects).not.toHaveBeenCalled();
  });

  it('scales grouped members around the pivot when Shift is held', async () => {
    const child = makeProjectObject({
      id: 'child',
      transform: { ...identity },
      bounds: { min: { x: 10, y: 0 }, max: { x: 20, y: 10 } },
      layer_id: 'layer1',
    });
    const group = makeProjectObject({
      id: 'group',
      transform: { ...identity },
      bounds: { min: { x: 10, y: 0 }, max: { x: 20, y: 10 } },
      layer_id: 'layer1',
      data: { type: 'group', children: ['child'] },
    });
    const ctx = makeToolContext({
      objects: [child, group],
      selectedObjectIds: ['group'],
    });
    const tool = new TwoPointRotateScaleTool();

    tool.onMouseDown(makeMouseEvent({ snappedX: 0, snappedY: 0 }), ctx);
    tool.onMouseDown(makeMouseEvent({ snappedX: 10, snappedY: 0 }), ctx);
    tool.onMouseMove(makeMouseEvent({ snappedX: 20, snappedY: 0, shiftKey: true }), ctx);
    tool.onMouseUp(makeMouseEvent({ snappedX: 20, snappedY: 0, shiftKey: true }), ctx);
    await flushToolPromises();

    expect(ctx.updateObjectBoundsBatch).toHaveBeenCalledWith([
      { id: 'group', bounds: { min: { x: 20, y: 0 }, max: { x: 40, y: 20 } } },
      { id: 'child', bounds: { min: { x: 20, y: 0 }, max: { x: 40, y: 20 } } },
    ]);
    expect(ctx.rotateObjects).not.toHaveBeenCalled();
  });

  it('cancels the pending pivot with Escape', async () => {
    const ctx = makeToolContext();
    const tool = new TwoPointRotateScaleTool();

    tool.onMouseDown(makeMouseEvent({ snappedX: 0, snappedY: 0 }), ctx);
    tool.onKeyDown({ key: 'Escape' } as KeyboardEvent, ctx);
    tool.onMouseDown(makeMouseEvent({ snappedX: 10, snappedY: 0 }), ctx);
    tool.onMouseUp(makeMouseEvent({ snappedX: 0, snappedY: 10 }), ctx);
    await flushToolPromises();

    expect(ctx.rotateObjects).not.toHaveBeenCalled();
    expect(ctx.updateObjectBoundsBatch).not.toHaveBeenCalled();
  });
});
