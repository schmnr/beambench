import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, cleanup, fireEvent, waitFor } from '@testing-library/react';
import { PropertiesPanel } from '../PropertiesPanel';
import { useProjectStore } from '../../../stores/projectStore';
import { useUiStore } from '../../../stores/uiStore';
import { makeLayer, makeProject as makeProjectFixture, makeProjectObject, makeStarObjectData, makeTextObjectData } from '../../../test-utils/projectFixtures';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn((cmd: string) => {
    if (cmd === 'get_system_fonts') {
      return Promise.resolve(['Arial', 'Noto Sans CJK SC']);
    }
    return Promise.resolve(null);
  }),
}));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockReturnValue(new Promise(() => {})) }));

const makeProject = (objectOverrides: Record<string, unknown> = {}, transformOverrides: Record<string, unknown> = {}) => ({
  ...makeProjectFixture({
    metadata: { format_version: '1', app_version: '0.1.0', project_id: 'p1', project_name: 'Test', created_at: '', modified_at: '' },
    layers: [makeLayer({ id: 'l1', name: 'L1', operation: 'line', color_tag: '#ff0000' })],
    assets: [],
  }),
  objects: [makeProjectObject({
    id: 'obj1',
    name: 'Rect1',
    transform: { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0, ...transformOverrides },
    bounds: { min: { x: 10, y: 20 }, max: { x: 60, y: 70 } },
    layer_id: 'l1',
    data: { type: 'shape' as const, kind: 'rectangle' as const, width: 50, height: 50, corner_radius: 5 },
    ...objectOverrides,
  })],
});

const initialState = useProjectStore.getState();
const initialUiState = useUiStore.getState();

afterEach(() => {
  cleanup();
  useProjectStore.setState(initialState, true);
  useUiStore.setState(initialUiState, true);
});

