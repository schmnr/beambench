import { describe, it, expect, vi, beforeEach } from 'vitest';
import { NodeTool } from './NodeTool';
import type { CanvasMouseEvent, ToolContext } from './types';
import { worldToScreen, type ViewportParams } from '../ViewportTransform';
import type { Project, ProjectObject, Transform2D, Bounds } from '../../types/project';
import { vectorService } from '../../services/vectorService';
import { useProjectStore } from '../../stores/projectStore';
import { makeProjectObject } from '../../test-utils/projectFixtures';
import { resolveCanvasPointerSnap } from '../pointerSnap';
import type { EditablePath, NodeId, NodeSelectionTarget } from '../../types/vector';

// Mock vectorService
vi.mock('../../services/vectorService', () => ({
  vectorService: {
    getEditablePath: vi.fn(),
    convertToPath: vi.fn(),
    updateNode: vi.fn(),
    updateNodesBatch: vi.fn(),
    setNodeType: vi.fn(),
    deleteNode: vi.fn(),
    deleteNodes: vi.fn(),
    insertNode: vi.fn(),
    deleteSegment: vi.fn(),
    breakPathAtNode: vi.fn(),
    convertSegmentToLine: vi.fn(),
    alignSegmentToAngle: vi.fn(),
    trimSegmentToIntersection: vi.fn(),
    extendEndpointToIntersection: vi.fn(),
    joinSubpaths: vi.fn(),
    togglePathClosed: vi.fn(),
    closeAndJoin: vi.fn(),
    convertSegmentToCurve: vi.fn(),
  },
}));

vi.mock('../pointerSnap', () => ({
  resolveCanvasPointerSnap: vi.fn(),
}));

// Mock uiStore
const uiState = {
  nodeSubMode: 'select',
  nudgeStepMm: 5,
  nudgeStepFineMm: 1,
  nudgeStepCoarseMm: 20,
  setNodeEditNodeCount: vi.fn(),
  setNodeSubMode: vi.fn(),
};

vi.mock('../../stores/uiStore', () => ({
  useUiStore: {
    getState: () => uiState,
  },
}));

const identity: Transform2D = { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 };
const originalRotateObjectsAndBakeActivePath = useProjectStore.getState().rotateObjectsAndBakeActivePath;
const originalCloseAndJoin = useProjectStore.getState().closeAndJoin;
const originalRemoveObject = useProjectStore.getState().removeObject;

const defaultVp: ViewportParams = {
  offset: { x: 200, y: 200 },
  zoom: 100,
  canvasWidth: 800,
  canvasHeight: 600,
};

const originVp: ViewportParams = {
  offset: { x: 0, y: 0 },
  zoom: 100,
  canvasWidth: 800,
  canvasHeight: 600,
};

function makeShapeObj(
  id: string,
  bounds: Bounds,
): ProjectObject {
  return makeProjectObject({
    id,
    name: id,
    transform: identity,
    bounds,
    layer_id: 'layer1',
    data: { type: 'shape', kind: 'rectangle', width: 10, height: 10, corner_radius: 0 },
  });
}

function makeVectorPathObj(
  id: string,
  bounds: Bounds,
): ProjectObject {
  return makeProjectObject({
    id,
    name: id,
    transform: identity,
    bounds,
    layer_id: 'layer1',
    data: { type: 'vector_path', path_data: 'M 0 0 L 10 0 L 10 10 Z', closed: true },
  });
}

