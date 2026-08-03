import { afterEach, describe, expect, it, vi } from 'vitest';
import { WarpTool } from './SelectionMeshDeformTool';
import type { CanvasMouseEvent, ToolContext } from './types';
import { vectorService } from '../../services/vectorService';
import { useProjectStore } from '../../stores/projectStore';
import { useUndoStore } from '../../stores/undoStore';
import { useUiStore } from '../../stores/uiStore';
import type { ViewportParams } from '../ViewportTransform';
import { worldToScreen } from '../ViewportTransform';
import { makeProject, makeProjectObject } from '../../test-utils/projectFixtures';

vi.mock('../../services/vectorService', () => ({
  vectorService: {
    meshDeformSelection: vi.fn(),
  },
}));

const initialProjectState = useProjectStore.getState();
const initialUndoState = useUndoStore.getState();
const initialUiState = useUiStore.getState();

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
    bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } },
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
    requestOverlayRender: vi.fn(),
    ...overrides,
  };
}

async function flushToolPromises(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
}

afterEach(() => {
  vi.restoreAllMocks();
  useProjectStore.setState(initialProjectState, true);
  useUndoStore.setState(initialUndoState, true);
  useUiStore.setState(initialUiState, true);
});

describe('SelectionMeshDeformTool', () => {
  it('renders a 4-corner warp grid and applies it through the vector service', async () => {
    const ctx = makeToolContext();
    const object = ctx.objects[0];
    useProjectStore.setState({
      project: makeProject({ objects: [object] }),
      selectedObjectIds: ['obj'],
    });
    vi.spyOn(useUndoStore.getState(), 'refresh').mockResolvedValue(undefined);
    vi.mocked(vectorService.meshDeformSelection).mockResolvedValue([
      {
        ...object,
        bounds: { min: { x: 0, y: 0 }, max: { x: 20, y: 10 } },
      },
    ]);

    useUiStore.setState({ meshDeformMode: 'warp' });
    const tool = new WarpTool();
    tool.onMouseMove(makeMouseEvent(), ctx);

    const overlay = tool.getOverlay();
    expect(overlay).toMatchObject({ type: 'mesh-deform', gridSize: 2 });
    if (overlay.type !== 'mesh-deform') throw new Error('expected mesh overlay');
    expect(overlay.handles).toHaveLength(4);

    const topRight = worldToScreen({ x: 10, y: 0 }, defaultVp);
    tool.onMouseDown(
      makeMouseEvent({
        screenX: topRight.x,
        screenY: topRight.y,
        snappedX: 10,
        snappedY: 0,
      }),
      ctx,
    );
    tool.onMouseMove(makeMouseEvent({ screenX: topRight.x + 20, screenY: topRight.y, snappedX: 20, snappedY: 0 }), ctx);
    tool.onMouseUp(makeMouseEvent({ screenX: topRight.x + 20, screenY: topRight.y, snappedX: 20, snappedY: 0 }), ctx);
    await flushToolPromises();

    expect(vectorService.meshDeformSelection).toHaveBeenCalledWith(
      ['obj'],
      { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } },
      [
        { x: 0, y: 0 },
        { x: 20, y: 0 },
        { x: 0, y: 10 },
        { x: 10, y: 10 },
      ],
      2,
      true,
    );
  });

  it('renders a 16-handle deform grid', () => {
    const ctx = makeToolContext();
    useUiStore.setState({ meshDeformMode: 'mesh' });
    const tool = new WarpTool();

    tool.onMouseMove(makeMouseEvent(), ctx);

    const overlay = tool.getOverlay();
    expect(overlay).toMatchObject({ type: 'mesh-deform', gridSize: 4 });
    if (overlay.type !== 'mesh-deform') throw new Error('expected mesh overlay');
    expect(overlay.handles).toHaveLength(16);
  });

  it('starts the live preview once, then repaints only the lightweight overlay', () => {
    const ctx = makeToolContext();
    useUiStore.setState({ meshDeformMode: 'warp' });
    const tool = new WarpTool();
    tool.prepareForSelection(ctx);
    const topRight = worldToScreen({ x: 10, y: 0 }, defaultVp);

    tool.onMouseDown(makeMouseEvent({
      screenX: topRight.x,
      screenY: topRight.y,
      snappedX: 10,
      snappedY: 0,
    }), ctx);
    tool.onMouseMove(makeMouseEvent({
      screenX: topRight.x + 20,
      screenY: topRight.y + 20,
      snappedX: 15,
      snappedY: 5,
    }), ctx);

    expect(ctx.requestRender).toHaveBeenCalledTimes(1);
    const overlay = tool.getOverlay();
    if (overlay.type !== 'mesh-deform') throw new Error('expected mesh overlay');
    expect(overlay.previewActive).toBe(true);
    expect(overlay.previewObjects?.[0].paths[0].points.length).toBeGreaterThan(1);

    vi.mocked(ctx.requestRender).mockClear();
    vi.mocked(ctx.requestOverlayRender!).mockClear();
    tool.onMouseMove(makeMouseEvent({
      screenX: topRight.x + 24,
      screenY: topRight.y + 24,
      snappedX: 16,
      snappedY: 6,
    }), ctx);

    expect(ctx.requestOverlayRender).toHaveBeenCalled();
    expect(ctx.requestRender).not.toHaveBeenCalled();
  });

  it('switches modes without changing tools', () => {
    const ctx = makeToolContext();
    const tool = new WarpTool();

    useUiStore.setState({ meshDeformMode: 'warp' });
    tool.onMouseMove(makeMouseEvent(), ctx);
    expect(tool.getOverlay()).toMatchObject({ type: 'mesh-deform', gridSize: 2 });

    useUiStore.setState({ meshDeformMode: 'mesh' });
    tool.onMouseMove(makeMouseEvent(), ctx);
    const overlay = tool.getOverlay();
    expect(overlay).toMatchObject({ type: 'mesh-deform', gridSize: 4 });
    if (overlay.type !== 'mesh-deform') throw new Error('expected mesh overlay');
    expect(overlay.handles).toHaveLength(16);
  });

  it('does not move a handle until the pointer actually drags', () => {
    const ctx = makeToolContext();
    const tool = new WarpTool();
    useUiStore.setState({ meshDeformMode: 'warp' });
    tool.onMouseMove(makeMouseEvent(), ctx);
    const topRight = worldToScreen({ x: 10, y: 0 }, defaultVp);

    tool.onMouseDown(makeMouseEvent({
      screenX: topRight.x,
      screenY: topRight.y,
      snappedX: 10,
      snappedY: 0,
    }), ctx);
    tool.onMouseUp(makeMouseEvent({
      screenX: topRight.x,
      screenY: topRight.y,
      snappedX: 10,
      snappedY: 0,
    }), ctx);

    const overlay = tool.getOverlay();
    if (overlay.type !== 'mesh-deform') throw new Error('expected mesh overlay');
    expect(overlay.handles[1]).toMatchObject({ worldX: 10, worldY: 0 });
    expect(vectorService.meshDeformSelection).not.toHaveBeenCalled();
  });

  it('allows an easy off-center grab without jumping or treating a click as a drag', () => {
    const ctx = makeToolContext();
    const tool = new WarpTool();
    useUiStore.setState({ meshDeformMode: 'warp' });
    tool.prepareForSelection(ctx);
    const topRight = worldToScreen({ x: 10, y: 0 }, defaultVp);

    tool.onMouseDown(makeMouseEvent({
      screenX: topRight.x + 16,
      screenY: topRight.y + 6,
      worldX: 14,
      worldY: 1.5,
      snappedX: 14,
      snappedY: 1.5,
    }), ctx);
    tool.onMouseMove(makeMouseEvent({
      screenX: topRight.x + 17,
      screenY: topRight.y + 6,
      worldX: 14.25,
      worldY: 1.5,
      snappedX: 14.25,
      snappedY: 1.5,
    }), ctx);
    tool.onMouseUp(makeMouseEvent({
      screenX: topRight.x + 17,
      screenY: topRight.y + 6,
      worldX: 14.25,
      worldY: 1.5,
      snappedX: 14.25,
      snappedY: 1.5,
    }), ctx);

    const overlay = tool.getOverlay();
    if (overlay.type !== 'mesh-deform') throw new Error('expected mesh overlay');
    expect(overlay.handles[1]).toMatchObject({ worldX: 10, worldY: 0 });
    expect(vectorService.meshDeformSelection).not.toHaveBeenCalled();
  });

  it('preserves the grab offset once an off-center drag starts', async () => {
    const ctx = makeToolContext();
    useProjectStore.setState({
      project: makeProject({ objects: ctx.objects }),
      selectedObjectIds: ['obj'],
    });
    useUiStore.setState({ meshDeformMode: 'warp' });
    vi.spyOn(useUndoStore.getState(), 'refresh').mockResolvedValue(undefined);
    const tool = new WarpTool();
    tool.prepareForSelection(ctx);
    const topRight = worldToScreen({ x: 10, y: 0 }, defaultVp);

    tool.onMouseDown(makeMouseEvent({
      screenX: topRight.x + 8,
      screenY: topRight.y,
      worldX: 12,
      snappedX: 12,
    }), ctx);
    tool.onMouseMove(makeMouseEvent({
      screenX: topRight.x + 28,
      screenY: topRight.y + 12,
      worldX: 17,
      worldY: 3,
      snappedX: 17,
      snappedY: 3,
    }), ctx);
    tool.onMouseUp(makeMouseEvent({
      screenX: topRight.x + 28,
      screenY: topRight.y + 12,
      worldX: 17,
      worldY: 3,
      snappedX: 17,
      snappedY: 3,
    }), ctx);
    await flushToolPromises();

    expect(vectorService.meshDeformSelection).toHaveBeenCalledWith(
      ['obj'],
      expect.any(Object),
      expect.arrayContaining([{ x: 15, y: 3 }]),
      2,
      true,
    );
  });

  it('warps an imported SVG group by deforming its editable leaf objects', async () => {
    const childA = makeProjectObject({
      id: 'child-a',
      bounds: { min: { x: 0, y: 0 }, max: { x: 5, y: 10 } },
      layer_id: 'layer1',
      data: { type: 'vector_path', path_data: 'M 0 0 L 5 0 L 5 10 Z', closed: true },
    });
    const childB = makeProjectObject({
      id: 'child-b',
      bounds: { min: { x: 5, y: 0 }, max: { x: 10, y: 10 } },
      layer_id: 'layer1',
      data: { type: 'shape', kind: 'rectangle', width: 5, height: 10, corner_radius: 0 },
    });
    const group = makeProjectObject({
      id: 'svg-group',
      bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } },
      layer_id: 'layer1',
      data: { type: 'group', children: ['child-a', 'child-b'] },
    });
    const ctx = makeToolContext({
      objects: [group, childA, childB],
      selectedObjectIds: ['svg-group'],
    });
    useProjectStore.setState({
      project: makeProject({ objects: [group, childA, childB] }),
      selectedObjectIds: ['svg-group'],
    });
    useUiStore.setState({ meshDeformMode: 'warp' });
    vi.spyOn(useUndoStore.getState(), 'refresh').mockResolvedValue(undefined);
    vi.mocked(vectorService.meshDeformSelection).mockResolvedValue([childA, childB, group]);
    const tool = new WarpTool();

    tool.onMouseMove(makeMouseEvent(), ctx);
    const overlay = tool.getOverlay();
    expect(overlay).toMatchObject({ type: 'mesh-deform', gridSize: 2 });

    const topRight = worldToScreen({ x: 10, y: 0 }, defaultVp);
    tool.onMouseDown(makeMouseEvent({ screenX: topRight.x, screenY: topRight.y, snappedX: 10, snappedY: 0 }), ctx);
    tool.onMouseMove(makeMouseEvent({ screenX: topRight.x + 10, screenY: topRight.y, snappedX: 15, snappedY: 0 }), ctx);
    tool.onMouseUp(makeMouseEvent({ screenX: topRight.x + 10, screenY: topRight.y, snappedX: 15, snappedY: 0 }), ctx);
    await flushToolPromises();

    expect(vectorService.meshDeformSelection).toHaveBeenCalledWith(
      ['child-a', 'child-b'],
      { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } },
      expect.any(Array),
      2,
      true,
    );
  });

  it('prepares its visible control grid as soon as the tool is activated', () => {
    const ctx = makeToolContext();
    useUiStore.setState({ meshDeformMode: 'mesh' });
    const tool = new WarpTool();

    expect(tool.getOverlay()).toEqual({ type: 'none' });
    expect(tool.prepareForSelection(ctx)).toBe(true);
    expect(tool.getOverlay()).toMatchObject({ type: 'mesh-deform', gridSize: 4 });
  });

  it('applies the pointer-up position even when the final pointer move was throttled', async () => {
    const ctx = makeToolContext();
    useUiStore.setState({ meshDeformMode: 'warp' });
    vi.spyOn(useUndoStore.getState(), 'refresh').mockResolvedValue(undefined);
    const tool = new WarpTool();
    tool.prepareForSelection(ctx);

    const topRight = worldToScreen({ x: 10, y: 0 }, defaultVp);
    tool.onMouseDown(makeMouseEvent({ screenX: topRight.x, screenY: topRight.y, snappedX: 10, snappedY: 0 }), ctx);
    tool.onMouseUp(makeMouseEvent({ screenX: topRight.x + 10, screenY: topRight.y + 4, snappedX: 15, snappedY: 2 }), ctx);
    await flushToolPromises();

    expect(vectorService.meshDeformSelection).toHaveBeenCalledWith(
      ['obj'],
      { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } },
      expect.arrayContaining([{ x: 15, y: 2 }]),
      2,
      true,
    );
  });
});
