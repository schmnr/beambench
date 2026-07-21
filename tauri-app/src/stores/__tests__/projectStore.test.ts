import { describe, it, expect, vi, beforeEach } from 'vitest';
import { useProjectStore } from '../projectStore';
import { useNotificationStore } from '../notificationStore';

const previewInvalidate = vi.fn();
const previewClear = vi.fn();
const undoRefresh = vi.fn().mockResolvedValue(undefined);
const undoClear = vi.fn();

vi.mock('../../services/projectService', () => ({
  projectService: {
    getProject: vi.fn(),
    createProject: vi.fn(),
    closeProject: vi.fn(),
    addLayer: vi.fn(),
    updateLayer: vi.fn(),
    removeLayer: vi.fn(),
    reorderLayer: vi.fn(),
    addObject: vi.fn(),
    addObjectAtomic: vi.fn(),
    updateObject: vi.fn(),
    updateObjectData: vi.fn(),
    resizeShapeObject: vi.fn(),
    removeObject: vi.fn(),
    removeObjects: vi.fn(),
    nudgeObjects: vi.fn(),
    duplicateObject: vi.fn(),
    duplicateObjects: vi.fn(),
    duplicateObjectInPlace: vi.fn(),
    duplicateObjectsInPlace: vi.fn(),
    pasteObjects: vi.fn(),
    bindMachineProfile: vi.fn(),
    getUndoState: vi.fn().mockResolvedValue({ can_undo: false, can_redo: false }),
    lockObjects: vi.fn(),
    unlockObjects: vi.fn(),
    flipObjects: vi.fn(),
    rotateObjects: vi.fn(),
    rotateObjectsAndBakeActivePath: vi.fn(),
    shearObjects: vi.fn(),
    updateObjectBoundsBatch: vi.fn(),
    pushDrawOrder: vi.fn(),
    moveObjectsTo: vi.fn(),
    mirrorAcrossLine: vi.fn(),
    makeSameSize: vi.fn(),
    moveObjectsTogether: vi.fn(),
    dockObjects: vi.fn(),
    resizeSlots: vi.fn(),
    reassignLayer: vi.fn(),
    countDuplicates: vi.fn(),
    deleteDuplicates: vi.fn(),
    autoJoinShapes: vi.fn(),
    optimizeShapes: vi.fn(),
    setStartFrom: vi.fn(),
    setJobOrigin: vi.fn(),
    setOptimization: vi.fn(),
    updateProjectNotes: vi.fn(),
    setTransformLocks: vi.fn(),
    setObjectsVisible: vi.fn(),
  },
}));

vi.mock('../../services/vectorService', () => ({
  vectorService: {
    convertToPath: vi.fn(),
    unlinkVirtualClone: vi.fn(),
    booleanUnion: vi.fn(),
    booleanSubtract: vi.fn(),
    booleanExclude: vi.fn(),
    booleanIntersection: vi.fn(),
    booleanWeld: vi.fn(),
    groupObjects: vi.fn(),
    autoGroupObjects: vi.fn(),
    ungroupObjects: vi.fn(),
    closeAndJoin: vi.fn(),
    offsetShapes: vi.fn(),
    breakApart: vi.fn(),
    closePath: vi.fn(),
    gridArray: vi.fn(),
    circularArray: vi.fn(),
    copyAlongPathBatch: vi.fn(),
    rubberBandOutline: vi.fn(),
    applyPathToText: vi.fn(),
    cropImage: vi.fn(),
    applyMaskToImage: vi.fn(),
    cutShapesApply: vi.fn(),
    convertToBitmap: vi.fn(),
    addTabs: vi.fn(),
    applyRadius: vi.fn(),
  },
}));

vi.mock('../../services/importService', () => ({
  importService: {
    pickFiles: vi.fn(),
    importFilePaths: vi.fn(),
    importGcodeFile: vi.fn(),
    refreshImage: vi.fn(),
    replaceImage: vi.fn(),
  },
}));

vi.mock('../../services/persistenceService', () => ({
  persistenceService: {
    saveProject: vi.fn(),
    saveProjectAs: vi.fn(),
    openProject: vi.fn(),
    openProjectFromPath: vi.fn(),
    getAssetData: vi.fn(),
  },
}));

vi.mock('../../services/previewService', () => ({
  previewService: {
    exportGcode: vi.fn(),
    cancelPlanning: vi.fn(),
  },
}));

vi.mock('../previewStore', () => ({
  usePreviewStore: {
    getState: () => ({
      invalidate: previewInvalidate,
      clearPreview: previewClear,
    }),
  },
}));

vi.mock('../undoStore', () => ({
  useUndoStore: {
    getState: () => ({
      refresh: undoRefresh,
      clear: undoClear,
    }),
  },
}));

import { projectService } from '../../services/projectService';
import { vectorService } from '../../services/vectorService';
import { importService } from '../../services/importService';
import { persistenceService } from '../../services/persistenceService';
import { previewService } from '../../services/previewService';
import type { ObjectData, Project, ProjectOptimization } from '../../types/project';
import { DEFAULT_PROJECT_OPTIMIZATION } from '../../types/project';
import { makeLayer, makeProject as makeProjectFixture, makeProjectObject } from '../../test-utils/projectFixtures';

const mockedProject = projectService as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockedVector = vectorService as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockedImport = importService as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockedPersistence = persistenceService as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockedPreviewService = previewService as unknown as Record<string, ReturnType<typeof vi.fn>>;

function mockUrlApi() {
  const originalCreate = URL.createObjectURL;
  const originalRevoke = URL.revokeObjectURL;
  const createObjectURL = vi.fn((blob: Blob) => `blob:${blob.size}`);
  const revokeObjectURL = vi.fn();

  Object.defineProperty(URL, 'createObjectURL', {
    configurable: true,
    writable: true,
    value: createObjectURL,
  });
  Object.defineProperty(URL, 'revokeObjectURL', {
    configurable: true,
    writable: true,
    value: revokeObjectURL,
  });

  return {
    createObjectURL,
    revokeObjectURL,
    restore() {
      Object.defineProperty(URL, 'createObjectURL', {
        configurable: true,
        writable: true,
        value: originalCreate,
      });
      Object.defineProperty(URL, 'revokeObjectURL', {
        configurable: true,
        writable: true,
        value: originalRevoke,
      });
    },
  };
}

const makeProject = (overrides: Partial<Project> = {}): Project => ({
  ...makeProjectFixture({
    metadata: {
      format_version: '1.0',
      app_version: '0.1.0',
      project_id: 'test-id',
      project_name: 'Test',
      created_at: '2026-01-01',
      modified_at: '2026-01-01',
    },
    workspace: { bed_width_mm: 400, bed_height_mm: 400, origin: 'top_left' as const },
    assets: [],
  }),
  layers: [
    makeLayer({
      id: 'layer1',
      name: 'L1',
      operation: 'line',
      color_tag: '#ff0000',
      speed_mm_min: 600,
      power_percent: 80,
    }),
  ],
  objects: [
    makeProjectObject({
      id: 'obj1',
      name: 'R1',
      layer_id: 'layer1',
      data: {
        type: 'shape' as const,
        kind: 'rectangle' as const,
        width: 10,
        height: 10,
        corner_radius: 0,
      },
    }),
    makeProjectObject({
      id: 'obj2',
      name: 'R2',
      bounds: { min: { x: 20, y: 20 }, max: { x: 30, y: 30 } },
      layer_id: 'layer1',
      z_index: 1,
      data: {
        type: 'shape' as const,
        kind: 'rectangle' as const,
        width: 10,
        height: 10,
        corner_radius: 0,
      },
      created_at: '2026-01-01T00:00:01Z',
    }),
  ],
  ...overrides,
});

