import { afterEach, describe, expect, it, vi } from 'vitest';
import { DeformSelectionTool, WarpSelectionTool } from './SelectionMeshDeformTool';
import type { CanvasMouseEvent, ToolContext } from './types';
import { vectorService } from '../../services/vectorService';
import { useProjectStore } from '../../stores/projectStore';
import { useUndoStore } from '../../stores/undoStore';
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

    const tool = new WarpSelectionTool();
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
    tool.onMouseMove(makeMouseEvent({ snappedX: 20, snappedY: 0 }), ctx);
    tool.onMouseUp(makeMouseEvent(), ctx);
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
    const tool = new DeformSelectionTool();

    tool.onMouseMove(makeMouseEvent(), ctx);

    const overlay = tool.getOverlay();
    expect(overlay).toMatchObject({ type: 'mesh-deform', gridSize: 4 });
    if (overlay.type !== 'mesh-deform') throw new Error('expected mesh overlay');
    expect(overlay.handles).toHaveLength(16);
  });
});
