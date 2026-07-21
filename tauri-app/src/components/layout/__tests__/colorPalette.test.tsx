import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, cleanup, fireEvent, waitFor } from '@testing-library/react';
import { ColorPalette } from '../ColorPalette';
import { useProjectStore } from '../../../stores/projectStore';
import type { Project } from '../../../types/project';
import { makeLayer, makeProject as makeProjectFixture, makeProjectObject, makeRasterSettings } from '../../../test-utils/projectFixtures';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockReturnValue(new Promise(() => {})) }));
vi.mock('../../../services/projectService', () => ({
  projectService: {
    addLayer: vi.fn(),
    updateLayer: vi.fn(),
    createProject: vi.fn(),
    closeProject: vi.fn(),
    getProject: vi.fn(),
    getUndoState: vi.fn().mockResolvedValue({ can_undo: false, can_redo: false }),
    undoProject: vi.fn(),
    redoProject: vi.fn(),
  },
}));

import { projectService } from '../../../services/projectService';

const makeProject = (): Project => ({
  ...makeProjectFixture({
    metadata: { format_version: '1', app_version: '0.1.0', project_id: 'p1', project_name: 'Test', created_at: '', modified_at: '' },
    workspace: { bed_width_mm: 400, bed_height_mm: 400, origin: 'top_left' as const },
    assets: [],
  }),
  layers: [
    makeLayer({ id: 'l1', name: 'L1', operation: 'line', order_index: 0, color_tag: '#FF0000' }),
    makeLayer({ id: 'l2', name: 'L2', operation: 'line', order_index: 1, color_tag: '#00FF00' }),
  ],
  objects: [makeProjectObject({
    id: 'obj1', name: 'Rect1',
    bounds: { min: { x: 10, y: 20 }, max: { x: 60, y: 70 } },
    layer_id: 'l1',
    data: { type: 'shape' as const, kind: 'rectangle' as const, width: 50, height: 50, corner_radius: 0 },
  })],
});

const initialState = useProjectStore.getState();
const mockedProjectService = projectService as unknown as Record<string, ReturnType<typeof vi.fn>>;

afterEach(() => {
  cleanup();
  useProjectStore.setState(initialState, true);
});