describe('projectStore — new actions', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    previewInvalidate.mockClear();
    previewClear.mockClear();
    undoRefresh.mockClear();
    undoClear.mockClear();
    mockedImport.pickFiles.mockReset();
    mockedImport.importFilePaths.mockReset();
    mockedImport.importGcodeFile.mockReset();
    mockedPreviewService.exportGcode.mockReset();
    useProjectStore.setState({
      project: makeProject(),
      projectPath: null,
      selectedLayerId: 'layer1',
      selectedObjectIds: ['obj1'],
      assetCache: new Map(),
      assetLoadErrors: new Map(),
      loading: false,
      error: null,
    });
    useNotificationStore.setState({ notifications: [] });
  });

  it('preserves ordered selection and moves deselect/reselect to the anchor end', () => {
    useProjectStore.getState().selectObjects(['obj1']);
    useProjectStore.getState().toggleObjectSelection('obj2');
    expect(useProjectStore.getState().selectedObjectIds).toEqual(['obj1', 'obj2']);

    useProjectStore.getState().toggleObjectSelection('obj1');
    expect(useProjectStore.getState().selectedObjectIds).toEqual(['obj2']);

    useProjectStore.getState().toggleObjectSelection('obj1');
    expect(useProjectStore.getState().selectedObjectIds).toEqual(['obj2', 'obj1']);
  });

  it('preserves add-order when selectObjects receives a replacement set', () => {
    useProjectStore.getState().selectObjects(['obj1', 'obj2']);
    useProjectStore.getState().selectObjects(['obj2']);
    useProjectStore.getState().selectObjects(['obj1', 'obj2']);

    expect(useProjectStore.getState().selectedObjectIds).toEqual(['obj2', 'obj1']);
  });

  it('selectObjects keeps tool-layer objects (regression: clicking T1/T2 object cleared selection)', () => {
    const project = makeProject({
      layers: [
        makeLayer({ id: 'layer1', is_tool_layer: false, color_tag: '#ff0000' }),
        makeLayer({ id: 'tool', is_tool_layer: true, color_tag: '#DA0B3F' }),
      ],
      objects: [
        makeProjectObject({ id: 'obj1', layer_id: 'layer1' }),
        makeProjectObject({ id: 'tool-obj', layer_id: 'tool' }),
      ],
    });
    useProjectStore.setState({ project, selectedObjectIds: [] });

    useProjectStore.getState().selectObjects(['tool-obj']);
    expect(useProjectStore.getState().selectedObjectIds).toEqual(['tool-obj']);

    useProjectStore.getState().toggleObjectSelection('obj1');
    expect(useProjectStore.getState().selectedObjectIds).toEqual(['tool-obj', 'obj1']);
  });

  it('selectAllObjects orders a multi-add batch so the first draw-order object is the anchor', () => {
    useProjectStore.setState({ selectedObjectIds: [] });
    useProjectStore.getState().selectAllObjects();

    expect(useProjectStore.getState().selectedObjectIds).toEqual(['obj2', 'obj1']);
  });

  it('stores top-level group ids when selecting grouped children', () => {
    const project = makeProject();
    const group = makeProjectObject({
      id: 'group1',
      name: 'Group',
      layer_id: 'layer1',
      data: { type: 'group', children: ['obj1'] },
    });
    useProjectStore.setState({ project: { ...project, objects: [...project.objects, group] }, selectedObjectIds: [] });

    useProjectStore.getState().selectObjects(['obj1']);
    expect(useProjectStore.getState().selectedObjectIds).toEqual(['group1']);

    useProjectStore.getState().toggleObjectSelection('obj1');
    expect(useProjectStore.getState().selectedObjectIds).toEqual([]);

    useProjectStore.getState().toggleObjectSelection('obj1');
    expect(useProjectStore.getState().selectedObjectIds).toEqual(['group1']);
  });

  it('selectAllObjects skips group children and selects top-level roots only', () => {
    const project = makeProject();
    const group = makeProjectObject({
      id: 'group1',
      name: 'Group',
      layer_id: 'layer1',
      data: { type: 'group', children: ['obj1'] },
    });
    useProjectStore.setState({ project: { ...project, objects: [...project.objects, group] }, selectedObjectIds: [] });

    useProjectStore.getState().selectAllObjects();

    expect(useProjectStore.getState().selectedObjectIds).toEqual(['group1', 'obj2']);
  });

  it('nudgeObjects expands a selected group to its children', async () => {
    const project = makeProject();
    const group = makeProjectObject({
      id: 'group1',
      name: 'Group',
      layer_id: 'layer1',
      bounds: { min: { x: 0, y: 0 }, max: { x: 30, y: 30 } },
      data: { type: 'group', children: ['obj1', 'obj2'] },
    });
    useProjectStore.setState({ project: { ...project, objects: [...project.objects, group] } });
    mockedProject.nudgeObjects.mockResolvedValue(undefined);

    await useProjectStore.getState().nudgeObjects(['group1'], 5, 0);

    expect(mockedProject.nudgeObjects).toHaveBeenCalledWith(['group1', 'obj1', 'obj2'], 5, 0);
    const moved = useProjectStore.getState().project!;
    expect(moved.objects.find((object) => object.id === 'group1')?.bounds.min.x).toBe(5);
    expect(moved.objects.find((object) => object.id === 'obj1')?.bounds.min.x).toBe(5);
    expect(moved.objects.find((object) => object.id === 'obj2')?.bounds.min.x).toBe(25);
  });

  it('removeObjects expands a selected group to its children', async () => {
    const project = makeProject();
    const group = makeProjectObject({
      id: 'group1',
      name: 'Group',
      layer_id: 'layer1',
      bounds: { min: { x: 0, y: 0 }, max: { x: 30, y: 30 } },
      data: { type: 'group', children: ['obj1', 'obj2'] },
    });
    const reloaded = { ...project, objects: [] };
    useProjectStore.setState({
      project: { ...project, objects: [...project.objects, group] },
      selectedObjectIds: ['group1'],
    });
    mockedProject.removeObjects.mockResolvedValue(3);
    mockedProject.getProject.mockResolvedValue(reloaded);

    await useProjectStore.getState().removeObjects(['group1']);

    expect(mockedProject.removeObjects).toHaveBeenCalledWith(['group1', 'obj1', 'obj2']);
    expect(useProjectStore.getState().selectedObjectIds).toEqual([]);
  });

  it('removeObjects can delete a selected ruler guide on a tool layer', async () => {
    const guide = makeProjectObject({
      id: 'guide1',
      name: 'Guide',
      layer_id: 'tool',
      bounds: { min: { x: 10, y: 0 }, max: { x: 10, y: 400 } },
      data: {
        type: 'vector_path',
        path_data: 'M 10 0 L 10 400',
        closed: false,
        ruler_guide_axis: 'vertical',
      },
    });
    const project = makeProject({
      layers: [
        makeLayer({ id: 'layer1', is_tool_layer: false, color_tag: '#ff0000' }),
        makeLayer({ id: 'tool', is_tool_layer: true, color_tag: '#DA0B3F' }),
      ],
      objects: [guide],
    });
    useProjectStore.setState({ project, selectedObjectIds: ['guide1'] });
    mockedProject.removeObjects.mockResolvedValue(1);
    mockedProject.getProject.mockResolvedValue({ ...project, objects: [] });

    await useProjectStore.getState().removeObjects(['guide1']);

    expect(mockedProject.removeObjects).toHaveBeenCalledWith(['guide1']);
    expect(useProjectStore.getState().selectedObjectIds).toEqual([]);
  });

  it('moveObjectsTo translates a selected group without collapsing child spacing', async () => {
    const project = makeProject();
    const group = makeProjectObject({
      id: 'group1',
      name: 'Group',
      layer_id: 'layer1',
      bounds: { min: { x: 0, y: 0 }, max: { x: 30, y: 30 } },
      data: { type: 'group', children: ['obj1', 'obj2'] },
    });
    const groupedProject = { ...project, objects: [...project.objects, group] };
    useProjectStore.setState({ project: groupedProject });
    mockedProject.updateObjectBoundsBatch.mockResolvedValue(undefined);
    mockedProject.getProject.mockResolvedValue(groupedProject);

    await useProjectStore.getState().moveObjectsTo(['group1'], 100, 100);

    expect(mockedProject.updateObjectBoundsBatch).toHaveBeenCalledWith([
      {
        id: 'group1',
        bounds: { min: { x: 100, y: 100 }, max: { x: 130, y: 130 } },
      },
      {
        id: 'obj1',
        bounds: { min: { x: 100, y: 100 }, max: { x: 110, y: 110 } },
      },
      {
        id: 'obj2',
        bounds: { min: { x: 120, y: 120 }, max: { x: 130, y: 130 } },
      },
    ]);
  });

  it('closeProject revokes cached raster blob URLs', async () => {
    const revokeObjectURL = vi.fn();
    const originalRevoke = URL.revokeObjectURL;
    URL.revokeObjectURL = revokeObjectURL;
    mockedProject.closeProject.mockResolvedValue(undefined);

    useProjectStore.setState({
      assetCache: new Map([
        ['asset-a', 'blob:asset-a'],
        ['asset-b', 'blob:asset-b'],
      ]),
    });

    await useProjectStore.getState().closeProject();

    expect(revokeObjectURL).toHaveBeenCalledWith('blob:asset-a');
    expect(revokeObjectURL).toHaveBeenCalledWith('blob:asset-b');
    expect(useProjectStore.getState().assetCache.size).toBe(0);

    URL.revokeObjectURL = originalRevoke;
  });

  it('replaceImage evicts the stale cached blob for the raster asset before reloading', async () => {
    const revokeObjectURL = vi.fn();
    const originalRevoke = URL.revokeObjectURL;
    URL.revokeObjectURL = revokeObjectURL;
    const project = makeProject();
    project.objects = [
      {
        id: 'img1',
        name: 'Image',
        visible: true,
        locked: false,
        transform: { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 },
        bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } },
        layer_id: 'layer1',
        z_index: 0,
        data: {
          type: 'raster_image',
          asset_key: 'asset-1',
          original_width_px: 10,
          original_height_px: 10,
        },
      } as never,
    ];
    mockedImport.replaceImage.mockResolvedValue(true);
    mockedProject.getProject.mockResolvedValue(project);
    useProjectStore.setState({
      project,
      assetCache: new Map([['asset-1', 'blob:asset-1']]),
    });

    await useProjectStore.getState().replaceImage('img1');

    expect(revokeObjectURL).toHaveBeenCalledWith('blob:asset-1');
    expect(useProjectStore.getState().assetCache.size).toBe(0);

    URL.revokeObjectURL = originalRevoke;
  });

  it('lockObjects calls service and reloads project', async () => {
    const reloaded = makeProject();
    reloaded.objects[0].locked = true;
    mockedProject.lockObjects.mockResolvedValue(reloaded.objects);
    mockedProject.getProject.mockResolvedValue(reloaded);

    await useProjectStore.getState().lockObjects(['obj1']);

    expect(mockedProject.lockObjects).toHaveBeenCalledWith(['obj1']);
    expect(mockedProject.getProject).toHaveBeenCalled();
    expect(useProjectStore.getState().project?.dirty).toBe(true);
  });

  it('setObjectsVisible calls batch service and reloads project', async () => {
    const reloaded = makeProject();
    reloaded.objects[0].visible = false;
    mockedProject.setObjectsVisible.mockResolvedValue(undefined);
    mockedProject.getProject.mockResolvedValue(reloaded);

    await useProjectStore.getState().setObjectsVisible(['obj1'], false);

    expect(mockedProject.setObjectsVisible).toHaveBeenCalledWith(['obj1'], false);
    expect(mockedProject.getProject).toHaveBeenCalled();
    expect(useProjectStore.getState().project?.objects[0].visible).toBe(false);
  });

  it('updateObjectBoundsBatch refetches committed path data and bounds on success', async () => {
    mockedProject.updateObjectBoundsBatch.mockResolvedValue(undefined);
    const project = makeProject();
    const optimisticPathData = 'M 0 0 L 10 0 L 10 10 Z';
    const committedPathData = 'M 50 60 L 80 60 L 80 90 Z';
    const updated = {
      ...project.objects[0],
      bounds: { min: { x: 50, y: 60 }, max: { x: 80, y: 90 } },
      data: { type: 'vector_path' as const, path_data: optimisticPathData, closed: true },
    };
    const committed = {
      ...updated,
      data: { type: 'vector_path' as const, path_data: committedPathData, closed: true },
    };
    mockedProject.getProject.mockResolvedValue({
      ...project,
      objects: [committed, project.objects[1]],
      dirty: false,
    });
    useProjectStore.setState({
      project: {
        ...project,
        objects: [updated, project.objects[1]],
        dirty: false,
      },
    });

    await useProjectStore.getState().updateObjectBoundsBatch([
      { id: 'obj1', bounds: updated.bounds },
    ]);

    expect(mockedProject.updateObjectBoundsBatch).toHaveBeenCalledWith([
      { id: 'obj1', bounds: updated.bounds },
    ]);
    expect(mockedProject.getProject).toHaveBeenCalled();
    expect(useProjectStore.getState().project?.objects[0].bounds).toEqual(updated.bounds);
    expect(useProjectStore.getState().project?.objects[0].data).toMatchObject({
      type: 'vector_path',
      path_data: committedPathData,
    });
    expect(useProjectStore.getState().project?.dirty).toBe(true);
    expect(previewInvalidate).toHaveBeenCalledOnce();
    expect(undoRefresh).toHaveBeenCalledOnce();
  });

  it('dockObjects refuses locked selections before calling the service', async () => {
    useProjectStore.setState({
      project: makeProject({
        objects: [makeProjectObject({ id: 'obj-1', locked: true })],
      }),
      selectedObjectIds: ['obj-1'],
    });

    const applied = await useProjectStore.getState().dockObjects(['obj-1'], 'left', {
      moveAsGroup: false,
      lockInnerObjects: false,
      paddingMm: 0,
    });

    expect(applied).toBe(false);
    expect(mockedProject.dockObjects).not.toHaveBeenCalled();
    const notifications = useNotificationStore.getState().notifications;
    expect(notifications).toHaveLength(1);
    expect(notifications[0].type).toBe('warning');
    expect(notifications[0].message).toContain('Object is locked');
  });

  it('mirrorAcrossLine uses the last selected vector path as the axis and selects duplicates', async () => {
    mockedProject.mirrorAcrossLine.mockResolvedValue([
      makeProjectObject({ id: 'dup-1' }),
      makeProjectObject({ id: 'dup-2' }),
    ]);
    useProjectStore.setState({
      project: makeProject({
        objects: [
          makeProjectObject({ id: 'shape-1', data: { type: 'shape', kind: 'rectangle', width: 10, height: 10, corner_radius: 0 } }),
          makeProjectObject({ id: 'axis-1', data: { type: 'vector_path', path_data: 'M 0 0 L 10 0', closed: false, ruler_guide_axis: null } }),
          makeProjectObject({ id: 'axis-2', data: { type: 'vector_path', path_data: 'M 0 0 L 0 10', closed: false, ruler_guide_axis: null } }),
        ],
      }),
      selectedObjectIds: ['shape-1', 'axis-1', 'axis-2'],
    });

    await useProjectStore.getState().mirrorAcrossLine();

    expect(mockedProject.mirrorAcrossLine).toHaveBeenCalledWith(
      ['shape-1', 'axis-1', 'axis-2'],
      'axis-2',
    );
    expect(useProjectStore.getState().selectedObjectIds).toEqual(['dup-1', 'dup-2']);
  });

  it('mirrorAcrossLine skips ineligible vector paths and picks the last eligible line axis', async () => {
    mockedProject.mirrorAcrossLine.mockResolvedValue([
      makeProjectObject({ id: 'dup-1' }),
    ]);
    useProjectStore.setState({
      project: makeProject({
        objects: [
          makeProjectObject({ id: 'shape-1', data: { type: 'shape', kind: 'rectangle', width: 10, height: 10, corner_radius: 0 } }),
          makeProjectObject({
            id: 'line-axis',
            data: { type: 'vector_path', path_data: 'M 0 0 L 10 0', closed: false, ruler_guide_axis: null },
          }),
          makeProjectObject({
            id: 'square-path',
            data: { type: 'vector_path', path_data: 'M 0 0 L 10 0 L 10 10 L 0 10 Z', closed: true, ruler_guide_axis: null },
          }),
        ],
      }),
      selectedObjectIds: ['shape-1', 'line-axis', 'square-path'],
    });

    await useProjectStore.getState().mirrorAcrossLine();

    expect(mockedProject.mirrorAcrossLine).toHaveBeenCalledWith(
      ['shape-1', 'line-axis', 'square-path'],
      'line-axis',
    );
  });

  it('mirrorAcrossLine accepts a straight cubic segment from the draw tool as the axis', async () => {
    mockedProject.mirrorAcrossLine.mockResolvedValue([
      makeProjectObject({ id: 'dup-1' }),
    ]);
    useProjectStore.setState({
      project: makeProject({
        objects: [
          makeProjectObject({ id: 'shape-1', data: { type: 'shape', kind: 'rectangle', width: 10, height: 10, corner_radius: 0 } }),
          makeProjectObject({
            id: 'drawn-axis',
            bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 0 } },
            data: {
              type: 'vector_path',
              path_data: 'M 0 0 C 0 0 10 0 10 0',
              closed: false,
              ruler_guide_axis: null,
            },
          }),
        ],
      }),
      selectedObjectIds: ['shape-1', 'drawn-axis'],
    });

    await useProjectStore.getState().mirrorAcrossLine();

    expect(mockedProject.mirrorAcrossLine).toHaveBeenCalledWith(
      ['shape-1', 'drawn-axis'],
      'drawn-axis',
    );
  });

  it('mirrorAcrossLine allows a tool-layer straight line as the axis without mirroring it', async () => {
    mockedProject.mirrorAcrossLine.mockResolvedValue([
      makeProjectObject({ id: 'dup-1' }),
    ]);
    useProjectStore.setState({
      project: makeProject({
        layers: [
          makeLayer({ id: 'tool-layer', is_tool_layer: true }),
          makeLayer({ id: 'normal-layer' }),
        ],
        objects: [
          makeProjectObject({ id: 'shape-1', layer_id: 'normal-layer' }),
          makeProjectObject({
            id: 'tool-axis',
            layer_id: 'tool-layer',
            data: { type: 'vector_path', path_data: 'M 0 0 L 10 0', closed: false, ruler_guide_axis: null },
          }),
        ],
      }),
      selectedObjectIds: ['shape-1', 'tool-axis'],
    });

    await useProjectStore.getState().mirrorAcrossLine();

    expect(mockedProject.mirrorAcrossLine).toHaveBeenCalledWith(
      ['shape-1', 'tool-axis'],
      'tool-axis',
    );
  });

  it('mirrorAcrossLine normalizes selection and selects only top-level duplicated groups', async () => {
    mockedProject.mirrorAcrossLine.mockResolvedValue([
      makeProjectObject({
        id: 'dup-group',
        data: { type: 'group', children: ['dup-child'] },
      }),
      makeProjectObject({ id: 'dup-child' }),
    ]);
    useProjectStore.setState({
      project: makeProject({
        layers: [
          makeLayer({ id: 'tool-layer', is_tool_layer: true }),
          makeLayer({ id: 'normal-layer' }),
        ],
        objects: [
          makeProjectObject({ id: 'shape-1', layer_id: 'normal-layer' }),
          makeProjectObject({
            id: 'guide-1',
            layer_id: 'tool-layer',
            data: { type: 'vector_path', path_data: 'M 0 0 L 10 0', closed: false, ruler_guide_axis: 'horizontal' },
          }),
          makeProjectObject({
            id: 'axis-1',
            layer_id: 'normal-layer',
            data: { type: 'vector_path', path_data: 'M 0 0 L 10 0', closed: false, ruler_guide_axis: null },
          }),
        ],
      }),
      selectedObjectIds: ['shape-1', 'guide-1', 'axis-1'],
    });

    await useProjectStore.getState().mirrorAcrossLine();

    expect(mockedProject.mirrorAcrossLine).toHaveBeenCalledWith(
      ['shape-1', 'axis-1'],
      'axis-1',
    );
    expect(useProjectStore.getState().selectedObjectIds).toEqual(['dup-group']);
  });

  it('makeSameSize normalizes selection and forwards the last normalized id as the anchor', async () => {
    mockedProject.makeSameSize.mockResolvedValue([
      makeProjectObject({ id: 'obj-a', bounds: { min: { x: 0, y: 0 }, max: { x: 20, y: 10 } } }),
    ]);
    useProjectStore.setState({
      project: makeProject({
        objects: [
          makeProjectObject({ id: 'obj-a' }),
          makeProjectObject({ id: 'obj-b' }),
        ],
      }),
      selectedObjectIds: ['obj-a', 'obj-b'],
    });

    await useProjectStore.getState().makeSameSize('width', true);

    expect(mockedProject.makeSameSize).toHaveBeenCalledWith(
      ['obj-a', 'obj-b'],
      'obj-b',
      'width',
      true,
    );
  });

  it('resizeSlots refuses scale-locked projects before calling the service', async () => {
    useProjectStore.setState({
      project: makeProject({
        transform_locks: {
          move_enabled: true,
          size_enabled: false,
          rotate_enabled: true,
          shear_enabled: true,
        },
        objects: [makeProjectObject({ id: 'obj-1' })],
      }),
      selectedObjectIds: ['obj-1'],
    });

    const applied = await useProjectStore.getState().resizeSlots(['obj-1'], {
      currentThicknessMm: 3,
      newThicknessMm: 4,
      toleranceMm: 0,
    });

    expect(applied).toBe(false);
    expect(mockedProject.resizeSlots).not.toHaveBeenCalled();
  });

  it('exportGcode advances auto variable text after a successful export', async () => {
    mockedPreviewService.exportGcode.mockResolvedValue('/tmp/out.gcode');
    const advanceAutoVariableText = vi
      .spyOn(useProjectStore.getState(), 'advanceAutoVariableText')
      .mockResolvedValue(true);

    const path = await useProjectStore.getState().exportGcode();

    expect(path).toBe('/tmp/out.gcode');
    expect(mockedPreviewService.exportGcode).toHaveBeenCalledOnce();
    expect(advanceAutoVariableText).toHaveBeenCalledOnce();
    advanceAutoVariableText.mockRestore();
  });

  it('restoreRecoveredProject clears a stale projectPath', () => {
    useProjectStore.setState({ projectPath: '/tmp/other-project.lzrproj' });

    const recovered = makeProject();
    recovered.metadata.project_id = 'recovered-project';

    useProjectStore.getState().restoreRecoveredProject(recovered);

    expect(useProjectStore.getState().projectPath).toBeNull();
    expect(useProjectStore.getState().project?.metadata.project_id).toBe('recovered-project');
    expect(useProjectStore.getState().selectedObjectIds).toEqual([]);
  });

  it('updateLayer returns false when the backend update fails', async () => {
    mockedProject.updateLayer.mockRejectedValue('Rename failed');

    await expect(
      useProjectStore.getState().updateLayer('layer1', { name: 'Renamed Layer' }),
    ).resolves.toBe(false);

    expect(useProjectStore.getState().error).toBe('Rename failed');
  });

  it('loadAssetData caches blob URLs and revokes them when the project closes', async () => {
    const urlApi = mockUrlApi();
    mockedPersistence.getAssetData.mockResolvedValue([1, 2, 3]);
    mockedProject.closeProject.mockResolvedValue(undefined);

    try {
      const first = await useProjectStore.getState().loadAssetData('asset-1');
      const second = await useProjectStore.getState().loadAssetData('asset-1');

      expect(first).toBe('blob:3');
      expect(second).toBe('blob:3');
      expect(mockedPersistence.getAssetData).toHaveBeenCalledTimes(1);
      expect(urlApi.createObjectURL).toHaveBeenCalledTimes(1);

      await useProjectStore.getState().closeProject();

      expect(urlApi.revokeObjectURL).toHaveBeenCalledWith('blob:3');
      expect(useProjectStore.getState().assetCache.size).toBe(0);
      expect(useProjectStore.getState().assetLoadErrors.size).toBe(0);
    } finally {
      urlApi.restore();
    }
  });

  it('loadAssetData caches failures and avoids repeated backend retries', async () => {
    mockedPersistence.getAssetData.mockRejectedValue(new Error('missing asset'));

    await expect(useProjectStore.getState().loadAssetData('asset-missing')).rejects.toThrow(
      'missing asset',
    );
    await expect(useProjectStore.getState().loadAssetData('asset-missing')).rejects.toThrow(
      'missing asset',
    );

    expect(mockedPersistence.getAssetData).toHaveBeenCalledTimes(1);
    expect(useProjectStore.getState().assetLoadErrors.get('asset-missing')).toBe(
      'Error: missing asset',
    );
  });

  it('loadProject prunes stale asset URLs and cached asset errors', async () => {
    const urlApi = mockUrlApi();
    const reloaded = makeProject();
    reloaded.assets = [
      {
        id: 'asset-keep',
        original_filename: 'keep.png',
        media_type: 'png',
        byte_size: 10,
        width_px: 10,
        height_px: 10,
      },
    ];
    mockedProject.getProject.mockResolvedValue(reloaded);
    useProjectStore.setState({
      project: reloaded,
      assetCache: new Map([
        ['asset-keep', 'blob:keep'],
        ['asset-drop', 'blob:drop'],
      ]),
      assetLoadErrors: new Map([
        ['asset-keep', 'keep failed'],
        ['asset-drop', 'drop failed'],
      ]),
    });

    try {
      await useProjectStore.getState().loadProject();

      expect(urlApi.revokeObjectURL).toHaveBeenCalledWith('blob:drop');
      expect(useProjectStore.getState().assetCache).toEqual(new Map([['asset-keep', 'blob:keep']]));
      expect(useProjectStore.getState().assetLoadErrors).toEqual(
        new Map([['asset-keep', 'keep failed']]),
      );
    } finally {
      urlApi.restore();
    }
  });

  it('updateObjectData returns false when the backend update fails', async () => {
    mockedProject.updateObjectData.mockRejectedValue('Update failed');

    await expect(
      useProjectStore.getState().updateObjectData('obj1', makeProject().objects[0].data),
    ).resolves.toBe(false);

    expect(useProjectStore.getState().error).toBe('Update failed');
  });

  it('duplicateObjects appends all duplicates and selects the full duplicated set', async () => {
    mockedProject.duplicateObjects.mockResolvedValue([
      {
        ...makeProject().objects[0],
        id: 'dup1',
        name: 'R1 copy',
      },
      {
        ...makeProject().objects[1],
        id: 'dup2',
        name: 'R2 copy',
      },
    ]);
    useProjectStore.setState({ selectedObjectIds: ['obj1', 'obj2'] });

    await useProjectStore.getState().duplicateObjects(['obj1', 'obj2']);

    expect(mockedProject.duplicateObjects).toHaveBeenCalledWith(['obj1', 'obj2']);
    expect(useProjectStore.getState().selectedObjectIds).toEqual(['dup1', 'dup2']);
    expect(useProjectStore.getState().project?.objects.map((o) => o.id)).toEqual([
      'obj1',
      'obj2',
      'dup1',
      'dup2',
    ]);
  });

  it('duplicateObjects selects the duplicated group root when backend returns its subtree', async () => {
    const project = makeProject();
    const group = makeProjectObject({
      id: 'group1',
      name: 'Group',
      layer_id: 'layer1',
      bounds: { min: { x: 0, y: 0 }, max: { x: 30, y: 30 } },
      data: { type: 'group', children: ['obj1', 'obj2'] },
    });
    const dupChild1 = { ...project.objects[0], id: 'dup-child-1', name: 'R1 copy' };
    const dupChild2 = { ...project.objects[1], id: 'dup-child-2', name: 'R2 copy' };
    const dupGroup = {
      ...group,
      id: 'dup-group',
      name: 'Group copy',
      data: { type: 'group' as const, children: ['dup-child-1', 'dup-child-2'] },
    };
    useProjectStore.setState({ project: { ...project, objects: [...project.objects, group] } });
    mockedProject.duplicateObjects.mockResolvedValue([dupGroup, dupChild1, dupChild2]);

    await useProjectStore.getState().duplicateObjects(['group1']);

    expect(mockedProject.duplicateObjects).toHaveBeenCalledWith(['group1']);
    expect(useProjectStore.getState().selectedObjectIds).toEqual(['dup-group']);
  });

  it('pasteObjects selects the pasted group root when backend returns its subtree', async () => {
    const project = makeProject();
    const pastedChild1 = { ...project.objects[0], id: 'pasted-child-1', name: 'R1 copy' };
    const pastedChild2 = { ...project.objects[1], id: 'pasted-child-2', name: 'R2 copy' };
    const pastedGroup = makeProjectObject({
      id: 'pasted-group',
      name: 'Group copy',
      layer_id: 'layer1',
      bounds: { min: { x: 0, y: 0 }, max: { x: 30, y: 30 } },
      data: { type: 'group', children: ['pasted-child-1', 'pasted-child-2'] },
    });
    const nextProject = {
      ...project,
      objects: [...project.objects, pastedGroup, pastedChild1, pastedChild2],
    };
    mockedProject.pasteObjects.mockResolvedValue([pastedGroup, pastedChild1, pastedChild2]);
    mockedProject.getProject.mockResolvedValue(nextProject);

    await useProjectStore.getState().pasteObjects([pastedGroup, pastedChild1, pastedChild2], true);

    expect(useProjectStore.getState().selectedObjectIds).toEqual(['pasted-group']);
  });

  it('setStartFrom skips backend call when mode is unchanged', async () => {
    useProjectStore.setState({ project: { ...makeProject(), start_from: 'absolute_coords' } });

    await useProjectStore.getState().setStartFrom('absolute_coords');

    expect(mockedProject.setStartFrom).not.toHaveBeenCalled();
    expect(undoRefresh).not.toHaveBeenCalled();
  });

  it('setJobOrigin skips backend call when anchor is unchanged', async () => {
    useProjectStore.setState({ project: { ...makeProject(), job_origin: 'top_left' } });

    await useProjectStore.getState().setJobOrigin('top_left');

    expect(mockedProject.setJobOrigin).not.toHaveBeenCalled();
    expect(undoRefresh).not.toHaveBeenCalled();
  });

  it('flipObjects calls service and reloads', async () => {
    mockedProject.flipObjects.mockResolvedValue([]);
    mockedProject.getProject.mockResolvedValue(makeProject());

    await useProjectStore.getState().flipObjects(['obj1'], 'horizontal');

    expect(mockedProject.flipObjects).toHaveBeenCalledWith(['obj1'], 'horizontal');
    expect(mockedProject.getProject).toHaveBeenCalled();
  });

  it('flipObjects expands selected groups and mirrors around the group center', async () => {
    const project = makeProject();
    const group = makeProjectObject({
      id: 'group1',
      name: 'Group',
      layer_id: 'layer1',
      bounds: { min: { x: 0, y: 0 }, max: { x: 30, y: 30 } },
      data: { type: 'group', children: ['obj1', 'obj2'] },
    });
    const groupedProject = { ...project, objects: [...project.objects, group] };
    useProjectStore.setState({ project: groupedProject });
    mockedProject.flipObjects.mockResolvedValue(undefined);
    mockedProject.getProject.mockResolvedValue(groupedProject);

    await useProjectStore.getState().flipObjects(['group1'], 'horizontal');

    expect(mockedProject.flipObjects).toHaveBeenCalledWith(
      ['group1', 'obj1', 'obj2'],
      'horizontal',
      { x: 15, y: 15 },
    );
    expect(mockedProject.getProject).toHaveBeenCalled();
  });

  it('rotateObjects expands selected groups and rotates around the group center', async () => {
    const project = makeProject();
    const group = makeProjectObject({
      id: 'group1',
      name: 'Group',
      layer_id: 'layer1',
      bounds: { min: { x: 0, y: 0 }, max: { x: 30, y: 30 } },
      data: { type: 'group', children: ['obj1', 'obj2'] },
    });
    const groupedProject = { ...project, objects: [...project.objects, group] };
    useProjectStore.setState({ project: groupedProject });
    mockedProject.rotateObjects.mockResolvedValue(undefined);
    mockedProject.getProject.mockResolvedValue(groupedProject);

    await useProjectStore.getState().rotateObjects(['group1'], 90);

    expect(mockedProject.rotateObjects).toHaveBeenCalledWith(
      ['group1', 'obj1', 'obj2'],
      90,
      { x: 15, y: 15 },
    );
    expect(mockedProject.getProject).toHaveBeenCalled();
  });

  it('rotateObjectsAndBakeActivePath expands selected groups and preserves active object id', async () => {
    const project = makeProject();
    const group = makeProjectObject({
      id: 'group1',
      name: 'Group',
      layer_id: 'layer1',
      bounds: { min: { x: 0, y: 0 }, max: { x: 30, y: 30 } },
      data: { type: 'group', children: ['obj1', 'obj2'] },
    });
    const groupedProject = { ...project, objects: [...project.objects, group] };
    useProjectStore.setState({ project: groupedProject });
    mockedProject.rotateObjectsAndBakeActivePath.mockResolvedValue(groupedProject.objects[0]);
    mockedProject.getProject.mockResolvedValue(groupedProject);

    await useProjectStore
      .getState()
      .rotateObjectsAndBakeActivePath(['group1'], 90, undefined, 'obj1');

    expect(mockedProject.rotateObjectsAndBakeActivePath).toHaveBeenCalledWith(
      ['group1', 'obj1', 'obj2'],
      90,
      { x: 15, y: 15 },
      'obj1',
    );
    expect(mockedProject.getProject).toHaveBeenCalled();
  });

  it('unlinkVirtualClone updates local state and refreshes preview/undo', async () => {
    const updated = {
      ...makeProject().objects[0],
      id: 'obj1',
      data: {
        type: 'shape' as const,
        kind: 'ellipse' as const,
        width: 12,
        height: 12,
        corner_radius: 0,
      },
    };
    useProjectStore.setState({
      project: {
        ...makeProject(),
        objects: [
          {
            ...makeProject().objects[0],
            id: 'obj1',
            data: { type: 'virtual_clone', source_id: 'src1' } satisfies ObjectData,
          },
          makeProject().objects[1],
        ],
      },
    });
    mockedVector.unlinkVirtualClone.mockResolvedValue(updated);

    await useProjectStore.getState().unlinkVirtualClone('obj1');

    expect(mockedVector.unlinkVirtualClone).toHaveBeenCalledWith('obj1');
    expect(
      useProjectStore.getState().project?.objects.find((o) => o.id === 'obj1')?.data.type,
    ).toBe('shape');
    expect(useProjectStore.getState().project?.dirty).toBe(true);
    expect(previewInvalidate).toHaveBeenCalledOnce();
    expect(undoRefresh).toHaveBeenCalledOnce();
  });

  it('addObject appends the created object locally and selects it', async () => {
    const created = {
      id: 'obj3',
      name: 'Circle',
      visible: true,
      locked: false,
      transform: { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 },
      bounds: { min: { x: 40, y: 40 }, max: { x: 60, y: 60 } },
      layer_id: 'layer1',
      z_index: 2,
      data: {
        type: 'shape' as const,
        kind: 'ellipse' as const,
        width: 20,
        height: 20,
        corner_radius: 0,
      },
    };
    mockedProject.addObject.mockResolvedValue(created);

    await useProjectStore
      .getState()
      .addObject(
        'Circle',
        'layer1',
        { type: 'shape', kind: 'ellipse', width: 20, height: 20, corner_radius: 0 },
        { min: { x: 40, y: 40 }, max: { x: 60, y: 60 } },
      );

    const state = useProjectStore.getState();
    expect(mockedProject.addObject).toHaveBeenCalled();
    expect(state.project?.objects.find((o) => o.id === 'obj3')).toEqual(created);
    expect(state.selectedObjectIds).toEqual(['obj3']);
    expect(state.selectedLayerId).toBe('layer1');
  });

  it('addObject uses atomic layer creation when a non-raster sibling must be created', async () => {
    const project = makeProject();
    project.layers = [
      makeLayer({
        id: 'image-layer',
        name: 'C00 (Image)',
        operation: 'image',
        order_index: 0,
        color_tag: '#ff0000',
        speed_mm_min: 600,
        power_percent: 80,
      }),
    ];
    project.objects = [
      makeProjectObject({
        id: 'img1',
        name: 'Image',
        bounds: { min: { x: 0, y: 0 }, max: { x: 20, y: 20 } },
        layer_id: 'image-layer',
        data: {
          type: 'raster_image',
          asset_key: 'asset1',
          original_width_px: 100,
          original_height_px: 100,
        },
      }),
    ];
    useProjectStore.setState({
      project,
      selectedLayerId: 'image-layer',
      selectedObjectIds: [],
      pendingPaletteColor: null,
    });

    const createdLayer = makeLayer({
      id: 'line-layer',
      name: 'C00 (Line)',
      operation: 'line',
      order_index: 1,
      color_tag: '#ff0000',
      speed_mm_min: 600,
      power_percent: 80,
    });
    const createdObject = makeProjectObject({
      id: 'obj-star',
      name: 'Star',
      bounds: { min: { x: 40, y: 40 }, max: { x: 60, y: 60 } },
      layer_id: 'line-layer',
      z_index: 1,
      data: {
        type: 'star' as const,
        points: 5,
        bulge: 0,
        ratio: 0.5,
        dual_radius: false,
        ratio2: null,
        corner_radius: 0,
        corner_radii: [],
      },
    });
    mockedProject.addObjectAtomic.mockResolvedValue({
      object: createdObject,
      createdLayer,
    });

    await useProjectStore.getState().addObject(
      'Star',
      'image-layer',
      {
        type: 'star',
        points: 5,
        bulge: 0,
        ratio: 0.5,
        dual_radius: false,
        ratio2: null,
        corner_radius: 0,
        corner_radii: [],
      },
      { min: { x: 40, y: 40 }, max: { x: 60, y: 60 } },
    );

    const state = useProjectStore.getState();
    expect(mockedProject.addObjectAtomic).toHaveBeenCalled();
    expect(mockedProject.addObjectAtomic).toHaveBeenCalledWith(
      'Star',
      'image-layer',
      expect.any(Object),
      expect.any(Object),
      expect.objectContaining({
        name: 'C00 (Line)',
        operation: 'line',
      }),
    );
    expect(mockedProject.addLayer).not.toHaveBeenCalled();
    expect(state.project?.layers.find((l) => l.id === 'line-layer')).toEqual(createdLayer);
    expect(state.project?.objects.find((o) => o.id === 'obj-star')).toEqual(createdObject);
    expect(state.selectedLayerId).toBe('line-layer');
    expect(state.selectedObjectIds).toEqual(['obj-star']);
  });

  it('addObject seeds the first auto-created layer with the first standard palette color', async () => {
    useProjectStore.setState({
      project: { ...makeProject(), layers: [], objects: [] },
      selectedLayerId: null,
      selectedObjectIds: [],
      pendingPaletteColor: null,
    });

    const createdLayer = makeLayer({
      id: 'first-layer',
      name: 'Line',
      operation: 'line',
      order_index: 0,
      color_tag: '#000000',
    });
    const createdObject = {
      id: 'obj-first',
      name: 'Rect',
      visible: true,
      locked: false,
      transform: { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 },
      bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } },
      layer_id: 'first-layer',
      z_index: 0,
      data: {
        type: 'shape' as const,
        kind: 'rectangle' as const,
        width: 10,
        height: 10,
        corner_radius: 0,
      },
    };
    mockedProject.addObjectAtomic.mockResolvedValue({
      object: createdObject,
      createdLayer,
    });

    await useProjectStore
      .getState()
      .addObject(
        'Rect',
        '__auto__',
        { type: 'shape', kind: 'rectangle', width: 10, height: 10, corner_radius: 0 },
        { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } },
      );

    expect(mockedProject.addObjectAtomic).toHaveBeenCalledWith(
      'Rect',
      '00000000-0000-0000-0000-000000000000',
      expect.any(Object),
      expect.any(Object),
      expect.objectContaining({
        name: 'Line',
        operation: 'line',
        color_tag: '#000000',
      }),
    );
    expect(useProjectStore.getState().selectedLayerId).toBe('first-layer');
  });

  it('addRulerGuide writes full-span guide geometry that matches the workspace', async () => {
    const project = makeProject();
    project.layers = [
      makeLayer({
        id: 'tool-layer',
        name: 'T1',
        is_tool_layer: true,
        color_tag: '#DA0B3F',
      }),
    ];
    project.objects = [];
    useProjectStore.setState({
      project,
      selectedLayerId: 'tool-layer',
      selectedObjectIds: [],
    });

    const created = makeProjectObject({
      id: 'guide-1',
      name: 'Guide',
      layer_id: 'tool-layer',
      bounds: { min: { x: 25, y: 0 }, max: { x: 25, y: 400 } },
      data: {
        type: 'vector_path',
        path_data: 'M 25 0 L 25 400',
        closed: false,
        ruler_guide_axis: 'vertical',
      },
    });
    mockedProject.addObject.mockResolvedValue(created);

    await useProjectStore.getState().addRulerGuide('vertical', 25);

    expect(mockedProject.addObject).toHaveBeenCalledWith(
      'Guide',
      'tool-layer',
      expect.objectContaining({
        type: 'vector_path',
        path_data: 'M 25 0 L 25 400',
        ruler_guide_axis: 'vertical',
      }),
      {
        min: { x: 25, y: 0 },
        max: { x: 25, y: 400 },
      },
    );
  });

  it('addRulerGuide creates T1 directly when the project has no layers', async () => {
    const project = { ...makeProject(), layers: [], objects: [] };
    useProjectStore.setState({
      project,
      selectedLayerId: null,
      selectedObjectIds: [],
    });

    const createdBase = makeLayer({
      id: 'new-tool-layer',
      name: 'T1',
      operation: 'line',
      color_tag: '#000000',
      is_tool_layer: false,
    });
    const updatedToolLayer = makeLayer({
      ...createdBase,
      color_tag: '#DA0B3F',
      is_tool_layer: true,
    });
    const createdGuide = makeProjectObject({
      id: 'guide-1',
      name: 'Guide',
      layer_id: 'new-tool-layer',
      bounds: { min: { x: 0, y: 50 }, max: { x: 400, y: 50 } },
      data: {
        type: 'vector_path',
        path_data: 'M 0 50 L 400 50',
        closed: false,
        ruler_guide_axis: 'horizontal',
      },
    });
    mockedProject.addLayer.mockResolvedValue(createdBase);
    mockedProject.updateLayer.mockResolvedValue(updatedToolLayer);
    mockedProject.addObject.mockResolvedValue(createdGuide);

    await useProjectStore.getState().addRulerGuide('horizontal', 50);

    expect(mockedProject.addLayer).toHaveBeenCalledWith('T1', 'line');
    expect(mockedProject.updateLayer).toHaveBeenCalledWith('new-tool-layer', {
      color_tag: '#DA0B3F',
    });
    expect(mockedProject.addObjectAtomic).not.toHaveBeenCalled();
    expect(mockedProject.addObject).toHaveBeenCalledWith(
      'Guide',
      'new-tool-layer',
      expect.objectContaining({
        path_data: 'M 0 50 L 400 50',
        ruler_guide_axis: 'horizontal',
      }),
      {
        min: { x: 0, y: 50 },
        max: { x: 400, y: 50 },
      },
    );
  });

  it('createProject selects the first layer by default', async () => {
    const project = makeProject();
    mockedProject.createProject.mockResolvedValue(project);

    await useProjectStore.getState().createProject('Test');

    expect(useProjectStore.getState().selectedLayerId).toBe('layer1');
  });

  it('openProjectFromPath re-stretches persisted ruler guides to the current workspace', async () => {
    const project = makeProject();
    project.workspace = { bed_width_mm: 600, bed_height_mm: 300, origin: 'top_left' };
    project.objects = [
      makeProjectObject({
        id: 'guide-1',
        layer_id: 'layer1',
        bounds: { min: { x: 20, y: 0 }, max: { x: 20, y: 100 } },
        data: {
          type: 'vector_path',
          path_data: 'M 0 0 L 0 1',
          closed: false,
          ruler_guide_axis: 'vertical',
        },
      }),
    ];
    mockedPersistence.openProjectFromPath.mockResolvedValue(project);

    await useProjectStore.getState().openProjectFromPath('/tmp/test.lzrproj');

    const stored = useProjectStore.getState().project?.objects[0];
    expect(stored?.bounds).toEqual({
      min: { x: 20, y: 0 },
      max: { x: 20, y: 300 },
    });
    expect(stored?.data).toEqual(
      expect.objectContaining({
        type: 'vector_path',
        path_data: 'M 20 0 L 20 300',
        ruler_guide_axis: 'vertical',
      }),
    );
  });

  it('loadProject preserves selected layer when it still exists', async () => {
    const project = makeProject();
    useProjectStore.setState({ selectedLayerId: 'layer1', selectedObjectIds: ['obj1'] });
    mockedProject.getProject.mockResolvedValue(project);

    await useProjectStore.getState().loadProject();

    expect(useProjectStore.getState().selectedLayerId).toBe('layer1');
    expect(useProjectStore.getState().selectedObjectIds).toEqual(['obj1']);
  });

  it('loadProject can invalidate preview for mutation-driven reloads', async () => {
    mockedProject.getProject.mockResolvedValue(makeProject());

    await useProjectStore.getState().loadProject({ invalidatePreview: true });

    expect(previewInvalidate).toHaveBeenCalledOnce();
  });

  it('openProject selects the first layer in the opened project', async () => {
    const project = makeProject();
    mockedPersistence.openProject.mockResolvedValue({ project, path: '/tmp/test.bb' });

    await useProjectStore.getState().openProject();

    expect(useProjectStore.getState().selectedLayerId).toBe('layer1');
  });

  it('removeLayer falls back to the next remaining layer', async () => {
    const project = makeProject();
    project.layers.push({
      ...makeLayer({
        id: 'layer2',
        name: 'L2',
        operation: 'fill',
        order_index: 1,
        color_tag: '#00ff00',
        speed_mm_min: 500,
        power_percent: 40,
      }),
    });
    useProjectStore.setState({ project, selectedLayerId: 'layer1', selectedObjectIds: [] });
    mockedProject.removeLayer.mockResolvedValue(undefined);

    await useProjectStore.getState().removeLayer('layer1');

    expect(useProjectStore.getState().selectedLayerId).toBe('layer2');
  });

  it('importFilePaths ignores G-code paths now that only artwork import is wired', async () => {
    const project = makeProject();
    project.layers = [
      makeLayer({
        id: 'line-layer',
        name: 'C00 (Line)',
        operation: 'line',
        order_index: 0,
        color_tag: '#ff0000',
        speed_mm_min: 600,
        power_percent: 80,
      }),
      makeLayer({
        id: 'image-layer',
        name: 'C00 (Image)',
        operation: 'image',
        order_index: 1,
        color_tag: '#ff0000',
        speed_mm_min: 600,
        power_percent: 80,
      }),
    ];
    useProjectStore.setState({
      project,
      selectedLayerId: 'image-layer',
      selectedObjectIds: [],
      pendingPaletteColor: null,
    });

    mockedImport.importFilePaths.mockResolvedValue([
      {
        id: 'img-import',
        name: 'Photo',
        visible: true,
        locked: false,
        transform: { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 },
        bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } },
        layer_id: 'image-layer',
        z_index: 0,
        data: {
          type: 'raster_image',
          asset_key: 'a1',
          original_width_px: 10,
          original_height_px: 10,
        },
      },
    ]);
    mockedProject.addLayer.mockResolvedValue(project.layers[1]);
    mockedProject.updateLayer.mockResolvedValue(undefined);
    mockedProject.getProject.mockResolvedValue({
      ...project,
      objects: [
        {
          id: 'img-import',
          name: 'Photo',
          visible: true,
          locked: false,
          transform: { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 },
          bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } },
          layer_id: 'image-layer',
          z_index: 0,
          data: {
            type: 'raster_image',
            asset_key: 'a1',
            original_width_px: 10,
            original_height_px: 10,
          },
        },
      ],
    });

    await useProjectStore.getState().importFilePaths(['/tmp/photo.png', '/tmp/toolpath.gcode']);

    expect(mockedImport.importFilePaths).toHaveBeenCalledWith(['/tmp/photo.png'], 'image-layer');
    expect(mockedImport.importGcodeFile).not.toHaveBeenCalled();
    expect(useProjectStore.getState().selectedObjectIds).toEqual(['img-import']);
    expect(useProjectStore.getState().selectedLayerId).toBe('image-layer');
  });

  it('pushDrawOrder calls service and reloads', async () => {
    mockedProject.pushDrawOrder.mockResolvedValue(undefined);
    mockedProject.getProject.mockResolvedValue(makeProject());

    await useProjectStore.getState().pushDrawOrder('obj1', 'forward');

    expect(mockedProject.pushDrawOrder).toHaveBeenCalledWith('obj1', 'forward');
  });

  it('convertToPath retargets the active layer when the converted object is rerouted', async () => {
    const project = makeProject();
    project.layers = [
      makeLayer({
        id: 'image-layer',
        name: 'Image',
        operation: 'image',
        order_index: 0,
        color_tag: '#ff0000',
        speed_mm_min: 600,
        power_percent: 80,
      }),
      makeLayer({
        id: 'line-layer',
        name: 'Line',
        operation: 'line',
        order_index: 1,
        color_tag: '#ff0000',
        speed_mm_min: 600,
        power_percent: 80,
      }),
    ];
    project.objects = [
      {
        ...project.objects[0],
        layer_id: 'image-layer',
      },
    ];
    useProjectStore.setState({
      project,
      selectedLayerId: 'image-layer',
      selectedObjectIds: ['obj1'],
    });
    mockedVector.convertToPath.mockResolvedValue({
      ...project.objects[0],
      layer_id: 'line-layer',
      data: { type: 'vector_path', path_data: 'M0 0 L10 10', closed: false },
    });

    await useProjectStore.getState().convertToPath('obj1');

    expect(useProjectStore.getState().selectedLayerId).toBe('line-layer');
  });

  it('setStartFrom updates project directly', async () => {
    const updated = { ...makeProject(), start_from: 'user_origin' as const };
    mockedProject.setStartFrom.mockResolvedValue(updated);

    await useProjectStore.getState().setStartFrom('user_origin');

    expect(mockedProject.setStartFrom).toHaveBeenCalledWith('user_origin');
    expect(useProjectStore.getState().project?.dirty).toBe(true);
  });

  it('setJobOrigin updates project directly', async () => {
    mockedProject.setJobOrigin.mockResolvedValue(undefined);

    await useProjectStore.getState().setJobOrigin('center');

    expect(mockedProject.setJobOrigin).toHaveBeenCalledWith('center');
    expect(useProjectStore.getState().project?.job_origin).toBe('center');
  });

  it('booleanIntersection removes inputs and adds result', async () => {
    const newObj = {
      id: 'obj3',
      name: 'Intersect',
      visible: true,
      locked: false,
      transform: { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 },
      bounds: { min: { x: 5, y: 5 }, max: { x: 10, y: 10 } },
      layer_id: 'layer1',
      z_index: 0,
      data: { type: 'vector_path' as const, path_data: 'M5 5 L10 10', closed: true },
    };
    mockedVector.booleanIntersection.mockResolvedValue(newObj);
    mockedProject.getProject.mockResolvedValue({
      ...makeProject(),
      objects: [newObj],
    });

    await useProjectStore.getState().booleanIntersection('obj1', 'obj2');

    expect(mockedVector.booleanIntersection).toHaveBeenCalledWith('obj1', 'obj2');
    const state = useProjectStore.getState();
    expect(state.project?.objects.find((o) => o.id === 'obj1')).toBeUndefined();
    expect(state.project?.objects.find((o) => o.id === 'obj2')).toBeUndefined();
    expect(state.project?.objects.find((o) => o.id === 'obj3')).toBeDefined();
    expect(state.selectedObjectIds).toEqual(['obj3']);
  });

  it('booleanUnion sets dirty flag', async () => {
    const newObj = {
      id: 'obj3',
      name: 'Union',
      visible: true,
      locked: false,
      transform: { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 },
      bounds: { min: { x: 0, y: 0 }, max: { x: 30, y: 30 } },
      layer_id: 'layer1',
      z_index: 0,
      data: { type: 'vector_path' as const, path_data: 'M0 0 L30 30', closed: true },
    };
    useProjectStore.setState({ selectedObjectIds: ['obj1', 'obj2'] });
    mockedVector.booleanUnion.mockResolvedValue(newObj);
    mockedProject.getProject.mockResolvedValue({
      ...makeProject(),
      objects: [newObj],
    });

    await useProjectStore.getState().booleanUnion('obj1', 'obj2');

    expect(useProjectStore.getState().project?.dirty).toBe(true);
  });

  it('booleanUnion retargets the active layer to the result layer', async () => {
    const project = makeProject();
    project.layers.push({
      ...makeLayer({
        id: 'layer2',
        name: 'L2',
        operation: 'line',
        order_index: 1,
        color_tag: '#00ff00',
        speed_mm_min: 500,
      }),
    });
    useProjectStore.setState({
      project,
      selectedLayerId: 'layer1',
      selectedObjectIds: ['obj1', 'obj2'],
    });
    const newObj = {
      id: 'obj3',
      name: 'Union',
      visible: true,
      locked: false,
      transform: { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 },
      bounds: { min: { x: 0, y: 0 }, max: { x: 30, y: 30 } },
      layer_id: 'layer2',
      z_index: 0,
      data: { type: 'vector_path' as const, path_data: 'M0 0 L30 30', closed: true },
    };
    mockedVector.booleanUnion.mockResolvedValue(newObj);
    mockedProject.getProject.mockResolvedValue({
      ...project,
      objects: [newObj],
    });

    await useProjectStore.getState().booleanUnion('obj1', 'obj2');

    expect(useProjectStore.getState().selectedLayerId).toBe('layer2');
  });

  it('booleanSubtract sets dirty flag', async () => {
    const newObj = {
      id: 'obj3',
      name: 'Subtract',
      visible: true,
      locked: false,
      transform: { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 },
      bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } },
      layer_id: 'layer1',
      z_index: 0,
      data: { type: 'vector_path' as const, path_data: 'M0 0 L10 10', closed: true },
    };
    useProjectStore.setState({ selectedObjectIds: ['obj1', 'obj2'] });
    mockedVector.booleanSubtract.mockResolvedValue(newObj);
    mockedProject.getProject.mockResolvedValue({
      ...makeProject(),
      objects: [newObj],
    });

    await useProjectStore.getState().booleanSubtract('obj1', 'obj2');

    expect(useProjectStore.getState().project?.dirty).toBe(true);
  });

  it('booleanExclude sets dirty flag', async () => {
    const newObj = {
      id: 'obj3',
      name: 'Exclude',
      visible: true,
      locked: false,
      transform: { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 },
      bounds: { min: { x: 0, y: 0 }, max: { x: 30, y: 30 } },
      layer_id: 'layer1',
      z_index: 0,
      data: { type: 'vector_path' as const, path_data: 'M0 0 L30 30', closed: true },
    };
    useProjectStore.setState({ selectedObjectIds: ['obj1', 'obj2'] });
    mockedVector.booleanExclude.mockResolvedValue(newObj);
    mockedProject.getProject.mockResolvedValue({
      ...makeProject(),
      objects: [newObj],
    });

    await useProjectStore.getState().booleanExclude('obj1', 'obj2');

    expect(mockedVector.booleanExclude).toHaveBeenCalledWith('obj1', 'obj2');
    expect(useProjectStore.getState().project?.dirty).toBe(true);
    expect(useProjectStore.getState().selectedObjectIds).toEqual(['obj3']);
  });

  it('groupObjects sets dirty flag', async () => {
    const group = {
      id: 'grp1',
      name: 'Group',
      visible: true,
      locked: false,
      transform: { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 },
      bounds: { min: { x: 0, y: 0 }, max: { x: 30, y: 30 } },
      layer_id: 'layer1',
      z_index: 0,
      data: { type: 'group' as const, children: ['obj1', 'obj2'] },
    };
    useProjectStore.setState({ selectedObjectIds: ['obj1', 'obj2'] });
    mockedVector.groupObjects.mockResolvedValue(group);

    await useProjectStore.getState().groupObjects(['obj1', 'obj2']);

    expect(mockedVector.groupObjects).toHaveBeenCalledWith(['obj1', 'obj2']);
    expect(useProjectStore.getState().project?.dirty).toBe(true);
    expect(useProjectStore.getState().selectedObjectIds).toEqual(['grp1']);
  });

  it('groupObjects normalizes selected children to existing group roots', async () => {
    const project = makeProject();
    const existingGroup = makeProjectObject({
      id: 'group1',
      name: 'Existing Group',
      layer_id: 'layer1',
      data: { type: 'group', children: ['obj1'] },
    });
    const newGroup = makeProjectObject({
      id: 'grp2',
      name: 'Group',
      layer_id: 'layer1',
      data: { type: 'group', children: ['group1', 'obj2'] },
    });
    useProjectStore.setState({
      project: { ...project, objects: [...project.objects, existingGroup] },
      selectedObjectIds: ['obj1', 'group1', 'obj2'],
    });
    mockedVector.groupObjects.mockResolvedValue(newGroup);

    await useProjectStore.getState().groupObjects(['obj1', 'group1', 'obj2']);

    expect(mockedVector.groupObjects).toHaveBeenCalledWith(['group1', 'obj2']);
    expect(useProjectStore.getState().selectedObjectIds).toEqual(['grp2']);
  });

  it('groupObjects retargets the active layer to the created group layer', async () => {
    const group = {
      id: 'grp1',
      name: 'Group',
      visible: true,
      locked: false,
      transform: { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 },
      bounds: { min: { x: 0, y: 0 }, max: { x: 30, y: 30 } },
      layer_id: 'line-layer',
      z_index: 0,
      data: { type: 'group' as const, children: ['obj1', 'obj2'] },
    };
    mockedVector.groupObjects.mockResolvedValue(group);

    await useProjectStore.getState().groupObjects(['obj1', 'obj2']);

    expect(useProjectStore.getState().selectedLayerId).toBe('line-layer');
  });

  it('ungroupObjects sets dirty flag', async () => {
    mockedVector.ungroupObjects.mockResolvedValue(['obj1', 'obj2']);

    await useProjectStore.getState().ungroupObjects('obj1');

    expect(mockedVector.ungroupObjects).toHaveBeenCalledWith('obj1');
    expect(useProjectStore.getState().project?.dirty).toBe(true);
    expect(useProjectStore.getState().selectedObjectIds).toEqual(['obj1', 'obj2']);
  });

  it('ungroupObjects retargets the active layer to the surviving child layer', async () => {
    const project = makeProject();
    project.layers.push({
      ...makeLayer({
        id: 'layer2',
        name: 'L2',
        operation: 'line',
        order_index: 1,
        color_tag: '#00ff00',
        speed_mm_min: 500,
      }),
    });
    project.objects = [
      makeProjectObject({
        id: 'group1',
        name: 'Group',
        bounds: { min: { x: 0, y: 0 }, max: { x: 30, y: 30 } },
        layer_id: 'layer1',
        data: { type: 'group' as const, children: ['obj1', 'obj2'] },
      }),
      {
        ...project.objects[0],
        id: 'obj1',
        layer_id: 'layer2',
        created_at: '2026-01-01T00:00:01Z',
      },
      {
        ...project.objects[1],
        layer_id: 'layer2',
        created_at: '2026-01-01T00:00:02Z',
      },
    ];
    useProjectStore.setState({
      project,
      selectedLayerId: 'layer1',
      selectedObjectIds: ['group1'],
    });
    mockedVector.ungroupObjects.mockResolvedValue(['obj1', 'obj2']);

    await useProjectStore.getState().ungroupObjects('group1');

    expect(useProjectStore.getState().selectedLayerId).toBe('layer2');
  });

  it('gridArray calls service and reloads', async () => {
    mockedVector.gridArray.mockResolvedValue({ createdIds: ['c1'], groupId: null });
    mockedProject.getProject.mockResolvedValue(makeProject());

    await useProjectStore
      .getState()
      .gridArray({ objectIds: ['obj1'], rows: 3, cols: 4, hSpacingMm: 15, vSpacingMm: 15 });

    expect(mockedVector.gridArray).toHaveBeenCalledWith({
      objectIds: ['obj1'],
      rows: 3,
      cols: 4,
      hSpacingMm: 15,
      vSpacingMm: 15,
    });
    expect(mockedProject.getProject).toHaveBeenCalled();
  });

  it('gridArray retargets the active layer to a rerouted result group', async () => {
    const reloaded = makeProject();
    reloaded.layers.push({
      ...makeLayer({
        id: 'line-layer',
        name: 'Line',
        operation: 'line',
        order_index: 1,
        color_tag: '#ff0000',
        speed_mm_min: 600,
        power_percent: 80,
      }),
    });
    reloaded.objects.push(makeProjectObject({
      id: 'group-1',
      name: 'Array Group',
      bounds: { min: { x: 0, y: 0 }, max: { x: 30, y: 30 } },
      layer_id: 'line-layer',
      z_index: 2,
      data: { type: 'group' as const, children: ['obj1', 'obj2'] },
    }));
    useProjectStore.setState({ selectedLayerId: 'image-layer' });
    mockedVector.gridArray.mockResolvedValue({ createdIds: ['obj1', 'obj2'], groupId: 'group-1' });
    mockedProject.getProject.mockResolvedValue(reloaded);

    await useProjectStore.getState().gridArray({
      objectIds: ['obj1'],
      rows: 2,
      cols: 2,
      hSpacingMm: 15,
      vSpacingMm: 15,
      groupResults: true,
    });

    expect(useProjectStore.getState().selectedLayerId).toBe('line-layer');
    expect(useProjectStore.getState().selectedObjectIds).toEqual(['group-1']);
  });

  it('circularArray calls service and reloads', async () => {
    mockedVector.circularArray.mockResolvedValue({ createdIds: ['c1', 'c2'], groupId: null });
    mockedProject.getProject.mockResolvedValue(makeProject());

    await useProjectStore.getState().circularArray({ objectIds: ['obj1'], count: 3, radiusMm: 50 });

    expect(mockedVector.circularArray).toHaveBeenCalledWith({
      objectIds: ['obj1'],
      count: 3,
      radiusMm: 50,
    });
    expect(mockedProject.getProject).toHaveBeenCalled();
    expect(useProjectStore.getState().selectedObjectIds).toEqual(['c1', 'c2']);
  });

  it('circularArray selects group when groupId returned', async () => {
    mockedVector.circularArray.mockResolvedValue({
      createdIds: ['orig-1', 'c1', 'c2'],
      groupId: 'g1',
    });
    mockedProject.getProject.mockResolvedValue(makeProject());

    await useProjectStore
      .getState()
      .circularArray({ objectIds: ['orig-1'], count: 3, radiusMm: 50 });

    expect(useProjectStore.getState().selectedObjectIds).toEqual(['g1']);
  });

  it('circularArray retargets the active layer to a rerouted result group', async () => {
    const reloaded = makeProject();
    reloaded.layers.push({
      ...makeLayer({
        id: 'line-layer',
        name: 'Line',
        operation: 'line',
        order_index: 1,
        color_tag: '#ff0000',
        speed_mm_min: 600,
        power_percent: 80,
      }),
    });
    reloaded.objects.push(makeProjectObject({
      id: 'g1',
      name: 'Array Group',
      bounds: { min: { x: 0, y: 0 }, max: { x: 40, y: 40 } },
      layer_id: 'line-layer',
      z_index: 2,
      data: { type: 'group' as const, children: ['obj1', 'obj2'] },
    }));
    useProjectStore.setState({ selectedLayerId: 'image-layer' });
    mockedVector.circularArray.mockResolvedValue({
      createdIds: ['orig-1', 'c1', 'c2'],
      groupId: 'g1',
    });
    mockedProject.getProject.mockResolvedValue(reloaded);

    await useProjectStore
      .getState()
      .circularArray({ objectIds: ['orig-1'], count: 3, radiusMm: 50, groupResults: true });

    expect(useProjectStore.getState().selectedLayerId).toBe('line-layer');
  });

  it('autoJoinShapes reloads after the backend returns path strings', async () => {
    mockedProject.autoJoinShapes.mockResolvedValue(['M0 0 L20 20']);
    mockedProject.getProject.mockResolvedValue(makeProject());

    await useProjectStore.getState().autoJoinShapes(['obj1', 'obj2'], 0.5);

    expect(useProjectStore.getState().selectedObjectIds).toEqual(['obj1']);
    expect(useProjectStore.getState().selectedLayerId).toBe('layer1');
  });

  it('optimizeShapes reloads after the backend returns path strings', async () => {
    mockedProject.optimizeShapes.mockResolvedValue(['M0 0 L10 10']);
    mockedProject.getProject.mockResolvedValue(makeProject());

    await useProjectStore.getState().optimizeShapes(['obj1']);

    expect(useProjectStore.getState().selectedObjectIds).toEqual(['obj1']);
    expect(useProjectStore.getState().selectedLayerId).toBe('layer1');
  });

  it('circularArray re-throws on failure', async () => {
    mockedVector.circularArray.mockRejectedValue(new Error('backend error'));

    await expect(
      useProjectStore.getState().circularArray({ objectIds: ['obj1'], count: 3, radiusMm: 50 }),
    ).rejects.toThrow('backend error');
  });

  it('breakApart selects created objects after success', async () => {
    const created = [
      {
        id: 'part1',
        name: 'R1 Part 1',
        visible: true,
        locked: false,
        transform: { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 },
        bounds: { min: { x: 0, y: 0 }, max: { x: 5, y: 5 } },
        layer_id: 'layer1',
        z_index: 0,
        data: { type: 'vector_path' as const, path_data: 'M0 0 L5 5 Z', closed: true },
      },
      {
        id: 'part2',
        name: 'R1 Part 2',
        visible: true,
        locked: false,
        transform: { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 },
        bounds: { min: { x: 5, y: 5 }, max: { x: 10, y: 10 } },
        layer_id: 'layer1',
        z_index: 1,
        data: { type: 'vector_path' as const, path_data: 'M5 5 L10 10 Z', closed: true },
      },
    ];
    mockedVector.breakApart.mockResolvedValue(created);
    mockedProject.getProject.mockResolvedValue(makeProject());

    await useProjectStore.getState().breakApart('obj1');

    expect(mockedVector.breakApart).toHaveBeenCalledWith('obj1');
    expect(mockedProject.getProject).toHaveBeenCalled();
    const state = useProjectStore.getState();
    expect(state.selectedObjectIds).toEqual(['part1', 'part2']);
    expect(state.project?.dirty).toBe(true);
  });

  it('breakApart retargets the active layer to the first created result layer after reload', async () => {
    const created = [
      makeProjectObject({
        id: 'part1',
        name: 'Part 1',
        bounds: { min: { x: 0, y: 0 }, max: { x: 5, y: 5 } },
        layer_id: 'layer2',
        data: { type: 'vector_path' as const, path_data: 'M0 0 L5 5', closed: false },
      }),
    ];
    const reloaded = makeProject();
    reloaded.layers.push({
      ...makeLayer({
        id: 'layer2',
        name: 'L2',
        operation: 'line',
        order_index: 1,
        color_tag: '#00ff00',
        speed_mm_min: 500,
      }),
    });
    reloaded.objects = created;
    useProjectStore.setState({ selectedLayerId: 'layer1' });
    mockedVector.breakApart.mockResolvedValue(created);
    mockedProject.getProject.mockResolvedValue(reloaded);

    await useProjectStore.getState().breakApart('obj1');

    expect(useProjectStore.getState().selectedLayerId).toBe('layer2');
  });

  it('breakApart single-subpath is silent no-op', async () => {
    mockedVector.breakApart.mockResolvedValue([]);

    await useProjectStore.getState().breakApart('obj1');

    // No error notification — empty result is a no-op
    const notifications = useNotificationStore.getState().notifications;
    expect(notifications).toHaveLength(0);
    // Selection should remain unchanged
    expect(useProjectStore.getState().selectedObjectIds).toEqual(['obj1']);
    // Project should not be reloaded or marked dirty
    expect(mockedProject.getProject).not.toHaveBeenCalled();
  });

  it('breakApart real errors still show notification', async () => {
    mockedVector.breakApart.mockRejectedValue('Object not found');

    await useProjectStore.getState().breakApart('obj1');

    const notifications = useNotificationStore.getState().notifications;
    expect(notifications).toHaveLength(1);
    expect(notifications[0].type).toBe('error');
  });

  it('offsetShapes selects created objects after success', async () => {
    const created = [
      {
        id: 'off1',
        name: 'Offset',
        visible: true,
        locked: false,
        transform: { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 },
        bounds: { min: { x: -1, y: -1 }, max: { x: 11, y: 11 } },
        layer_id: 'layer1',
        z_index: 2,
        data: { type: 'vector_path' as const, path_data: 'M-1 -1 L11 11', closed: true },
      },
      {
        id: 'off2',
        name: 'Offset 2',
        visible: true,
        locked: false,
        transform: { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 },
        bounds: { min: { x: 1, y: 1 }, max: { x: 9, y: 9 } },
        layer_id: 'layer1',
        z_index: 3,
        data: { type: 'vector_path' as const, path_data: 'M1 1 L9 9', closed: true },
      },
    ];
    mockedVector.offsetShapes.mockResolvedValue(created);
    mockedProject.getProject.mockResolvedValue(makeProject());

    await useProjectStore.getState().offsetShapes(['obj1'], 1, 'both', 'miter', false);

    expect(mockedVector.offsetShapes).toHaveBeenCalledWith(['obj1'], 1, 'both', 'miter', false);
    const state = useProjectStore.getState();
    expect(state.selectedObjectIds).toEqual(['off1', 'off2']);
    expect(state.project?.dirty).toBe(true);
  });

  it('closeAndJoin retargets the active layer to the joined result', async () => {
    const result = {
      object: {
        id: 'joined1',
        name: 'Joined',
        visible: true,
        locked: false,
        transform: { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 },
        bounds: { min: { x: 0, y: 0 }, max: { x: 20, y: 10 } },
        layer_id: 'layer2',
        z_index: 0,
        data: { type: 'vector_path' as const, path_data: 'M0 0 L20 10', closed: false },
      },
      fullyClosed: true,
    };
    const project = makeProject();
    mockedProject.getProject.mockResolvedValue({
      ...project,
      objects: [result.object],
    });
    project.layers.push({
      ...makeLayer({
        id: 'layer2',
        name: 'L2',
        operation: 'line',
        order_index: 1,
        color_tag: '#00ff00',
        speed_mm_min: 500,
      }),
    });
    useProjectStore.setState({
      project,
      selectedLayerId: 'layer1',
      selectedObjectIds: ['obj1', 'obj2'],
    });
    mockedVector.closeAndJoin.mockResolvedValue(result);

    const storeResult = await useProjectStore.getState().closeAndJoin(['obj1', 'obj2']);

    expect(storeResult).toBe(result);
    expect(mockedProject.getProject).toHaveBeenCalled();
    expect(useProjectStore.getState().selectedLayerId).toBe('layer2');
    expect(useProjectStore.getState().selectedObjectIds).toEqual(['joined1']);
  });

  it('closeAndJoin refreshes the project so joined bounds are authoritative', async () => {
    const serviceObject = {
      id: 'joined1',
      name: 'Joined',
      visible: true,
      locked: false,
      transform: { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 },
      bounds: { min: { x: 0, y: 0 }, max: { x: 1, y: 1 } },
      layer_id: 'layer1',
      z_index: 0,
      data: { type: 'vector_path' as const, path_data: 'M0 0 L1 1', closed: false },
    };
    const refreshedObject = {
      ...serviceObject,
      bounds: { min: { x: 10, y: 20 }, max: { x: 30, y: 40 } },
      data: { type: 'vector_path' as const, path_data: 'M10 20 L30 40', closed: false },
    };
    const result = { object: serviceObject, fullyClosed: true };
    mockedVector.closeAndJoin.mockResolvedValue(result);
    mockedProject.getProject.mockResolvedValue({
      ...makeProject(),
      objects: [refreshedObject],
    });

    await useProjectStore.getState().closeAndJoin(['obj1', 'obj2']);

    const joined = useProjectStore.getState().project?.objects.find((object) => object.id === 'joined1');
    expect(joined?.bounds).toEqual(refreshedObject.bounds);
    expect(joined?.data).toEqual(refreshedObject.data);
    expect(useProjectStore.getState().selectedObjectIds).toEqual(['joined1']);
  });

  it('convertToBitmap retargets the active layer to the rerouted image layer', async () => {
    const updated = {
      ...makeProject().objects[0],
      id: 'obj1',
      layer_id: 'image-layer',
      data: {
        type: 'raster_image' as const,
        asset_key: 'asset-1',
        original_width_px: 100,
        original_height_px: 100,
      },
    };
    const reloaded = {
      ...makeProject(),
      layers: [
        ...makeProject().layers,
        makeLayer({
          id: 'image-layer',
          name: 'Image',
          operation: 'image',
          order_index: 1,
          color_tag: '#ff0000',
          speed_mm_min: 600,
          power_percent: 80,
        }),
      ],
      objects: [updated, makeProject().objects[1]],
    };
    mockedVector.convertToBitmap.mockResolvedValue(updated);
    mockedProject.getProject.mockResolvedValue(reloaded);

    await useProjectStore.getState().convertToBitmap('obj1', 300);

    expect(useProjectStore.getState().selectedLayerId).toBe('image-layer');
    expect(useProjectStore.getState().selectedObjectIds).toEqual(['obj1']);
  });

  it('applyMaskToImage retargets the active layer to the rerouted vector layer', async () => {
    const updated = {
      ...makeProject().objects[0],
      id: 'obj1',
      layer_id: 'line-layer',
      data: {
        type: 'vector_path' as const,
        path_data: 'M0 0 L10 10',
        closed: true,
      },
    };
    const reloaded = {
      ...makeProject(),
      layers: [
        ...makeProject().layers,
        makeLayer({
          id: 'line-layer',
          name: 'Line',
          operation: 'line',
          order_index: 1,
          color_tag: '#ff0000',
          speed_mm_min: 600,
          power_percent: 80,
        }),
      ],
      objects: [updated, makeProject().objects[1]],
    };
    mockedVector.applyMaskToImage.mockResolvedValue(updated);
    mockedProject.getProject.mockResolvedValue(reloaded);

    await useProjectStore.getState().applyMaskToImage('obj1', 'mask1');

    expect(useProjectStore.getState().selectedLayerId).toBe('line-layer');
    expect(useProjectStore.getState().selectedObjectIds).toEqual(['obj1']);
  });

  it('rubberBandOutline appends and selects the created outline object', async () => {
    const outline = {
      id: 'outline-1',
      name: 'Rubber Band Outline',
      visible: true,
      locked: false,
      transform: { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 },
      bounds: { min: { x: 0, y: 0 }, max: { x: 40, y: 40 } },
      layer_id: 'line-layer',
      z_index: 2,
      data: { type: 'vector_path' as const, path_data: 'M0 0 L40 40 Z', closed: true },
    };
    mockedVector.rubberBandOutline.mockResolvedValue(outline);

    await useProjectStore.getState().rubberBandOutline(['obj1', 'obj2']);

    expect(useProjectStore.getState().project?.objects.find((o) => o.id === 'outline-1')).toEqual(
      outline,
    );
    expect(useProjectStore.getState().selectedObjectIds).toEqual(['outline-1']);
    expect(useProjectStore.getState().selectedLayerId).toBe('line-layer');
  });

  it('offsetShapes with empty result sets empty selection', async () => {
    mockedVector.offsetShapes.mockResolvedValue([]);
    mockedProject.getProject.mockResolvedValue(makeProject());

    await useProjectStore.getState().offsetShapes(['obj1'], 5, 'inward', 'miter', false);

    expect(useProjectStore.getState().selectedObjectIds).toEqual([]);
  });

  it('reassignLayer retargets the active layer when the current selection is moved', async () => {
    const reloaded = makeProject();
    reloaded.layers.push({
      ...makeLayer({
        id: 'layer2',
        name: 'L2',
        operation: 'line',
        order_index: 1,
        color_tag: '#00ff00',
        speed_mm_min: 500,
      }),
    });
    reloaded.objects = [
      {
        ...reloaded.objects[0],
        layer_id: 'layer2',
      },
      reloaded.objects[1],
    ];
    mockedProject.reassignLayer.mockResolvedValue(undefined);
    mockedProject.getProject.mockResolvedValue(reloaded);
    useProjectStore.setState({
      project: makeProject(),
      selectedLayerId: 'layer1',
      selectedObjectIds: ['obj1'],
    });

    await useProjectStore.getState().reassignLayer(['obj1'], 'layer2');

    expect(useProjectStore.getState().selectedLayerId).toBe('layer2');
  });

  it('offsetShapes rethrows error after notification', async () => {
    mockedVector.offsetShapes.mockRejectedValue(new Error('offset failed'));

    await expect(
      useProjectStore.getState().offsetShapes(['obj1'], 1, 'outward', 'miter', false),
    ).rejects.toThrow('offset failed');

    const notifications = useNotificationStore.getState().notifications;
    expect(notifications).toHaveLength(1);
    expect(notifications[0].type).toBe('error');
  });

  it('refreshImage reloads project after backend rewrites the image asset key', async () => {
    const projectBefore = makeProject();
    projectBefore.objects[0] = {
      ...projectBefore.objects[0],
      data: { type: 'raster_image', asset_key: 'asset-old', original_width_px: 10, original_height_px: 10 },
    };
    const projectAfter = makeProject();
    projectAfter.objects[0] = {
      ...projectAfter.objects[0],
      data: { type: 'raster_image', asset_key: 'asset-new', original_width_px: 20, original_height_px: 20 },
    };
    mockedImport.refreshImage.mockResolvedValue(projectAfter.objects[0]);
    mockedProject.getProject.mockResolvedValue(projectAfter);
    useProjectStore.setState({
      project: projectBefore,
      assetCache: new Map([['asset-old', 'blob:old']]),
      assetLoadErrors: new Map([['asset-old', 'old error']]),
    });

    await useProjectStore.getState().refreshImage('obj1');

    expect(mockedImport.refreshImage).toHaveBeenCalledWith('obj1');
    expect(mockedProject.getProject).toHaveBeenCalledOnce();
    expect(useProjectStore.getState().project?.objects[0].data).toEqual(projectAfter.objects[0].data);
    expect(useProjectStore.getState().assetCache.has('asset-old')).toBe(false);
    expect(useProjectStore.getState().assetLoadErrors.has('asset-old')).toBe(false);
    expect(previewInvalidate).toHaveBeenCalledOnce();
    expect(undoRefresh).toHaveBeenCalledOnce();
  });

  it('replaceImage reloads project so passthrough clone updates are not lost locally', async () => {
    const reloaded = makeProject();
    reloaded.objects[0].bounds = { min: { x: 0, y: 0 }, max: { x: 40, y: 40 } };
    mockedImport.replaceImage.mockResolvedValue(reloaded.objects[0]);
    mockedProject.getProject.mockResolvedValue(reloaded);

    await useProjectStore.getState().replaceImage('obj1', '/tmp/new.png');

    expect(mockedImport.replaceImage).toHaveBeenCalledWith('obj1', '/tmp/new.png');
    expect(mockedProject.getProject).toHaveBeenCalledOnce();
    expect(useProjectStore.getState().project?.objects[0].bounds).toEqual(
      reloaded.objects[0].bounds,
    );
    expect(previewInvalidate).toHaveBeenCalledOnce();
  });

  it('replaceImage ignores dialog cancel without notifying', async () => {
    mockedImport.replaceImage.mockResolvedValue(null);

    await useProjectStore.getState().replaceImage('obj1');

    expect(mockedImport.replaceImage).toHaveBeenCalledWith('obj1', undefined);
    expect(mockedProject.getProject).not.toHaveBeenCalled();
    expect(previewInvalidate).not.toHaveBeenCalled();
    expect(useNotificationStore.getState().notifications).toHaveLength(0);
  });

  it('handles errors with notification', async () => {
    mockedProject.lockObjects.mockRejectedValue('Lock failed');

    await useProjectStore.getState().lockObjects(['obj1']);

    const notifications = useNotificationStore.getState().notifications;
    expect(notifications).toHaveLength(1);
    expect(notifications[0].type).toBe('error');
  });

  it('convertToBitmap reloads project state so rerouted sibling layers are preserved', async () => {
    const updated = {
      ...makeProject().objects[0],
      layer_id: 'layer-image',
      data: {
        type: 'raster_image' as const,
        asset_key: 'asset-1',
        original_width_px: 100,
        original_height_px: 100,
        adjustments: undefined,
      },
    };
    const reloaded = {
      ...makeProject(),
      layers: [
        ...makeProject().layers,
        {
          ...makeLayer({
            id: 'layer-image',
            name: 'L1 Image',
            operation: 'image',
            order_index: 1,
            color_tag: '#ff0000',
            speed_mm_min: 600,
            power_percent: 80,
          }),
        },
      ],
      objects: [updated, makeProject().objects[1]],
    };
    mockedVector.convertToBitmap.mockResolvedValue(updated);
    mockedProject.getProject.mockResolvedValue(reloaded);

    await useProjectStore.getState().convertToBitmap('obj1', 300);

    const state = useProjectStore.getState();
    expect(state.project?.layers.some((layer) => layer.id === 'layer-image')).toBe(true);
    expect(state.selectedObjectIds).toEqual(['obj1']);
    expect(state.selectedLayerId).toBe('layer-image');
  });

  it('applyPathToText selects the created path after conversion', async () => {
    const created = {
      ...makeProject().objects[0],
      id: 'pathified-1',
      data: { type: 'vector_path' as const, path_data: 'M0 0 L10 0', closed: false },
    };
    const reloaded = {
      ...makeProject(),
      objects: [created, makeProject().objects[1]],
    };
    mockedVector.applyPathToText.mockResolvedValue(created);
    mockedProject.getProject.mockResolvedValue(reloaded);

    await useProjectStore.getState().applyPathToText('obj1', 'obj2');

    expect(useProjectStore.getState().selectedObjectIds).toEqual(['pathified-1']);
  });

  it('applyMaskToImage reloads project state so created sibling layers are preserved', async () => {
    const updated = {
      ...makeProject().objects[0],
      layer_id: 'layer-vector',
      data: { type: 'vector_path' as const, path_data: 'M0 0 L10 10 Z', closed: true },
    };
    const reloaded = {
      ...makeProject(),
      layers: [
        ...makeProject().layers,
        {
          ...makeLayer({
            id: 'layer-vector',
            name: 'L1 Line',
            operation: 'line',
            order_index: 1,
            color_tag: '#ff0000',
            speed_mm_min: 600,
            power_percent: 80,
          }),
        },
      ],
      objects: [updated, makeProject().objects[1]],
    };
    mockedVector.applyMaskToImage.mockResolvedValue(updated);
    mockedProject.getProject.mockResolvedValue(reloaded);

    await useProjectStore.getState().applyMaskToImage('obj1', 'obj2');

    const state = useProjectStore.getState();
    expect(state.project?.layers.some((layer) => layer.id === 'layer-vector')).toBe(true);
    expect(state.selectedObjectIds).toEqual(['obj1']);
    expect(state.selectedLayerId).toBe('layer-vector');
  });

  it('copyAlongPath batches the request and selects the created copies', async () => {
    const created = [
      { ...makeProject().objects[0], id: 'copy-1' },
      { ...makeProject().objects[1], id: 'copy-2' },
    ];
    const reloaded = {
      ...makeProject(),
      objects: [...makeProject().objects, ...created],
    };
    mockedVector.copyAlongPathBatch.mockResolvedValue(created);
    mockedProject.getProject.mockResolvedValue(reloaded);

    await useProjectStore.getState().copyAlongPath(['obj1', 'obj2'], 'guide-1', {
      count: 6,
      rotateCopies: true,
      scaleCopies: false,
      finalScalePercent: 100,
    });

    expect(mockedVector.copyAlongPathBatch).toHaveBeenCalledTimes(1);
    // rotate defaults to true when the store action's 4th arg is omitted.
    expect(mockedVector.copyAlongPathBatch).toHaveBeenCalledWith(['obj1', 'obj2'], 'guide-1', {
      count: 6,
      rotateCopies: true,
      scaleCopies: false,
      finalScalePercent: 100,
    });
    expect(useProjectStore.getState().selectedObjectIds).toEqual(['copy-1', 'copy-2']);
  });

  it('copyAlongPath selects copied group roots instead of copied children', async () => {
    const createdGroup = makeProjectObject({
      id: 'copy-group',
      data: { type: 'group' as const, children: ['copy-child'] },
      bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } },
    });
    const createdChild = makeProjectObject({
      id: 'copy-child',
      bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } },
    });
    const reloaded = {
      ...makeProject(),
      objects: [...makeProject().objects, createdGroup, createdChild],
    };
    mockedVector.copyAlongPathBatch.mockResolvedValue([createdGroup, createdChild]);
    mockedProject.getProject.mockResolvedValue(reloaded);

    await useProjectStore.getState().copyAlongPath(['group-1'], 'guide-1', {
      count: 6,
      rotateCopies: true,
      scaleCopies: false,
      finalScalePercent: 100,
    });

    expect(useProjectStore.getState().selectedObjectIds).toEqual(['copy-group']);
  });

  it('copyAlongPath refuses locked selections before calling the service', async () => {
    useProjectStore.setState({
      project: makeProject({
        objects: [
          makeProjectObject({
            id: 'obj1',
            locked: true,
            data: { type: 'shape' as const, kind: 'rectangle' as const, width: 10, height: 10, corner_radius: 0 },
          }),
          makeProjectObject({
            id: 'guide-1',
            bounds: { min: { x: 0, y: 20 }, max: { x: 100, y: 25 } },
            data: { type: 'vector_path' as const, path_data: 'M 0 0 L 100 0', closed: false, ruler_guide_axis: null },
          }),
        ],
      }),
    });

    const applied = await useProjectStore.getState().copyAlongPath(['obj1'], 'guide-1', {
      count: 4,
      rotateCopies: true,
      scaleCopies: false,
      finalScalePercent: 100,
    });

    expect(applied).toBe(false);
    expect(mockedVector.copyAlongPathBatch).not.toHaveBeenCalled();
    expect(useNotificationStore.getState().notifications[0]?.message).toContain('Object is locked');
  });

  it('copyAlongPath refuses scale-locked projects when scaling is enabled', async () => {
    useProjectStore.setState({
      project: makeProject({
        transform_locks: { move_enabled: true, size_enabled: false, rotate_enabled: true, shear_enabled: true },
        objects: [
          ...makeProject().objects,
          makeProjectObject({
            id: 'guide-1',
            bounds: { min: { x: 0, y: 20 }, max: { x: 100, y: 25 } },
            data: { type: 'vector_path' as const, path_data: 'M 0 0 L 100 0', closed: false, ruler_guide_axis: null },
          }),
        ],
      }),
    });

    const applied = await useProjectStore.getState().copyAlongPath(['obj1'], 'guide-1', {
      count: 4,
      rotateCopies: true,
      scaleCopies: true,
      finalScalePercent: 50,
    });

    expect(applied).toBe(false);
    expect(mockedVector.copyAlongPathBatch).not.toHaveBeenCalled();
    expect(useNotificationStore.getState().notifications[0]?.message).toContain('Scale is locked');
  });

  it('rubberBandOutline selects the created outline', async () => {
    const created = {
      ...makeProject().objects[0],
      id: 'outline-1',
      data: { type: 'vector_path' as const, path_data: 'M0 0 L30 30 Z', closed: true },
    };
    mockedVector.rubberBandOutline.mockResolvedValue(created);

    await useProjectStore.getState().rubberBandOutline(['obj1', 'obj2']);

    expect(useProjectStore.getState().selectedObjectIds).toEqual(['outline-1']);
    expect(useProjectStore.getState().selectedLayerId).toBe('layer1');
  });

  it('setOptimization sends patch to backend and merges into store', async () => {
    const initial = makeProject();
    const backendMerged: ProjectOptimization = {
      ...DEFAULT_PROJECT_OPTIMIZATION,
      inner_first: true,
    };
    useProjectStore.setState({ project: initial });
    mockedProject.setOptimization.mockResolvedValue(backendMerged);

    await useProjectStore.getState().setOptimization({ inner_first: true });

    expect(mockedProject.setOptimization).toHaveBeenCalledWith({ inner_first: true });
    const stored = useProjectStore.getState().project;
    expect(stored?.optimization?.inner_first).toBe(true);
    expect(stored?.optimization?.ordering).toContain('layer');
    expect(stored?.dirty).toBe(true);
    expect(previewInvalidate).toHaveBeenCalled();
    expect(undoRefresh).toHaveBeenCalled();
  });

  it('setOptimization skips local invalidation when backend returns unchanged optimization', async () => {
    const withOpt = {
      ...makeProject(),
      optimization: {
        enabled: true,
        ordering: ['layer', 'priority'] as ProjectOptimization['ordering'],
        inner_first: false,
        direction_order: 'none' as const,
        reduce_travel: false,
        hide_backlash: false,
        reduce_direction_changes: false,
        choose_best_start: false,
        choose_corners: false,
        choose_best_direction: false,
        remove_overlapping: false,
        remove_overlap_tolerance_mm: 0.05,
        start_point_x: null,
        start_point_y: null,
        finish_position: 'origin' as const,
        finish_x: null,
        finish_y: null,
      },
    };
    useProjectStore.setState({ project: withOpt });
    mockedProject.setOptimization.mockResolvedValue(withOpt.optimization);

    await useProjectStore.getState().setOptimization({ enabled: true });

    expect(mockedProject.setOptimization).toHaveBeenCalledWith({ enabled: true });
    expect(previewInvalidate).not.toHaveBeenCalled();
    expect(undoRefresh).not.toHaveBeenCalled();
    expect(useProjectStore.getState().project?.dirty).toBeFalsy();
  });

  it('setOptimization partial patch preserves sibling fields', async () => {
    const initialOptimization: ProjectOptimization = {
      ...DEFAULT_PROJECT_OPTIMIZATION,
      enabled: true,
      ordering: ['layer', 'priority'],
      inner_first: true,
      reduce_travel: true,
    };
    useProjectStore.setState({
      project: {
        ...makeProject(),
        optimization: initialOptimization,
      },
    });
    mockedProject.setOptimization.mockResolvedValue({
      ...initialOptimization,
      inner_first: false,
    });

    await useProjectStore.getState().setOptimization({ inner_first: false });

    const opt = useProjectStore.getState().project?.optimization;
    expect(opt?.inner_first).toBe(false);
    // Sibling flag untouched.
    expect(opt?.reduce_travel).toBe(true);
  });
});