describe('PropertiesPanel', () => {
  it('renders corner radius field for rectangle shapes', () => {
    useProjectStore.setState({ project: makeProject(), selectedObjectIds: ['obj1'] });
    render(<PropertiesPanel />);
    expect(screen.getByText('Corner Radius')).toBeDefined();
    const inputs = screen.getAllByRole('spinbutton');
    // Find the corner radius input (last spinbutton, value=5)
    const cornerRadiusInput = inputs.find((input) => (input as HTMLInputElement).value === '5');
    expect(cornerRadiusInput).toBeDefined();
  });

  it('hides corner radius field for non-rectangle objects', () => {
    const project = makeProject({
      data: { type: 'vector_path' as const, path_data: 'M0 0L10 10', closed: false },
    });
    useProjectStore.setState({ project, selectedObjectIds: ['obj1'] });
    render(<PropertiesPanel />);
    expect(screen.queryByText('Corner Radius')).toBeNull();
  });

  it('does not show misleading placeholder message', () => {
    useProjectStore.setState({ project: makeProject(), selectedObjectIds: ['obj1'] });
    render(<PropertiesPanel />);
    expect(screen.queryByText(/Position and numeric edits/)).toBeNull();
  });

  it('renders Locked control for single-object properties', () => {
    useProjectStore.setState({ project: makeProject(), selectedObjectIds: ['obj1'] });
    render(<PropertiesPanel />);
    expect(screen.getByText('Locked')).toBeDefined();
  });

  it('renders Sides input for polygon objects', () => {
    const project = makeProject({
      data: { type: 'polygon' as const, sides: 6, radius: 25 },
    });
    useProjectStore.setState({ project, selectedObjectIds: ['obj1'] });
    render(<PropertiesPanel />);
    expect(screen.getByText('Sides')).toBeDefined();
    const inputs = screen.getAllByRole('spinbutton');
    const sidesInput = inputs.find((input) => (input as HTMLInputElement).value === '6');
    expect(sidesInput).toBeDefined();
  });

  it('renders Points/Bulge/Ratio for star objects', () => {
    const project = makeProject({
      data: makeStarObjectData(),
    });
    useProjectStore.setState({ project, selectedObjectIds: ['obj1'] });
    render(<PropertiesPanel />);
    expect(screen.getByText('Points')).toBeDefined();
    expect(screen.getByText('Bulge')).toBeDefined();
    expect(screen.getByText('Ratio')).toBeDefined();
    expect(screen.getByText('Dual Radius')).toBeDefined();
  });

  it('hides star fields for non-star objects', () => {
    useProjectStore.setState({ project: makeProject(), selectedObjectIds: ['obj1'] });
    render(<PropertiesPanel />);
    expect(screen.queryByText('Points')).toBeNull();
    expect(screen.queryByText('Bulge')).toBeNull();
  });

  it('shows Ratio 2 when dual_radius is enabled', () => {
    const project = makeProject({
      data: makeStarObjectData({ dual_radius: true, ratio2: 0.7 }),
    });
    useProjectStore.setState({ project, selectedObjectIds: ['obj1'] });
    render(<PropertiesPanel />);
    expect(screen.getByText('Ratio 2')).toBeDefined();
  });

  it('renders text shape properties and mode-specific visibility', async () => {
    const project = makeProject({
      data: {
        type: 'text' as const,
        content: 'Hello',
        font_family: 'sans-serif',
        font_size_mm: 10,
        alignment: 'left' as const,
        alignment_v: 'top' as const,
        bold: false,
        italic: false,
        upper_case: false,
        welded: false,
        h_spacing: 0,
        v_spacing: 0,
        on_path: false,
        path_offset: 0,
        distort: false,
        layout_mode: 'straight' as const,
        rtl: false,
        bend_radius: 0,
        transform_style: 'none' as const,
        transform_curve: 0,
        circle_placement: 'top_outside' as const,
        max_width: 40,
        squeeze: true,
        ignore_empty_vars: true,
      },
    });
    useProjectStore.setState({ project, selectedObjectIds: ['obj1'] });
    render(<PropertiesPanel />);
    expect(screen.getByText(/Max Width/)).toBeDefined();
    expect(screen.getByText('Squeeze')).toBeDefined();
    expect(screen.queryByText('Ignore Empty Vars')).toBeNull();
    expect(screen.getByText('RTL')).toBeDefined();
    expect(screen.queryByText('Path Offset')).toBeNull();
    expect(screen.queryByText('Bend Radius')).toBeNull();
    expect(await screen.findByRole('option', { name: 'Noto Sans CJK SC' })).toBeDefined();
  });

  it('loads system fonts in the text properties panel', async () => {
    const project = makeProject({
      data: makeTextObjectData({ font_family: 'Arial' }),
    });
    useProjectStore.setState({ project, selectedObjectIds: ['obj1'] });

    render(<PropertiesPanel />);

    expect(await screen.findByRole('option', { name: 'Noto Sans CJK SC' })).toBeDefined();
  });

  it('shows missing glyph warnings for text objects', async () => {
    const project = makeProject({
      data: makeTextObjectData({ missing_glyphs: ['中'] }),
    });
    useProjectStore.setState({ project, selectedObjectIds: ['obj1'] });

    render(<PropertiesPanel />);

    expect(screen.getByText(/Missing glyphs: 中/)).toBeDefined();
    expect(await screen.findByRole('option', { name: 'Noto Sans CJK SC' })).toBeDefined();
  });

  it('does not duplicate the Transform section width and height controls', () => {
    useProjectStore.setState({ project: makeProject(), selectedObjectIds: ['obj1'] });

    render(<PropertiesPanel />);

    expect(screen.queryByLabelText('Width')).toBeNull();
    expect(screen.queryByLabelText('Height')).toBeNull();
  });

  it('Path layout enters guide-pick mode from the text properties panel', async () => {
    const project = makeProject({
      data: {
        type: 'text' as const,
        content: 'Hello',
        font_family: 'sans-serif',
        font_size_mm: 10,
        alignment: 'left' as const,
        alignment_v: 'top' as const,
        bold: false,
        italic: false,
        upper_case: false,
        welded: false,
        h_spacing: 0,
        v_spacing: 0,
        on_path: false,
        path_offset: 0,
        distort: false,
        layout_mode: 'straight' as const,
        rtl: false,
        bend_radius: 0,
        transform_style: 'none' as const,
        transform_curve: 0,
        circle_placement: 'top_outside' as const,
        max_width: null,
        squeeze: false,
        ignore_empty_vars: false,
      },
    });
    const updateObjectData = vi.fn().mockResolvedValue(true);
    useProjectStore.setState({ project, selectedObjectIds: ['obj1'], updateObjectData });
    useUiStore.setState({ pendingGuidePathTextId: null });

    render(<PropertiesPanel />);
    fireEvent.change(screen.getByLabelText('Layout'), { target: { value: 'path' } });

    await waitFor(() => {
      expect(updateObjectData).toHaveBeenCalledWith(
        'obj1',
        expect.objectContaining({ layout_mode: 'path', on_path: true }),
      );
    });
    expect(useUiStore.getState().pendingGuidePathTextId).toBe('obj1');
  });

  it('Convert to Path reloads with preview invalidation', async () => {
    const loadProjectSpy = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({
      project: makeProject(),
      selectedObjectIds: ['obj1'],
      loadProject: loadProjectSpy,
    });

    render(<PropertiesPanel />);

    fireEvent.click(screen.getByRole('button', { name: 'Convert to Path' }));

    await waitFor(() => {
      expect(loadProjectSpy).toHaveBeenCalledWith({ invalidatePreview: true });
    });
  });

  it('routes multi-select layer changes through reassignLayer', () => {
    const base = makeProject();
    const project = {
      ...base,
      layers: [
        ...base.layers,
        makeLayer({
          id: 'l2',
          name: 'L2',
          operation: 'cut',
          order_index: 1,
          color_tag: '#00ff00',
          speed_mm_min: 900,
          power_percent: 60,
        }),
      ],
      objects: [
        base.objects[0],
        { ...base.objects[0], id: 'obj2', name: 'Rect2' },
      ],
    };
    const reassignLayer = vi.fn();
    useProjectStore.setState({ project, selectedObjectIds: ['obj1', 'obj2'], reassignLayer });

    render(<PropertiesPanel />);
    fireEvent.change(screen.getByLabelText('Layer'), { target: { value: 'l2' } });

    expect(reassignLayer).toHaveBeenCalledWith(['obj1', 'obj2'], 'l2');
  });

  it('routes multi-select visibility and lock toggles through batch store actions', () => {
    const base = makeProject();
    const project = {
      ...base,
      objects: [
        base.objects[0],
        { ...base.objects[0], id: 'obj2', name: 'Rect2', locked: true, visible: false },
      ],
    };
    const setObjectsVisible = vi.fn();
    const lockObjects = vi.fn();
    const unlockObjects = vi.fn();
    useProjectStore.setState({
      project,
      selectedObjectIds: ['obj1', 'obj2'],
      setObjectsVisible,
      lockObjects,
      unlockObjects,
    });

    render(<PropertiesPanel />);
    fireEvent.click(screen.getByTestId('batch-visible'));
    fireEvent.click(screen.getByTestId('batch-locked'));

    expect(setObjectsVisible).toHaveBeenCalledWith(['obj1', 'obj2'], true);
    expect(lockObjects).toHaveBeenCalledWith(['obj1', 'obj2']);
    expect(unlockObjects).not.toHaveBeenCalled();
  });
});