function makeTransformedVectorPathObj(
  id: string,
  bounds: Bounds,
): ProjectObject {
  return {
    ...makeVectorPathObj(id, bounds),
    transform: { a: 0, b: 1, c: -1, d: 0, tx: 5, ty: 6 },
  };
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

function deferred<T>(): {
  promise: Promise<T>;
  resolve: (value: T | PromiseLike<T>) => void;
} {
  let resolve!: (value: T | PromiseLike<T>) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

function primeStraightPathTool(tool: NodeTool, objectId = 'path1'): ProjectObject {
  const obj = makeVectorPathObj(objectId, { min: { x: 0, y: 0 }, max: { x: 10, y: 0 } });
  // @ts-expect-error private state setup
  tool.objectId = objectId;
  // @ts-expect-error private state setup
  tool.objectBounds = { min: { x: 0, y: 0 }, max: { x: 10, y: 0 } };
  // @ts-expect-error private state setup
  tool.pathBBox = { minX: 0, minY: 0, maxX: 10, maxY: 0, width: 10, height: 0 };
  // @ts-expect-error private state setup
  tool.editablePaths = [{
    closed: false,
    nodes: [
      {
        id: { subpath_idx: 0, command_idx: 0 },
        position: { x: 0, y: 0 },
        handle_in: null,
        handle_out: null,
        node_type: 'corner',
      },
      {
        id: { subpath_idx: 0, command_idx: 1 },
        position: { x: 10, y: 0 },
        handle_in: null,
        handle_out: null,
        node_type: 'corner',
      },
    ],
  }];
  return obj;
}

function primeThreeNodePathTool(tool: NodeTool, objectId = 'path1'): ProjectObject {
  const obj = makeVectorPathObj(objectId, { min: { x: 0, y: 0 }, max: { x: 20, y: 0 } });
  // @ts-expect-error private state setup
  tool.objectId = objectId;
  // @ts-expect-error private state setup
  tool.objectBounds = { min: { x: 0, y: 0 }, max: { x: 20, y: 0 } };
  // @ts-expect-error private state setup
  tool.pathBBox = { minX: 0, minY: 0, maxX: 20, maxY: 0, width: 20, height: 0 };
  // @ts-expect-error private state setup
  tool.editablePaths = [{
    closed: false,
    nodes: [
      {
        id: { subpath_idx: 0, command_idx: 0 },
        position: { x: 0, y: 0 },
        handle_in: null,
        handle_out: null,
        node_type: 'corner',
      },
      {
        id: { subpath_idx: 0, command_idx: 1 },
        position: { x: 10, y: 0 },
        handle_in: null,
        handle_out: null,
        node_type: 'corner',
      },
      {
        id: { subpath_idx: 0, command_idx: 2 },
        position: { x: 20, y: 0 },
        handle_in: null,
        handle_out: null,
        node_type: 'corner',
      },
    ],
  }];
  return obj;
}

function makeLineEditablePath(
  subpathIdx: number,
  start: { x: number; y: number },
  end: { x: number; y: number },
  closed = false,
): EditablePath {
  return {
    closed,
    nodes: [
      {
        id: { subpath_idx: subpathIdx, command_idx: 0 },
        position: start,
        handle_in: null,
        handle_out: null,
        node_type: 'corner',
      },
      {
        id: { subpath_idx: subpathIdx, command_idx: 1 },
        position: end,
        handle_in: null,
        handle_out: null,
        node_type: 'corner',
      },
    ],
  };
}

describe('NodeTool', () => {
  let tool: NodeTool;

  beforeEach(() => {
    tool = new NodeTool();
    vi.clearAllMocks();
    uiState.nodeSubMode = 'select';
    useProjectStore.setState({
      project: null,
      rotateObjectsAndBakeActivePath: originalRotateObjectsAndBakeActivePath,
      closeAndJoin: originalCloseAndJoin,
      removeObject: originalRemoveObject,
    });
    vi.mocked(vectorService.getEditablePath).mockResolvedValue([]);
    vi.mocked(vectorService.convertToPath).mockResolvedValue(
      makeVectorPathObj('converted', { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } }),
    );
    vi.mocked(vectorService.deleteNode).mockResolvedValue(
      makeVectorPathObj('path1', { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } }),
    );
    vi.mocked(vectorService.deleteNodes).mockResolvedValue(
      makeVectorPathObj('path1', { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } }),
    );
    vi.mocked(vectorService.insertNode).mockResolvedValue(
      makeVectorPathObj('path1', { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } }),
    );
    vi.mocked(vectorService.deleteSegment).mockResolvedValue(
      makeVectorPathObj('path1', { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } }),
    );
    vi.mocked(vectorService.breakPathAtNode).mockResolvedValue(
      makeVectorPathObj('path1', { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } }),
    );
    vi.mocked(vectorService.setNodeType).mockResolvedValue(
      makeVectorPathObj('path1', { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } }),
    );
    vi.mocked(vectorService.convertSegmentToLine).mockResolvedValue(
      makeVectorPathObj('path1', { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } }),
    );
    vi.mocked(vectorService.convertSegmentToCurve).mockResolvedValue(
      makeVectorPathObj('path1', { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } }),
    );
    vi.mocked(vectorService.trimSegmentToIntersection).mockResolvedValue(
      makeVectorPathObj('path1', { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } }),
    );
    vi.mocked(vectorService.extendEndpointToIntersection).mockResolvedValue(
      makeVectorPathObj('path1', { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } }),
    );
    vi.mocked(resolveCanvasPointerSnap).mockImplementation(({ world }) => ({
      snapped: world,
      nextPreferredTargetKey: null,
    }));
  });

  it('has name "node"', () => {
    expect(tool.name).toBe('node');
  });

  it('returns "none" overlay when no paths are loaded', () => {
    expect(tool.getOverlay()).toEqual({ type: 'none' });
  });

  it('returns "default" cursor in idle state', () => {
    expect(tool.getCursor()).toBe('default');
  });

  it('shows status message when no object is selected', () => {
    const ctx = makeToolContext({ selectedObjectIds: [] });
    tool.onMouseDown(makeMouseEvent(), ctx);
    expect(ctx.setStatusMessage).toHaveBeenCalledWith(
      'Select a vector path to edit nodes',
    );
  });

  it('allows multiple selected objects while loading the active vector path', async () => {
    const objA = makeVectorPathObj('path1', { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } });
    const objB = makeVectorPathObj('path2', { min: { x: 20, y: 0 }, max: { x: 30, y: 10 } });
    const ctx = makeToolContext({
      selectedObjectIds: ['path1', 'path2'],
      objects: [objA, objB],
    });

    await tool.prepareForSelection(ctx);

    expect(ctx.setStatusMessage).not.toHaveBeenCalled();
    expect(vectorService.getEditablePath).toHaveBeenCalledWith('path1');
  });

  it('triggers auto-conversion when selected object is not a vector path', () => {
    const obj = makeShapeObj('shape1', { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } });
    const ctx = makeToolContext({
      selectedObjectIds: ['shape1'],
      objects: [obj],
    });
    tool.onMouseDown(makeMouseEvent(), ctx);
    // onMouseDown calls prepareForSelection asynchronously, which will attempt conversion
    // No synchronous status message — the conversion happens in the async path
    expect(ctx.setStatusMessage).not.toHaveBeenCalled();
  });

  it('preloads editable path for a selected vector path', async () => {
    vi.mocked(vectorService.getEditablePath).mockResolvedValueOnce([
      { closed: true, nodes: [] },
    ]);
    const obj = makeVectorPathObj('path1', { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } });
    const ctx = makeToolContext({
      selectedObjectIds: ['path1'],
      objects: [obj],
    });

    await tool.prepareForSelection(ctx);

    expect(vectorService.getEditablePath).toHaveBeenCalledWith('path1');
    expect(tool.getOverlay()).toMatchObject({ type: 'node-edit', objectId: 'path1' });
  });

  it('maps editable nodes from the freshly loaded path when frontend path data is stale after a move', async () => {
    const freshPaths = [{
      closed: true,
      nodes: [
        {
          id: { subpath_idx: 0, command_idx: 0 },
          position: { x: 100, y: 100 },
          handle_in: null,
          handle_out: null,
          node_type: 'corner' as const,
        },
        {
          id: { subpath_idx: 0, command_idx: 1 },
          position: { x: 110, y: 100 },
          handle_in: null,
          handle_out: null,
          node_type: 'corner' as const,
        },
        {
          id: { subpath_idx: 0, command_idx: 2 },
          position: { x: 110, y: 110 },
          handle_in: null,
          handle_out: null,
          node_type: 'corner' as const,
        },
      ],
    }];
    vi.mocked(vectorService.getEditablePath).mockResolvedValueOnce(freshPaths);
    const staleFrontendObj = makeVectorPathObj('path1', {
      min: { x: 100, y: 100 },
      max: { x: 110, y: 110 },
    });
    const ctx = makeToolContext({
      selectedObjectIds: ['path1'],
      objects: [staleFrontendObj],
    });

    await tool.prepareForSelection(ctx);

    const overlay = tool.getOverlay();
    if (overlay.type !== 'node-edit' || !overlay.nodeToWorld) {
      throw new Error('Expected node-edit overlay');
    }
    expect(overlay.nodeToWorld(freshPaths[0].nodes[0].position)).toEqual({ x: 100, y: 100 });
    expect(overlay.nodeToWorld(freshPaths[0].nodes[2].position)).toEqual({ x: 110, y: 110 });
  });

  it('auto-converts shapes to vector paths when entering node edit', async () => {
    const convertedObj = makeVectorPathObj('shape1', { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } });
    vi.mocked(vectorService.convertToPath).mockResolvedValueOnce(convertedObj);
    vi.mocked(vectorService.getEditablePath).mockResolvedValueOnce([
      { closed: true, nodes: [] },
    ]);

    const shape = makeShapeObj('shape1', { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } });
    const ctx = makeToolContext({
      selectedObjectIds: ['shape1'],
      objects: [shape],
    });

    await tool.prepareForSelection(ctx);

    expect(vectorService.convertToPath).toHaveBeenCalledWith('shape1');
  });

  it('auto-converts transformed vector paths to bake in transform', async () => {
    const transformed = makeTransformedVectorPathObj('path1', { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } });
    const convertedObj = makeVectorPathObj('path1', { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } });
    vi.mocked(vectorService.convertToPath).mockResolvedValueOnce(convertedObj);
    vi.mocked(vectorService.getEditablePath).mockResolvedValueOnce([
      { closed: true, nodes: [] },
    ]);
    const ctx = makeToolContext({
      selectedObjectIds: ['path1'],
      objects: [transformed],
    });

    await tool.prepareForSelection(ctx);

    expect(vectorService.convertToPath).toHaveBeenCalledWith('path1');
    expect(vectorService.getEditablePath).toHaveBeenCalledWith('path1');
  });

  it('reloads editable paths when the selected object data changes', async () => {
    const obj = makeVectorPathObj('path1', { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } });
    vi.mocked(vectorService.getEditablePath)
      .mockResolvedValueOnce([{ closed: true, nodes: [] }])
      .mockResolvedValueOnce([{ closed: true, nodes: [] }]);

    const firstCtx = makeToolContext({
      selectedObjectIds: ['path1'],
      objects: [obj],
    });
    await tool.prepareForSelection(firstCtx);

    const changed = {
      ...obj,
      data: { ...obj.data, path_data: 'M 0 0 L 20 0 L 20 10 Z' as const },
    };
    const secondCtx = makeToolContext({
      selectedObjectIds: ['path1'],
      objects: [changed],
    });
    await tool.prepareForSelection(secondCtx);

    expect(vectorService.getEditablePath).toHaveBeenCalledTimes(2);
  });

  it('ignores stale editable-path loads that resolve after a newer load', async () => {
    const obj = makeVectorPathObj('path1', { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } });
    const firstLoad = deferred<EditablePath[]>();
    const secondLoad = deferred<EditablePath[]>();
    vi.mocked(vectorService.getEditablePath)
      .mockReturnValueOnce(firstLoad.promise)
      .mockReturnValueOnce(secondLoad.promise);
    const ctx = makeToolContext({
      selectedObjectIds: ['path1'],
      objects: [obj],
    });

    // @ts-expect-error private method test
    const firstPromise = tool.loadEditablePath('path1', ctx);
    // @ts-expect-error private method test
    const secondPromise = tool.loadEditablePath('path1', ctx);

    secondLoad.resolve([makeLineEditablePath(0, { x: 0, y: 0 }, { x: 10, y: 0 }, true)]);
    await secondPromise;
    firstLoad.resolve([makeLineEditablePath(0, { x: 0, y: 0 }, { x: 10, y: 0 }, false)]);
    await firstPromise;

    const overlay = tool.getOverlay();
    if (overlay.type !== 'node-edit') throw new Error('Expected node-edit overlay');
    expect(overlay.paths[0].closed).toBe(true);
  });

  it('preserves node selection when project refresh reloads the same editable path', async () => {
    const obj = makeVectorPathObj('path1', { min: { x: 0, y: 0 }, max: { x: 10, y: 0 } });
    const selectedTarget: NodeSelectionTarget = {
      kind: 'handle',
      nodeId: { subpath_idx: 0, command_idx: 1 },
      handleType: 'in',
    };
    const freshPath = [{
      closed: false,
      nodes: [
        {
          id: { subpath_idx: 0, command_idx: 0 },
          position: { x: 0, y: 0 },
          handle_in: null,
          handle_out: { x: 3, y: 0 },
          node_type: 'corner' as const,
        },
        {
          id: { subpath_idx: 0, command_idx: 1 },
          position: { x: 10, y: 0 },
          handle_in: { x: 7, y: 0 },
          handle_out: null,
          node_type: 'corner' as const,
        },
      ],
    }];
    vi.mocked(vectorService.getEditablePath).mockResolvedValueOnce(freshPath);
    // @ts-expect-error private state setup
    tool.objectId = 'path1';
    // @ts-expect-error private state setup
    tool.editablePaths = freshPath;
    // @ts-expect-error private state setup
    tool.selectedTargets = [selectedTarget];
    // @ts-expect-error private state setup
    tool.primaryTarget = selectedTarget;
    // @ts-expect-error private state setup
    tool.loadedSignature = 'stale-signature';

    await tool.prepareForSelection(makeToolContext({
      selectedObjectIds: ['path1'],
      objects: [obj],
    }));

    const overlay = tool.getOverlay();
    if (overlay.type !== 'node-edit') throw new Error('Expected node-edit overlay');
    expect(overlay.selectedTargets).toEqual([selectedTarget]);
    expect(overlay.primaryTarget).toEqual(selectedTarget);
  });

  it('getNodeCount returns 0 when no paths loaded', () => {
    expect(tool.getNodeCount()).toBe(0);
  });

  it('reset clears all state', () => {
    uiState.nodeSubMode = 'align';
    tool.reset();
    expect(tool.getOverlay()).toEqual({ type: 'none' });
    expect(tool.getNodeCount()).toBe(0);
    expect(uiState.setNodeSubMode).toHaveBeenCalledWith('select');
  });

  it('owns Escape while node editing so it returns to select submode', () => {
    uiState.nodeSubMode = 'trim';
    const event = {
      key: 'Escape',
      preventDefault: vi.fn(),
      stopImmediatePropagation: vi.fn(),
    } as unknown as KeyboardEvent;
    const ctx = makeToolContext();

    tool.onKeyDown(event, ctx);

    expect(event.preventDefault).toHaveBeenCalled();
    expect(event.stopImmediatePropagation).toHaveBeenCalled();
    expect(uiState.setNodeSubMode).toHaveBeenCalledWith('select');
    expect(ctx.requestRender).toHaveBeenCalled();
  });

  it('getCursor returns "default" for idle state', () => {
    expect(tool.getCursor()).toBe('default');
  });

  it('triggers convertToPath for polygon objects', async () => {
    const convertedObj = makeVectorPathObj('poly1', { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } });
    vi.mocked(vectorService.convertToPath).mockResolvedValueOnce(convertedObj);
    vi.mocked(vectorService.getEditablePath).mockResolvedValueOnce([
      { closed: true, nodes: [] },
    ]);

    const polygon: ProjectObject = makeProjectObject({
      id: 'poly1',
      name: 'poly1',
      transform: identity,
      bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } },
      layer_id: 'layer1',
      // schema-correct polygon data (no phantom path_data/closed).
      data: { type: 'polygon', sides: 6, radius: 5 },
    });
    const ctx = makeToolContext({
      selectedObjectIds: ['poly1'],
      objects: [polygon],
    });

    await tool.prepareForSelection(ctx);

    expect(vectorService.convertToPath).toHaveBeenCalledWith('poly1');
  });

  it('triggers convertToPath for star objects', async () => {
    const convertedObj = makeVectorPathObj('star1', { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } });
    vi.mocked(vectorService.convertToPath).mockResolvedValueOnce(convertedObj);
    vi.mocked(vectorService.getEditablePath).mockResolvedValueOnce([
      { closed: true, nodes: [] },
    ]);

    const star: ProjectObject = makeProjectObject({
      id: 'star1',
      name: 'star1',
      transform: identity,
      bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } },
      layer_id: 'layer1',
      // schema-correct star data matches the production ObjectData union.
      data: { type: 'star', points: 5, bulge: 0, ratio: 0.5, dual_radius: false, ratio2: null, corner_radius: 0, corner_radii: [] },
    });
    const ctx = makeToolContext({
      selectedObjectIds: ['star1'],
      objects: [star],
    });

    await tool.prepareForSelection(ctx);

    expect(vectorService.convertToPath).toHaveBeenCalledWith('star1');
  });

  it('uses the last active open subpath for close path when no node is currently selected', async () => {
    const updatedObj = makeVectorPathObj('path1', { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } });
    vi.mocked(vectorService.togglePathClosed).mockResolvedValueOnce(updatedObj);
    vi.mocked(vectorService.getEditablePath).mockResolvedValueOnce([
      { closed: false, nodes: [] },
      { closed: false, nodes: [] },
    ]);

    // @ts-expect-error accessing private field
    tool.objectId = 'path1';
    // @ts-expect-error accessing private field
    tool.editablePaths = [{ closed: false, nodes: [] }, { closed: false, nodes: [] }];
    // @ts-expect-error accessing private field
    tool.activeSubpathIdx = 1;

    const ctx = makeToolContext();
    // @ts-expect-error accessing private method
    tool.handleCloseOpenClick(ctx);

    await vi.waitFor(() => {
      expect(vectorService.togglePathClosed).toHaveBeenCalledWith('path1', 1);
    });
  });

  it('does not open a closed subpath when there are no open paths to close', () => {
    // @ts-expect-error accessing private field
    tool.objectId = 'path1';
    // @ts-expect-error accessing private field
    tool.editablePaths = [
      { closed: true, nodes: [] },
      { closed: true, nodes: [] },
    ];
    // @ts-expect-error accessing private field
    tool.activeSubpathIdx = 0;

    const ctx = makeToolContext();
    // @ts-expect-error accessing private method
    tool.handleCloseOpenClick(ctx);

    expect(vectorService.togglePathClosed).not.toHaveBeenCalled();
    expect(ctx.setStatusMessage).toHaveBeenCalledWith('No open paths to close');
    expect(ctx.requestRender).toHaveBeenCalled();
  });

  it('closes an open subpath instead of opening the active closed subpath', async () => {
    const updatedObj = makeVectorPathObj('path1', { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } });
    vi.mocked(vectorService.togglePathClosed).mockResolvedValueOnce(updatedObj);
    vi.mocked(vectorService.getEditablePath).mockResolvedValueOnce([
      { closed: true, nodes: [] },
      { closed: true, nodes: [] },
    ]);

    // @ts-expect-error accessing private field
    tool.objectId = 'path1';
    // @ts-expect-error accessing private field
    tool.editablePaths = [
      { closed: true, nodes: [] },
      { closed: false, nodes: [] },
    ];
    // @ts-expect-error accessing private field
    tool.activeSubpathIdx = 0;

    const ctx = makeToolContext();
    // @ts-expect-error accessing private method
    tool.handleCloseOpenClick(ctx);

    await vi.waitFor(() => {
      expect(vectorService.togglePathClosed).toHaveBeenCalledWith('path1', 1);
    });
  });

  it('resets close path mode immediately before the async toggle resolves', () => {
    vi.mocked(vectorService.togglePathClosed).mockReturnValue(new Promise(() => {}));

    // @ts-expect-error accessing private field
    tool.objectId = 'path1';
    // @ts-expect-error accessing private field
    tool.editablePaths = [{ closed: false, nodes: [] }];
    uiState.nodeSubMode = 'close_open';

    const ctx = makeToolContext();
    // @ts-expect-error accessing private method
    tool.handleCloseOpenClick(ctx);

    expect(uiState.setNodeSubMode).toHaveBeenCalledWith('select');
    expect(vectorService.togglePathClosed).toHaveBeenCalledWith('path1', 0);
  });

  it('resets auto-join mode immediately before the async join resolves', async () => {
    const closeAndJoin = vi.fn().mockReturnValue(new Promise(() => {}));
    useProjectStore.setState({ closeAndJoin });

    // @ts-expect-error accessing private field
    tool.objectId = 'path1';
    uiState.nodeSubMode = 'auto_join';

    const ctx = makeToolContext();
    // @ts-expect-error accessing private method
    tool.handleAutoJoinClick(ctx);

    expect(uiState.setNodeSubMode).toHaveBeenCalledWith('select');
    await vi.waitFor(() => {
      expect(closeAndJoin).toHaveBeenCalledWith(['path1'], 0.5, { warnIfOpen: false });
    });
  });

  it('auto-join uses the full current selection instead of only the active node path', async () => {
    const closeAndJoin = vi.fn().mockReturnValue(new Promise(() => {}));
    useProjectStore.setState({ closeAndJoin });

    // @ts-expect-error accessing private field
    tool.objectId = 'path1';

    const ctx = makeToolContext({ selectedObjectIds: ['path1', 'path2'] });
    // @ts-expect-error accessing private method
    tool.handleAutoJoinClick(ctx);

    await vi.waitFor(() => {
      expect(closeAndJoin).toHaveBeenCalledWith(['path1', 'path2'], 0.5, { warnIfOpen: false });
    });
  });

  it('waits for a pending node-position commit before auto-joining', async () => {
    const obj = primeStraightPathTool(tool);
    const pendingUpdate = deferred<ProjectObject>();
    const joinedObj = makeVectorPathObj('joined1', { min: { x: 0, y: 0 }, max: { x: 20, y: 0 } });
    const closeAndJoin = vi.fn().mockResolvedValue({ object: joinedObj, fullyClosed: true });
    const ctx = makeToolContext({ selectedObjectIds: ['path1'], objects: [obj] });
    const target: NodeSelectionTarget = {
      kind: 'node',
      nodeId: { subpath_idx: 0, command_idx: 1 },
    };

    useProjectStore.setState({
      selectedObjectIds: ['path1'],
      closeAndJoin,
    });
    // @ts-expect-error private state setup
    tool.selectedTargets = [target];
    // @ts-expect-error private state setup
    tool.primaryTarget = target;
    vi.mocked(vectorService.updateNodesBatch).mockReturnValueOnce(pendingUpdate.promise);

    tool.onKeyDown(
      { key: 'ArrowRight', preventDefault: vi.fn() } as unknown as KeyboardEvent,
      ctx,
    );
    tool.performImmediateAction('auto_join', ctx);

    expect(closeAndJoin).not.toHaveBeenCalled();

    pendingUpdate.resolve(obj);

    await vi.waitFor(() => {
      expect(closeAndJoin).toHaveBeenCalledWith(['path1'], 0.5, { warnIfOpen: false });
    });
  });

  it('waits for a pending delete-segment commit before auto-joining', async () => {
    const obj = primeStraightPathTool(tool);
    const pendingDelete = deferred<ProjectObject>();
    const closeAndJoin = vi.fn().mockResolvedValue({ object: obj, fullyClosed: true });
    const ctx = makeToolContext({ selectedObjectIds: ['path1'], objects: [obj] });
    useProjectStore.setState({
      selectedObjectIds: ['path1'],
      closeAndJoin,
    });
    vi.mocked(vectorService.deleteSegment).mockReturnValueOnce(pendingDelete.promise);
    vi.mocked(vectorService.getEditablePath).mockResolvedValue([
      makeLineEditablePath(0, { x: 0, y: 0 }, { x: 10, y: 0 }),
    ]);

    // @ts-expect-error private method test
    tool.deleteSegmentByHit({ nodeId: { subpath_idx: 0, command_idx: 1 }, t: 0.5 }, ctx);
    // @ts-expect-error private method test
    tool.handleAutoJoinClick(ctx);

    expect(closeAndJoin).not.toHaveBeenCalled();

    pendingDelete.resolve(obj);

    await vi.waitFor(() => {
      expect(closeAndJoin).toHaveBeenCalledWith(['path1'], 0.5, { warnIfOpen: false });
    });
  });

  it('flushes locally moved node edits before auto-joining when no commit is pending', async () => {
    const obj = primeStraightPathTool(tool);
    const pendingUpdate = deferred<ProjectObject>();
    const closeAndJoin = vi.fn().mockResolvedValue({ object: obj, fullyClosed: false });
    const ctx = makeToolContext({ selectedObjectIds: ['path1'], objects: [obj] });
    const target: NodeSelectionTarget = {
      kind: 'node',
      nodeId: { subpath_idx: 0, command_idx: 1 },
    };

    useProjectStore.setState({ closeAndJoin });
    // @ts-expect-error private state setup
    tool.selectedTargets = [target];
    // @ts-expect-error private state setup
    tool.primaryTarget = target;
    // @ts-expect-error private state setup
    tool.editablePaths[0].nodes[1].position = { x: 9.75, y: 0 };
    // @ts-expect-error private state setup
    tool.localNodeDirty = true;
    vi.mocked(vectorService.updateNodesBatch).mockReturnValueOnce(pendingUpdate.promise);

    tool.performImmediateAction('auto_join', ctx);

    expect(vectorService.updateNodesBatch).toHaveBeenCalledWith('path1', [
      {
        node_id: { subpath_idx: 0, command_idx: 1 },
        x: 9.75,
        y: 0,
        handle_type: null,
      },
    ]);
    expect(closeAndJoin).not.toHaveBeenCalled();

    pendingUpdate.resolve(obj);

    await vi.waitFor(() => {
      expect(closeAndJoin).toHaveBeenCalledWith(['path1'], 0.5, { warnIfOpen: false });
    });
  });

  it('does not report auto-join complete when the store action produces no result', async () => {
    const closeAndJoin = vi.fn().mockResolvedValue(null);
    useProjectStore.setState({ closeAndJoin });

    // @ts-expect-error accessing private field
    tool.objectId = 'path1';

    const ctx = makeToolContext();
    // @ts-expect-error accessing private method
    tool.handleAutoJoinClick(ctx);

    await vi.waitFor(() => {
      expect(closeAndJoin).toHaveBeenCalledWith(['path1'], 0.5, { warnIfOpen: false });
    });
    expect(ctx.setStatusMessage).not.toHaveBeenCalledWith('Auto-Join complete');
    expect(ctx.requestRender).toHaveBeenCalled();
  });

  it('requests a canvas render after auto-join reloads the joined editable path', async () => {
    const joinedObj = makeVectorPathObj('joined1', { min: { x: 0, y: 0 }, max: { x: 20, y: 20 } });
    const closeAndJoin = vi.fn().mockImplementation(async () => {
      useProjectStore.setState({
        project: { objects: [joinedObj] } as Project,
        selectedObjectIds: ['joined1'],
      });
      return { object: joinedObj, fullyClosed: true };
    });
    useProjectStore.setState({ closeAndJoin });
    vi.mocked(vectorService.getEditablePath).mockResolvedValue([
      {
        closed: true,
        nodes: [
          {
            id: { subpath_idx: 0, command_idx: 0 },
            position: { x: 0, y: 0 },
            handle_in: null,
            handle_out: null,
            node_type: 'corner',
          },
          {
            id: { subpath_idx: 0, command_idx: 1 },
            position: { x: 20, y: 20 },
            handle_in: null,
            handle_out: null,
            node_type: 'corner',
          },
        ],
      },
    ]);
    // @ts-expect-error accessing private field
    tool.objectId = 'path1';

    const ctx = makeToolContext({ selectedObjectIds: ['path1'] });
    // @ts-expect-error accessing private method
    tool.handleAutoJoinClick(ctx);

    await vi.waitFor(() => {
      expect(vectorService.getEditablePath).toHaveBeenCalledWith('joined1');
    });
    expect(ctx.requestRender).toHaveBeenCalled();
    expect(ctx.setStatusMessage).toHaveBeenCalledWith('Auto-Join complete');
  });

  it('immediate close path action runs the one-shot close without a canvas click', () => {
    vi.mocked(vectorService.togglePathClosed).mockReturnValue(new Promise(() => {}));

    // @ts-expect-error accessing private field
    tool.objectId = 'path1';
    // @ts-expect-error accessing private field
    tool.editablePaths = [{ closed: false, nodes: [] }];
    uiState.nodeSubMode = 'close_open';

    const ctx = makeToolContext();
    tool.performImmediateAction('close_open', ctx);

    expect(uiState.setNodeSubMode).toHaveBeenCalledWith('select');
    expect(vectorService.togglePathClosed).toHaveBeenCalledWith('path1', 0);
  });

  it('updates hovered segment when only t changes on the same edge', () => {
    const ctx = makeToolContext();
    uiState.nodeSubMode = 'insert';
    // @ts-expect-error accessing private field
    tool.hoveredSegment = {
      nodeId: { subpath_idx: 0, command_idx: 1 },
      t: 0.2,
    };
    // @ts-expect-error accessing private method
    tool.hitTestSegment = vi.fn().mockReturnValue({
      nodeId: { subpath_idx: 0, command_idx: 1 },
      t: 0.7,
    });

    tool.onMouseMove(makeMouseEvent(), ctx);

    // @ts-expect-error accessing private field
    expect(tool.hoveredSegment).toEqual({
      nodeId: { subpath_idx: 0, command_idx: 1 },
      t: 0.7,
    });
    expect(ctx.requestRender).toHaveBeenCalled();
  });

  it('insert midpoint submode resolves the segment at click time', () => {
    const obj = primeStraightPathTool(tool);
    const ctx = makeToolContext({
      selectedObjectIds: ['path1'],
      objects: [obj],
      vp: originVp,
    });
    uiState.nodeSubMode = 'insert_midpoint';

    tool.onMouseDown(makeMouseEvent({ screenX: 410, screenY: 300, worldX: 5, worldY: 0 }), ctx);

    expect(vectorService.insertNode).toHaveBeenCalledWith('path1', 0, 1, 0.5);
  });

  it('midpoint toolbar one-click action still uses the cached hovered segment', () => {
    const obj = primeStraightPathTool(tool);
    const ctx = makeToolContext({ selectedObjectIds: ['path1'], objects: [obj], vp: originVp });
    // @ts-expect-error private state setup
    tool.hoveredSegment = { nodeId: { subpath_idx: 0, command_idx: 1 }, t: 0.25 };

    tool.performImmediateAction('midpoint', ctx);

    expect(vectorService.insertNode).toHaveBeenCalledWith('path1', 0, 1, 0.5);
  });

  it('trim and extend submodes resolve click-time targets', () => {
    const obj = primeStraightPathTool(tool);
    const ctx = makeToolContext({ selectedObjectIds: ['path1'], objects: [obj], vp: originVp });

    uiState.nodeSubMode = 'trim';
    tool.onMouseDown(makeMouseEvent({ screenX: 410, screenY: 300, worldX: 5, worldY: 0 }), ctx);
    expect(vectorService.trimSegmentToIntersection).toHaveBeenCalledWith('path1', 0, 1, 5, 0);

    uiState.nodeSubMode = 'extend';
    tool.onMouseDown(makeMouseEvent({ screenX: 420, screenY: 300, worldX: 10, worldY: 0 }), ctx);
    expect(vectorService.extendEndpointToIntersection).toHaveBeenCalledWith(
      'path1',
      { subpath_idx: 0, command_idx: 1 },
    );
  });

  it('switches the active editable object when clicking another selected vector path', async () => {
    const top = makeVectorPathObj('top', { min: { x: 0, y: -2 }, max: { x: 10, y: 2 } });
    const bottom = makeVectorPathObj('bottom', { min: { x: 0, y: 18 }, max: { x: 10, y: 22 } });
    const ctx = makeToolContext({
      selectedObjectIds: ['top', 'bottom'],
      objects: [top, bottom],
      vp: originVp,
    });
    // @ts-expect-error private state setup
    tool.objectId = 'top';
    // @ts-expect-error private state setup
    tool.objectBounds = top.bounds;
    // @ts-expect-error private state setup
    tool.pathBBox = { minX: 0, minY: 0, maxX: 10, maxY: 0, width: 10, height: 0 };
    // @ts-expect-error private state setup
    tool.editablePaths = [makeLineEditablePath(0, { x: 0, y: 0 }, { x: 10, y: 0 })];
    vi.mocked(vectorService.getEditablePath).mockResolvedValueOnce([
      makeLineEditablePath(0, { x: 0, y: 20 }, { x: 10, y: 20 }),
    ]);
    uiState.nodeSubMode = 'delete_segment';

    tool.onMouseDown(makeMouseEvent({ screenX: 410, screenY: 340, worldX: 5, worldY: 20 }), ctx);

    await vi.waitFor(() => {
      expect(vectorService.getEditablePath).toHaveBeenCalledWith('bottom');
      expect(vectorService.deleteSegment).toHaveBeenCalledWith('bottom', 0, 1);
    });
  });

  it('select-mode segment clicks remember the clicked subpath for path-level actions', () => {
    const obj = makeVectorPathObj('path1', { min: { x: 0, y: 0 }, max: { x: 10, y: 20 } });
    const ctx = makeToolContext({
      selectedObjectIds: ['path1'],
      objects: [obj],
      vp: originVp,
    });
    // @ts-expect-error private state setup
    tool.objectId = 'path1';
    // @ts-expect-error private state setup
    tool.objectBounds = obj.bounds;
    // @ts-expect-error private state setup
    tool.pathBBox = { minX: 0, minY: 0, maxX: 10, maxY: 20, width: 10, height: 20 };
    // @ts-expect-error private state setup
    tool.editablePaths = [
      makeLineEditablePath(0, { x: 0, y: 0 }, { x: 10, y: 0 }),
      makeLineEditablePath(1, { x: 0, y: 20 }, { x: 10, y: 20 }),
    ];

    tool.onMouseDown(makeMouseEvent({ screenX: 410, screenY: 340, worldX: 5, worldY: 20 }), ctx);

    // @ts-expect-error private state read
    expect(tool.activeSubpathIdx).toBe(1);
  });

  it('align submode rotates all selected objects around the clicked segment world midpoint', () => {
    const obj = makeVectorPathObj('path1', { min: { x: 0, y: 0 }, max: { x: 8, y: 6 } });
    const rotateObjectsAndBakeActivePath = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({ rotateObjectsAndBakeActivePath });
    const zoomedVp: ViewportParams = {
      offset: { x: -12, y: 8 },
      zoom: 250,
      canvasWidth: 800,
      canvasHeight: 600,
    };
    const ctx = makeToolContext({
      selectedObjectIds: ['path1', 'other1'],
      objects: [obj],
      vp: zoomedVp,
    });
    // @ts-expect-error private state setup
    tool.objectId = 'path1';
    // @ts-expect-error private state setup
    tool.objectBounds = { min: { x: 0, y: 0 }, max: { x: 8, y: 6 } };
    // @ts-expect-error private state setup
    tool.pathBBox = { minX: 0, minY: 0, maxX: 8, maxY: 6, width: 8, height: 6 };
    // @ts-expect-error private state setup
    tool.editablePaths = [{
      closed: false,
      nodes: [
        {
          id: { subpath_idx: 0, command_idx: 0 },
          position: { x: 0, y: 0 },
          handle_in: null,
          handle_out: null,
          node_type: 'corner',
        },
        {
          id: { subpath_idx: 0, command_idx: 1 },
          position: { x: 8, y: 6 },
          handle_in: null,
          handle_out: null,
          node_type: 'corner',
        },
      ],
    }];
    uiState.nodeSubMode = 'align';
    const screenPt = worldToScreen({ x: 4, y: 3 }, zoomedVp);

    tool.onMouseDown(makeMouseEvent({ screenX: screenPt.x, screenY: screenPt.y, worldX: 4, worldY: 3 }), ctx);

    expect(rotateObjectsAndBakeActivePath).toHaveBeenCalledWith(
      ['path1', 'other1'],
      expect.closeTo(8.130102354, 6),
      { x: 4, y: 3 },
      'path1',
    );
  });

  it('keyboard hover actions insert, delete segment, break, and convert node corner', () => {
    const obj = primeStraightPathTool(tool);
    const ctx = makeToolContext({ selectedObjectIds: ['path1'], objects: [obj], vp: originVp });
    // @ts-expect-error private state setup
    tool.hoveredSegment = { nodeId: { subpath_idx: 0, command_idx: 1 }, t: 0.25 };

    tool.onKeyDown({ key: 'i' } as KeyboardEvent, ctx);
    expect(vectorService.insertNode).toHaveBeenCalledWith('path1', 0, 1, 0.25);

    tool.onKeyDown({ key: 'd' } as KeyboardEvent, ctx);
    expect(vectorService.deleteSegment).toHaveBeenCalledWith('path1', 0, 1);

    // @ts-expect-error private state setup
    tool.hoveredSegment = null;
    // @ts-expect-error private state setup
    tool.hoveredEndpoint = { subpath_idx: 0, command_idx: 1 };

    tool.onKeyDown({ key: 'b' } as KeyboardEvent, ctx);
    expect(vectorService.breakPathAtNode).toHaveBeenCalledWith('path1', 0, 1);

    tool.onKeyDown({ key: 'c' } as KeyboardEvent, ctx);
    expect(vectorService.setNodeType).toHaveBeenCalledWith('path1', 0, 1, 'corner');
  });

  it('keyboard S converts hovered straight segment to curve and L converts hovered curve to line', () => {
    const obj = primeStraightPathTool(tool);
    const ctx = makeToolContext({ selectedObjectIds: ['path1'], objects: [obj], vp: originVp });
    // @ts-expect-error private state setup
    tool.hoveredSegment = { nodeId: { subpath_idx: 0, command_idx: 1 }, t: 0.5 };

    tool.onKeyDown({ key: 's' } as KeyboardEvent, ctx);
    expect(vectorService.convertSegmentToCurve).toHaveBeenCalledWith('path1', 0, 1);

    // @ts-expect-error private state setup
    tool.editablePaths[0].nodes[0].handle_out = { x: 3, y: 0 };
    // @ts-expect-error private state setup
    tool.editablePaths[0].nodes[1].handle_in = { x: 7, y: 0 };

    tool.onKeyDown({ key: 'l' } as KeyboardEvent, ctx);
    expect(vectorService.convertSegmentToLine).toHaveBeenCalledWith('path1', 0, 1);
  });

  it('keeps a smoothed node selected after reload so generated handles stay visible', async () => {
    const obj = makeVectorPathObj('path1', { min: { x: 0, y: 0 }, max: { x: 20, y: 0 } });
    const ctx = makeToolContext({ selectedObjectIds: ['path1'], objects: [obj], vp: originVp });
    // @ts-expect-error private state setup
    tool.objectId = 'path1';
    // @ts-expect-error private state setup
    tool.objectBounds = { min: { x: 0, y: 0 }, max: { x: 20, y: 0 } };
    // @ts-expect-error private state setup
    tool.pathBBox = { minX: 0, minY: 0, maxX: 20, maxY: 0, width: 20, height: 0 };
    // @ts-expect-error private state setup
    tool.editablePaths = [{
      closed: false,
      nodes: [
        { id: { subpath_idx: 0, command_idx: 0 }, position: { x: 0, y: 0 }, handle_in: null, handle_out: null, node_type: 'corner' },
        { id: { subpath_idx: 0, command_idx: 1 }, position: { x: 10, y: 0 }, handle_in: null, handle_out: null, node_type: 'corner' },
        { id: { subpath_idx: 0, command_idx: 2 }, position: { x: 20, y: 0 }, handle_in: null, handle_out: null, node_type: 'corner' },
      ],
    }];
    // @ts-expect-error private state setup
    tool.hoveredEndpoint = { subpath_idx: 0, command_idx: 1 };
    vi.mocked(vectorService.getEditablePath).mockResolvedValueOnce([{
      closed: false,
      nodes: [
        { id: { subpath_idx: 0, command_idx: 0 }, position: { x: 0, y: 0 }, handle_in: null, handle_out: { x: 3, y: 0 }, node_type: 'corner' },
        { id: { subpath_idx: 0, command_idx: 1 }, position: { x: 10, y: 0 }, handle_in: { x: 7, y: 0 }, handle_out: { x: 13, y: 0 }, node_type: 'smooth' },
        { id: { subpath_idx: 0, command_idx: 2 }, position: { x: 20, y: 0 }, handle_in: { x: 17, y: 0 }, handle_out: null, node_type: 'corner' },
      ],
    }]);

    tool.onKeyDown({ key: 's' } as KeyboardEvent, ctx);

    await vi.waitFor(() => {
      expect(vectorService.getEditablePath).toHaveBeenCalledWith('path1');
    });
    const overlay = tool.getOverlay();
    if (overlay.type !== 'node-edit') throw new Error('Expected node-edit overlay');
    expect(overlay.selectedTargets).toEqual([
      { kind: 'node', nodeId: { subpath_idx: 0, command_idx: 1 } },
    ]);
    expect(overlay.paths[0].nodes[1].handle_in).toEqual({ x: 7, y: 0 });
    expect(overlay.paths[0].nodes[1].handle_out).toEqual({ x: 13, y: 0 });
  });

  it('selects both endpoints after converting a straight segment to a curve', async () => {
    const obj = primeStraightPathTool(tool);
    const ctx = makeToolContext({ selectedObjectIds: ['path1'], objects: [obj], vp: originVp });
    // @ts-expect-error private state setup
    tool.hoveredSegment = { nodeId: { subpath_idx: 0, command_idx: 1 }, t: 0.5 };
    vi.mocked(vectorService.getEditablePath).mockResolvedValueOnce([{
      closed: false,
      nodes: [
        { id: { subpath_idx: 0, command_idx: 0 }, position: { x: 0, y: 0 }, handle_in: null, handle_out: { x: 3, y: 0 }, node_type: 'corner' },
        { id: { subpath_idx: 0, command_idx: 1 }, position: { x: 10, y: 0 }, handle_in: { x: 7, y: 0 }, handle_out: null, node_type: 'corner' },
      ],
    }]);

    tool.onKeyDown({ key: 's' } as KeyboardEvent, ctx);

    await vi.waitFor(() => {
      expect(vectorService.getEditablePath).toHaveBeenCalledWith('path1');
    });
    const overlay = tool.getOverlay();
    if (overlay.type !== 'node-edit') throw new Error('Expected node-edit overlay');
    expect(overlay.selectedTargets).toEqual([
      { kind: 'node', nodeId: { subpath_idx: 0, command_idx: 0 } },
      { kind: 'node', nodeId: { subpath_idx: 0, command_idx: 1 } },
    ]);
  });

  it('keeps sibling snap available on the same path while excluding the dragged node itself', () => {
    const ctx = makeToolContext({
      selectedObjectIds: ['path1'],
      objects: [makeVectorPathObj('path1', { min: { x: 0, y: 0 }, max: { x: 10, y: 0 } })],
      snapEnabled: true,
      snapToObjects: true,
    });
    // @ts-expect-error private state setup
    tool.objectId = 'path1';
    // @ts-expect-error private state setup
    tool.objectBounds = { min: { x: 0, y: 0 }, max: { x: 10, y: 0 } };
    // @ts-expect-error private state setup
    tool.pathBBox = { minX: 0, minY: 0, maxX: 10, maxY: 0, width: 10, height: 0 };
    // @ts-expect-error private state setup
    tool.editablePaths = [{
      closed: false,
      nodes: [
        {
          id: { subpath_idx: 0, command_idx: 0 },
          position: { x: 0, y: 0 },
          handle_in: null,
          handle_out: null,
          node_type: 'corner',
        },
        {
          id: { subpath_idx: 0, command_idx: 1 },
          position: { x: 10, y: 0 },
          handle_in: null,
          handle_out: null,
          node_type: 'corner',
        },
      ],
    }];
    // @ts-expect-error private state setup
    tool.selectedTargets = [{ kind: 'node', nodeId: { subpath_idx: 0, command_idx: 0 } }];
    // @ts-expect-error private state setup
    tool.primaryTarget = { kind: 'node', nodeId: { subpath_idx: 0, command_idx: 0 } };
    // @ts-expect-error private state setup
    tool.state = {
      type: 'dragging',
      target: { kind: 'node', nodeId: { subpath_idx: 0, command_idx: 0 } },
      startWorld: { x: 0, y: 0 },
      initialPoints: [{
        target: { kind: 'node', nodeId: { subpath_idx: 0, command_idx: 0 } },
        world: { x: 0, y: 0 },
      }],
      excludedPoints: [{ x: 0, y: 0 }],
      preferredTargetKey: null,
      mirroredTarget: null,
    };

    vi.mocked(resolveCanvasPointerSnap).mockImplementation((args) => {
      expect(args.excludedPoints).toEqual([{ x: 0, y: 0 }]);
      return {
        snapped: { x: 10, y: 0 },
        nextPreferredTargetKey: 'path1:pt:1',
      };
    });

    tool.onMouseMove(makeMouseEvent({ screenX: 230, screenY: 200, worldX: 6, worldY: 0 }), ctx);

    // @ts-expect-error private helper access
    const moved = tool.findNode({ subpath_idx: 0, command_idx: 0 });
    expect(moved).not.toBeNull();
    expect(moved!.position.x).toBe(10);
  });

  it('shift-click on a second node adds it to selectedTargets without clearing the first', () => {
    const obj = makeVectorPathObj('path1', { min: { x: 0, y: 0 }, max: { x: 10, y: 0 } });
    const ctx = makeToolContext({
      selectedObjectIds: ['path1'],
      objects: [obj],
      // Origin viewport so worldToScreen(p) = (p.x * 2 + 400, p.y * 2 + 300).
      vp: { offset: { x: 0, y: 0 }, zoom: 100, canvasWidth: 800, canvasHeight: 600 },
    });
    // @ts-expect-error private state setup
    tool.objectId = 'path1';
    // @ts-expect-error private state setup
    tool.objectBounds = { min: { x: 0, y: 0 }, max: { x: 10, y: 0 } };
    // @ts-expect-error private state setup
    tool.pathBBox = { minX: 0, minY: 0, maxX: 10, maxY: 0, width: 10, height: 0 };
    // @ts-expect-error private state setup
    tool.editablePaths = [{
      closed: false,
      nodes: [
        {
          id: { subpath_idx: 0, command_idx: 0 },
          position: { x: 0, y: 0 },
          handle_in: null,
          handle_out: null,
          node_type: 'corner',
        },
        {
          id: { subpath_idx: 0, command_idx: 1 },
          position: { x: 10, y: 0 },
          handle_in: null,
          handle_out: null,
          node_type: 'corner',
        },
      ],
    }];

    // First click selects node 0 (no shift). World (0,0) → screen (400, 300).
    tool.onMouseDown(makeMouseEvent({ screenX: 400, screenY: 300, worldX: 0, worldY: 0 }), ctx);
    // @ts-expect-error private state read
    expect(tool.selectedTargets).toHaveLength(1);
    // @ts-expect-error private state read
    expect(tool.selectedTargets[0].nodeId.command_idx).toBe(0);

    // Shift-click adds node 1 without dropping node 0. World (10,0) → screen (420, 300).
    tool.onMouseDown(
      makeMouseEvent({ screenX: 420, screenY: 300, worldX: 10, worldY: 0, shiftKey: true }),
      ctx,
    );
    // @ts-expect-error private state read
    expect(tool.selectedTargets).toHaveLength(2);
    // @ts-expect-error private state read
    const cmdIdxs = tool.selectedTargets.map((t: { nodeId: NodeId }) => t.nodeId.command_idx).sort();
    expect(cmdIdxs).toEqual([0, 1]);
  });

  it('rubber-band drag selects multiple nodes', () => {
    const obj = primeThreeNodePathTool(tool);
    const ctx = makeToolContext({
      selectedObjectIds: ['path1'],
      objects: [obj],
      vp: originVp,
    });

    tool.onMouseDown(makeMouseEvent({ screenX: 380, screenY: 280 }), ctx);
    tool.onMouseMove(makeMouseEvent({ screenX: 430, screenY: 310 }), ctx);

    const overlay = tool.getOverlay();
    if (overlay.type !== 'node-edit') throw new Error('Expected node-edit overlay');
    expect(overlay.selectionRect).toMatchObject({
      startScreen: { x: 380, y: 280 },
      endScreen: { x: 430, y: 310 },
    });

    tool.onMouseUp(makeMouseEvent(), ctx);

    // @ts-expect-error private state read
    const cmdIdxs = tool.selectedTargets.map((t: { nodeId: NodeId }) => t.nodeId.command_idx).sort();
    expect(cmdIdxs).toEqual([0, 1]);
  });

  it('shift rubber-band adds node hits to the existing node selection', () => {
    const obj = primeThreeNodePathTool(tool);
    const ctx = makeToolContext({
      selectedObjectIds: ['path1'],
      objects: [obj],
      vp: originVp,
    });
    // @ts-expect-error private state setup
    tool.selectedTargets = [{ kind: 'node', nodeId: { subpath_idx: 0, command_idx: 2 } }];
    // @ts-expect-error private state setup
    tool.primaryTarget = { kind: 'node', nodeId: { subpath_idx: 0, command_idx: 2 } };

    tool.onMouseDown(makeMouseEvent({ screenX: 380, screenY: 280, shiftKey: true }), ctx);
    tool.onMouseMove(makeMouseEvent({ screenX: 430, screenY: 310, shiftKey: true }), ctx);
    tool.onMouseUp(makeMouseEvent({ shiftKey: true }), ctx);

    // @ts-expect-error private state read
    const cmdIdxs = tool.selectedTargets.map((t: { nodeId: NodeId }) => t.nodeId.command_idx).sort();
    expect(cmdIdxs).toEqual([0, 1, 2]);
  });

  it('Ctrl/Cmd+A selects every editable node', () => {
    const obj = primeThreeNodePathTool(tool);
    const ctx = makeToolContext({
      selectedObjectIds: ['path1'],
      objects: [obj],
    });
    const preventDefault = vi.fn();
    const stopImmediatePropagation = vi.fn();

    tool.onKeyDown({
      key: 'a',
      metaKey: true,
      preventDefault,
      stopImmediatePropagation,
    } as unknown as KeyboardEvent, ctx);

    expect(preventDefault).toHaveBeenCalled();
    expect(stopImmediatePropagation).toHaveBeenCalled();
    // @ts-expect-error private state read
    const cmdIdxs = tool.selectedTargets.map((t: { nodeId: NodeId }) => t.nodeId.command_idx).sort();
    expect(cmdIdxs).toEqual([0, 1, 2]);
  });

  it('Delete removes all selected nodes through the batch command', () => {
    const obj = primeThreeNodePathTool(tool);
    const ctx = makeToolContext({
      selectedObjectIds: ['path1'],
      objects: [obj],
    });
    // @ts-expect-error private state setup
    tool.selectedTargets = [
      { kind: 'node', nodeId: { subpath_idx: 0, command_idx: 0 } },
      { kind: 'node', nodeId: { subpath_idx: 0, command_idx: 2 } },
    ];
    // @ts-expect-error private state setup
    tool.primaryTarget = { kind: 'node', nodeId: { subpath_idx: 0, command_idx: 2 } };
    const preventDefault = vi.fn();
    const stopImmediatePropagation = vi.fn();

    tool.onKeyDown({
      key: 'Delete',
      preventDefault,
      stopImmediatePropagation,
    } as unknown as KeyboardEvent, ctx);

    expect(preventDefault).toHaveBeenCalled();
    expect(stopImmediatePropagation).toHaveBeenCalled();
    expect(vectorService.deleteNodes).toHaveBeenCalledWith('path1', [
      { subpath_idx: 0, command_idx: 0 },
      { subpath_idx: 0, command_idx: 2 },
    ]);
    expect(vectorService.deleteNode).not.toHaveBeenCalled();
  });

  it('Delete removes the object when every editable node is selected', async () => {
    const obj = primeThreeNodePathTool(tool);
    const ctx = makeToolContext({
      selectedObjectIds: ['path1'],
      objects: [obj],
    });
    const removeObject = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({ removeObject } as never);
    // @ts-expect-error private state setup
    tool.selectedTargets = [
      { kind: 'node', nodeId: { subpath_idx: 0, command_idx: 0 } },
      { kind: 'node', nodeId: { subpath_idx: 0, command_idx: 1 } },
      { kind: 'node', nodeId: { subpath_idx: 0, command_idx: 2 } },
    ];
    // @ts-expect-error private state setup
    tool.primaryTarget = { kind: 'node', nodeId: { subpath_idx: 0, command_idx: 2 } };
    const preventDefault = vi.fn();
    const stopImmediatePropagation = vi.fn();

    tool.onKeyDown({
      key: 'Delete',
      preventDefault,
      stopImmediatePropagation,
    } as unknown as KeyboardEvent, ctx);
    await Promise.resolve();

    expect(preventDefault).toHaveBeenCalled();
    expect(stopImmediatePropagation).toHaveBeenCalled();
    expect(removeObject).toHaveBeenCalledWith('path1');
    expect(vectorService.deleteNodes).not.toHaveBeenCalled();
    expect(tool.getOverlay()).toEqual({ type: 'none' });
    expect(ctx.requestRender).toHaveBeenCalled();
  });

  it('D removes the object when the hovered node is the only editable node', async () => {
    const obj = makeVectorPathObj('path1', { min: { x: 0, y: 0 }, max: { x: 0, y: 0 } });
    const ctx = makeToolContext({
      selectedObjectIds: ['path1'],
      objects: [obj],
    });
    const removeObject = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({ removeObject } as never);
    // @ts-expect-error private state setup
    tool.objectId = 'path1';
    // @ts-expect-error private state setup
    tool.objectBounds = { min: { x: 0, y: 0 }, max: { x: 0, y: 0 } };
    // @ts-expect-error private state setup
    tool.pathBBox = { minX: 0, minY: 0, maxX: 0, maxY: 0, width: 0, height: 0 };
    // @ts-expect-error private state setup
    tool.editablePaths = [{
      closed: false,
      nodes: [{
        id: { subpath_idx: 0, command_idx: 0 },
        position: { x: 0, y: 0 },
        handle_in: null,
        handle_out: null,
        node_type: 'corner',
      }],
    }];
    // @ts-expect-error private state setup
    tool.hoveredEndpoint = { subpath_idx: 0, command_idx: 0 };

    tool.onKeyDown({ key: 'd' } as KeyboardEvent, ctx);
    await Promise.resolve();

    expect(removeObject).toHaveBeenCalledWith('path1');
    expect(vectorService.deleteNode).not.toHaveBeenCalled();
    expect(tool.getOverlay()).toEqual({ type: 'none' });
  });

  it('Delete in node edit mode does not fall through to object deletion when no nodes are selected', () => {
    const obj = primeThreeNodePathTool(tool);
    const ctx = makeToolContext({
      selectedObjectIds: ['path1'],
      objects: [obj],
    });
    const preventDefault = vi.fn();
    const stopImmediatePropagation = vi.fn();

    tool.onKeyDown({
      key: 'Delete',
      preventDefault,
      stopImmediatePropagation,
    } as unknown as KeyboardEvent, ctx);

    expect(preventDefault).toHaveBeenCalled();
    expect(stopImmediatePropagation).toHaveBeenCalled();
    expect(vectorService.deleteNodes).not.toHaveBeenCalled();
    expect(vectorService.deleteNode).not.toHaveBeenCalled();
  });

  it('arrow keys nudge selected nodes by 5 mm / 20 mm with Shift / 1 mm with Ctrl / 0.1 mm with Ctrl+Shift', () => {
    const obj = makeVectorPathObj('path1', { min: { x: 0, y: 0 }, max: { x: 10, y: 0 } });
    const ctx = makeToolContext({ selectedObjectIds: ['path1'], objects: [obj] });
    // @ts-expect-error private state setup
    tool.objectId = 'path1';
    // @ts-expect-error private state setup
    tool.objectBounds = { min: { x: 0, y: 0 }, max: { x: 10, y: 0 } };
    // @ts-expect-error private state setup
    tool.pathBBox = { minX: 0, minY: 0, maxX: 10, maxY: 0, width: 10, height: 0 };
    // @ts-expect-error private state setup
    tool.editablePaths = [{
      closed: false,
      nodes: [{
        id: { subpath_idx: 0, command_idx: 0 },
        position: { x: 0, y: 0 },
        handle_in: null,
        handle_out: null,
        node_type: 'corner',
      }],
    }];
    // @ts-expect-error private state setup
    tool.selectedTargets = [{ kind: 'node', nodeId: { subpath_idx: 0, command_idx: 0 } }];
    // @ts-expect-error private state setup
    tool.primaryTarget = { kind: 'node', nodeId: { subpath_idx: 0, command_idx: 0 } };

    vi.mocked(vectorService.updateNodesBatch).mockResolvedValue(obj);

    // Default: 5 mm step
    tool.onKeyDown(
      { key: 'ArrowRight', preventDefault: vi.fn() } as unknown as KeyboardEvent,
      ctx,
    );
    expect(vectorService.updateNodesBatch).toHaveBeenLastCalledWith('path1', [
      expect.objectContaining({ x: 5, y: 0 }),
    ]);

    // Reset position so the next nudge is from (0, 0)
    // @ts-expect-error private state reset
    tool.editablePaths[0].nodes[0].position = { x: 0, y: 0 };

    // Shift: 20 mm step
    tool.onKeyDown(
      { key: 'ArrowRight', shiftKey: true, preventDefault: vi.fn() } as unknown as KeyboardEvent,
      ctx,
    );
    expect(vectorService.updateNodesBatch).toHaveBeenLastCalledWith('path1', [
      expect.objectContaining({ x: 20, y: 0 }),
    ]);

    // @ts-expect-error private state reset
    tool.editablePaths[0].nodes[0].position = { x: 0, y: 0 };

    // Ctrl: 1 mm step
    tool.onKeyDown(
      { key: 'ArrowRight', ctrlKey: true, preventDefault: vi.fn() } as unknown as KeyboardEvent,
      ctx,
    );
    expect(vectorService.updateNodesBatch).toHaveBeenLastCalledWith('path1', [
      expect.objectContaining({ x: 1, y: 0 }),
    ]);

    // @ts-expect-error private state reset
    tool.editablePaths[0].nodes[0].position = { x: 0, y: 0 };

    // Ctrl+Shift: fine / 10 extra-fine step
    tool.onKeyDown(
      { key: 'ArrowRight', ctrlKey: true, shiftKey: true, preventDefault: vi.fn() } as unknown as KeyboardEvent,
      ctx,
    );
    expect(vectorService.updateNodesBatch).toHaveBeenLastCalledWith('path1', [
      expect.objectContaining({ x: 0.1, y: 0 }),
    ]);
  });

  it('drag-to-join commits the moved endpoint before joining subpaths', async () => {
    const obj = makeVectorPathObj('path1', { min: { x: 0, y: 0 }, max: { x: 20, y: 0 } });
    const ctx = makeToolContext({ selectedObjectIds: ['path1'], objects: [obj] });
    // @ts-expect-error private state setup
    tool.objectId = 'path1';
    // @ts-expect-error private state setup
    tool.objectBounds = { min: { x: 0, y: 0 }, max: { x: 20, y: 0 } };
    // @ts-expect-error private state setup
    tool.pathBBox = { minX: 0, minY: 0, maxX: 20, maxY: 0, width: 20, height: 0 };
    // @ts-expect-error private state setup
    tool.editablePaths = [{
      closed: false,
      nodes: [
        {
          id: { subpath_idx: 0, command_idx: 0 },
          position: { x: 0, y: 0 },
          handle_in: null,
          handle_out: null,
          node_type: 'corner',
        },
        {
          id: { subpath_idx: 0, command_idx: 1 },
          position: { x: 10.25, y: 0 },
          handle_in: null,
          handle_out: null,
          node_type: 'corner',
        },
      ],
    }, {
      closed: false,
      nodes: [
        {
          id: { subpath_idx: 1, command_idx: 0 },
          position: { x: 10, y: 0 },
          handle_in: null,
          handle_out: null,
          node_type: 'corner',
        },
        {
          id: { subpath_idx: 1, command_idx: 1 },
          position: { x: 20, y: 0 },
          handle_in: null,
          handle_out: null,
          node_type: 'corner',
        },
      ],
    }];
    const dragTarget: NodeSelectionTarget = {
      kind: 'node',
      nodeId: { subpath_idx: 0, command_idx: 1 },
    };
    // @ts-expect-error private state setup
    tool.selectedTargets = [dragTarget];
    // @ts-expect-error private state setup
    tool.primaryTarget = dragTarget;
    // @ts-expect-error private state setup
    tool.state = {
      type: 'dragging',
      target: dragTarget,
      startWorld: { x: 10, y: 0 },
      initialPoints: [{ target: dragTarget, world: { x: 10, y: 0 } }],
      excludedPoints: [],
      preferredTargetKey: null,
      mirroredTarget: null,
    };
    // @ts-expect-error private state setup
    tool.joinTargetNodeId = { subpath_idx: 1, command_idx: 0 };

    const pendingUpdate = deferred<ProjectObject>();
    vi.mocked(vectorService.updateNodesBatch).mockReturnValueOnce(pendingUpdate.promise);
    vi.mocked(vectorService.joinSubpaths).mockResolvedValue(obj);

    tool.onMouseUp(makeMouseEvent({ screenX: 1200, screenY: 200, worldX: 10, worldY: 0 }), ctx);

    expect(vectorService.updateNodesBatch).toHaveBeenCalledWith('path1', [
      {
        node_id: { subpath_idx: 0, command_idx: 1 },
        x: 10.25,
        y: 0,
        handle_type: null,
      },
    ]);
    expect(vectorService.joinSubpaths).not.toHaveBeenCalled();

    pendingUpdate.resolve(obj);

    await vi.waitFor(() => {
      expect(vectorService.joinSubpaths).toHaveBeenCalledWith(
        'path1',
        { subpath_idx: 0, command_idx: 1 },
        { subpath_idx: 1, command_idx: 0 },
      );
    });
  });
});