describe('ColorPalette', () => {
  it('renders 32 swatches', () => {
    useProjectStore.setState({ project: makeProject() });
    render(<ColorPalette />);
    const buttons = screen.getAllByRole('button');
    expect(buttons).toHaveLength(32);
  });

  it('click assigns to matching layer', async () => {
    const reassignLayer = vi.fn().mockResolvedValue(true);
    useProjectStore.setState({
      project: makeProject(),
      selectedObjectIds: ['obj1'],
      reassignLayer,
    });
    render(<ColorPalette />);
    fireEvent.click(screen.getByLabelText('Green'));
    await waitFor(() => {
      expect(reassignLayer).toHaveBeenCalledWith(['obj1'], 'l2');
    });
  });

  it('tool layers have dashed border', () => {
    useProjectStore.setState({ project: makeProject() });
    render(<ColorPalette />);
    const toolSwatch = screen.getByLabelText('Tool 1');
    expect(toolSwatch.className).toContain('border-dashed');
  });

  it('current layer color highlighted', () => {
    useProjectStore.setState({
      project: makeProject(),
      selectedObjectIds: ['obj1'],
    });
    render(<ColorPalette />);
    const redSwatch = screen.getByLabelText('Red');
    expect(redSwatch.className).toContain('border-white');
  });

  it('selected layer color takes precedence over selected object color for highlight', () => {
    const project = makeProject();
    project.layers.push({
      ...makeLayer({
        id: 'l3', name: 'Image', operation: 'image', order_index: 2, color_tag: '#00FF00',
        raster_settings: makeRasterSettings({ mode: 'threshold', overscan_mm: 0 }),
        vector_settings: null,
      }),
    });
    useProjectStore.setState({
      project,
      selectedObjectIds: ['obj1'], // red object
      selectedLayerId: 'l3', // green image row
    });

    render(<ColorPalette />);

    expect(screen.getByLabelText('Green').className).toContain('border-white');
    expect(screen.getByLabelText('Red').className).not.toContain('border-white');
  });

  it('plain swatch click with no selection prefers the non-image sibling', () => {
    const project = makeProject();
    project.layers.push({
      ...makeLayer({
        id: 'l3', name: 'Green Image', operation: 'image', order_index: 2, color_tag: '#00FF00',
        raster_settings: makeRasterSettings({ mode: 'threshold', overscan_mm: 0 }),
        vector_settings: null,
      }),
    });
    // Put image row before the vector sibling in array order to
    // catch accidental first-match behavior.
    project.layers = [project.layers[2], project.layers[1], project.layers[0]];
    useProjectStore.setState({
      project,
      selectedObjectIds: [],
      selectedLayerId: null,
    });

    render(<ColorPalette />);
    fireEvent.click(screen.getByLabelText('Green'));

    expect(useProjectStore.getState().selectedLayerId).toBe('l2');
  });

  it('defers layer creation when clicking unassigned color with no selection', () => {
    useProjectStore.setState({
      project: makeProject(),
      selectedLayerId: 'l1',
    });

    render(<ColorPalette />);
    fireEvent.click(screen.getByLabelText('Blue'));

    // No layer should be created — just set pending palette color
    expect(mockedProjectService.addLayer).not.toHaveBeenCalled();
    expect(useProjectStore.getState().pendingPaletteColor).toBe('#0000FF');
  });

  it('pending palette color cleared on createProject (real store)', async () => {
    useProjectStore.setState({ project: makeProject(), pendingPaletteColor: '#0000FF' });
    mockedProjectService.createProject.mockResolvedValue(makeProject());

    await useProjectStore.getState().createProject('New');
    expect(useProjectStore.getState().pendingPaletteColor).toBeNull();
  });

  it('pending palette color cleared on closeProject (real store)', async () => {
    useProjectStore.setState({ project: makeProject(), pendingPaletteColor: '#0000FF' });
    mockedProjectService.closeProject.mockResolvedValue(undefined);

    await useProjectStore.getState().closeProject();
    expect(useProjectStore.getState().pendingPaletteColor).toBeNull();
  });

  it('pending palette color cleared on loadProject (real store)', async () => {
    useProjectStore.setState({ project: makeProject(), pendingPaletteColor: '#0000FF' });
    mockedProjectService.getProject.mockResolvedValue(makeProject());

    await useProjectStore.getState().loadProject();
    expect(useProjectStore.getState().pendingPaletteColor).toBeNull();
  });

  it('pending palette color cleared on undo', async () => {
    useProjectStore.setState({ project: makeProject(), pendingPaletteColor: '#0000FF' });
    const { useUndoStore } = await import('../../../stores/undoStore');
    mockedProjectService.undoProject.mockResolvedValue(makeProject());
    mockedProjectService.getUndoState.mockResolvedValue({ can_undo: false, can_redo: false });

    await useUndoStore.getState().undo();
    expect(useProjectStore.getState().pendingPaletteColor).toBeNull();
  });

  it('pending palette color cleared on redo', async () => {
    useProjectStore.setState({ project: makeProject(), pendingPaletteColor: '#0000FF' });
    const { useUndoStore } = await import('../../../stores/undoStore');
    mockedProjectService.redoProject.mockResolvedValue(makeProject());
    mockedProjectService.getUndoState.mockResolvedValue({ can_undo: false, can_redo: false });

    await useUndoStore.getState().redo();
    expect(useProjectStore.getState().pendingPaletteColor).toBeNull();
  });

  it('pending palette color cleared on selectLayer', () => {
    useProjectStore.setState({
      project: makeProject(),
      pendingPaletteColor: '#0000FF',
    });

    useProjectStore.getState().selectLayer('l1');
    expect(useProjectStore.getState().pendingPaletteColor).toBeNull();
  });

  it('recolors layer when all objects on source layer are selected and target color has no layer', async () => {
    const project = makeProject();
    const updateLayer = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({
      project,
      selectedLayerId: 'l1',
      selectedObjectIds: ['obj1'],
      updateLayer,
    });

    render(<ColorPalette />);
    fireEvent.click(screen.getByLabelText('Blue'));

    // All objects on l1 are selected → recolor l1, don't create new layer
    expect(updateLayer).toHaveBeenCalledWith('l1', { color_tag: '#0000FF' });
    expect(mockedProjectService.addLayer).not.toHaveBeenCalled();
  });

  it('creates new layer for partial selection on source layer', async () => {
    const project = makeProject();
    // Add a second object on the same layer so obj1 is a partial selection
    project.objects.push(makeProjectObject({
      id: 'obj2', name: 'Rect2',
      bounds: { min: { x: 20, y: 30 }, max: { x: 70, y: 80 } },
      layer_id: 'l1', z_index: 1,
      data: { type: 'shape' as const, kind: 'rectangle' as const, width: 50, height: 50, corner_radius: 0 },
      created_at: '2026-01-01T00:00:01Z',
    }));
    mockedProjectService.addLayer.mockResolvedValue({
      ...makeLayer({
        id: 'l3',
        name: 'Line',
        operation: 'line',
        order_index: 2,
        color_tag: '#000000',
      }),
    });
    mockedProjectService.updateLayer.mockResolvedValue(undefined);
    const loadProject = vi.fn().mockImplementation(async () => {
      useProjectStore.setState((state) => ({
        project: state.project ? {
          ...state.project,
          layers: [
            ...state.project.layers,
            {
              ...makeLayer({
                id: 'l3',
                name: 'Line',
                operation: 'line',
                order_index: 2,
                color_tag: '#0000FF',
              }),
            },
          ],
        } : state.project,
      }));
    });
    useProjectStore.setState({
      project,
      selectedLayerId: 'l1',
      selectedObjectIds: ['obj1'], // only 1 of 2 objects on l1
      loadProject,
      reassignLayer: vi.fn().mockResolvedValue(true),
    });

    render(<ColorPalette />);
    fireEvent.click(screen.getByLabelText('Blue'));

    // Partial selection → create new layer and reassign
    await waitFor(() => {
      expect(mockedProjectService.addLayer).toHaveBeenCalledWith('C03 (Line)', 'line');
      expect(mockedProjectService.updateLayer).toHaveBeenCalledWith('l3', { color_tag: '#0000ff' });
    });
  });

  it('aborts cleanup and active-layer follow-up when one mixed recolor move fails', async () => {
    const project: Project = {
      ...makeProject(),
      layers: [
        makeLayer({ id: 'line-red', name: 'Line Red', operation: 'line', order_index: 0, color_tag: '#FF0000' }),
        makeLayer({ id: 'line-green', name: 'Line Green', operation: 'line', order_index: 1, color_tag: '#00FF00' }),
        makeLayer({ id: 'img-red', name: 'Image Red', operation: 'image', order_index: 2, color_tag: '#FF0000', raster_settings: makeRasterSettings({ mode: 'threshold', overscan_mm: 0 }), vector_settings: null }),
        makeLayer({ id: 'img-green', name: 'Image Green', operation: 'image', order_index: 3, color_tag: '#00FF00', raster_settings: makeRasterSettings({ mode: 'threshold', overscan_mm: 0 }), vector_settings: null }),
      ],
      objects: [
        makeProjectObject({
          id: 'vec1', name: 'Rect1',
          bounds: { min: { x: 10, y: 20 }, max: { x: 60, y: 70 } },
          layer_id: 'line-red', z_index: 0,
          data: { type: 'shape' as const, kind: 'rectangle' as const, width: 50, height: 50, corner_radius: 0 },
        }),
        makeProjectObject({
          id: 'img1', name: 'Raster1',
          bounds: { min: { x: 70, y: 20 }, max: { x: 120, y: 70 } },
          layer_id: 'img-red', z_index: 1,
          data: {
            type: 'raster_image' as const,
            asset_key: 'asset-1',
            original_width_px: 500,
            original_height_px: 500,
            adjustments: undefined,
          },
          created_at: '2026-01-01T00:00:01Z',
        }),
      ],
    };
    const reassignLayer = vi.fn()
      .mockResolvedValueOnce(true)
      .mockResolvedValueOnce(false);
    const removeLayer = vi.fn().mockResolvedValue(undefined);
    const selectLayer = vi.fn();
    const loadProject = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({
      project,
      selectedLayerId: 'line-red',
      selectedObjectIds: ['vec1', 'img1'],
      reassignLayer,
      removeLayer,
      selectLayer,
      loadProject,
    });

    render(<ColorPalette />);
    fireEvent.click(screen.getByLabelText('Green'));

    await waitFor(() => {
      expect(reassignLayer).toHaveBeenCalledTimes(2);
    });
    expect(removeLayer).not.toHaveBeenCalled();
    expect(selectLayer).not.toHaveBeenCalled();
    expect(loadProject).not.toHaveBeenCalled();
  });
});
