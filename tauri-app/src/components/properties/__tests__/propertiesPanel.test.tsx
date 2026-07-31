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
  it('shows next-text settings when the text tool is active without a selection', () => {
    useProjectStore.setState({ project: makeProjectFixture({ objects: [] }), selectedObjectIds: [] });
    useUiStore.setState({ activeTool: 'text' });

    render(<PropertiesPanel />);

    expect(screen.getByTestId('text-defaults-card')).toBeDefined();
    expect(screen.getByText('Choose Point to click and type, or Box to drag a wrapping region.')).toBeDefined();
    expect(screen.getByRole('button', { name: 'Point' }).getAttribute('aria-pressed')).toBe('true');
  });

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

  it('keeps the object lock control out of the property list', () => {
    useProjectStore.setState({ project: makeProject(), selectedObjectIds: ['obj1'] });
    render(<PropertiesPanel />);
    expect(screen.queryByText('Locked')).toBeNull();
  });

  it('uses the cyan inspector card and shared power slider', () => {
    const updateObject = vi.fn();
    useProjectStore.setState({
      project: makeProject({ power_scale: 0.5 }),
      selectedObjectIds: ['obj1'],
      updateObject,
    });

    render(<PropertiesPanel />);

    expect(screen.getByTestId('properties-card').className).toContain('border-bb-accent/40');
    const powerSlider = screen.getByTestId('properties-power-scale-slider') as HTMLInputElement;
    expect(powerSlider.type).toBe('range');
    expect(powerSlider.value).toBe('50');

    fireEvent.change(powerSlider, { target: { value: '65' } });
    expect(updateObject).toHaveBeenCalledWith('obj1', { power_scale: 0.65 });
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
    expect(screen.getByText(/Box width/)).toBeDefined();
    expect(screen.getByText('Squeeze to fit')).toBeDefined();
    expect(screen.queryByText('Ignore Empty Vars')).toBeNull();
    expect(screen.getByText('RTL')).toBeDefined();
    expect(screen.queryByText('Path Offset')).toBeNull();
    expect(screen.queryByText('Bend Radius')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Font: sans-serif' }));
    expect(await screen.findByRole('option', { name: 'Noto Sans CJK SC' })).toBeDefined();
  });

  it('keeps the shared object flow before text-specific controls', () => {
    const project = makeProject({ data: makeTextObjectData({ content: 'Hello' }) });
    useProjectStore.setState({ project, selectedObjectIds: ['obj1'] });

    render(<PropertiesPanel />);

    const textPanel = screen.getByTestId('text-properties-panel');
    const transformPanel = screen.getByTestId('transform-section');
    const powerSlider = screen.getByTestId('properties-power-scale-slider');
    expect(transformPanel.compareDocumentPosition(powerSlider) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(powerSlider.compareDocumentPosition(textPanel) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it('loads system fonts in the text properties panel', async () => {
    const project = makeProject({
      data: makeTextObjectData({ font_family: 'Arial' }),
    });
    useProjectStore.setState({ project, selectedObjectIds: ['obj1'] });

    render(<PropertiesPanel />);

    fireEvent.click(screen.getByRole('button', { name: /^Font:/ }));
    expect(await screen.findByRole('option', { name: 'Noto Sans CJK SC' })).toBeDefined();
  });

  it('shows missing glyph warnings for text objects', async () => {
    const project = makeProject({
      data: makeTextObjectData({ missing_glyphs: ['中'] }),
    });
    useProjectStore.setState({ project, selectedObjectIds: ['obj1'] });

    render(<PropertiesPanel />);

    expect(screen.getByText(/Missing glyphs: 中/)).toBeDefined();
    fireEvent.click(screen.getByRole('button', { name: /^Font:/ }));
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
    fireEvent.click(screen.getByRole('button', { name: 'Path' }));

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

  it('does not duplicate layer assignment controls in object properties', () => {
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
    useProjectStore.setState({ project, selectedObjectIds: ['obj1', 'obj2'] });

    render(<PropertiesPanel />);
    expect(screen.queryByLabelText('Layer')).toBeNull();
  });

  it('routes multi-select visibility through the batch store action', () => {
    const base = makeProject();
    const project = {
      ...base,
      objects: [
        base.objects[0],
        { ...base.objects[0], id: 'obj2', name: 'Rect2', locked: true, visible: false },
      ],
    };
    const setObjectsVisible = vi.fn();
    useProjectStore.setState({
      project,
      selectedObjectIds: ['obj1', 'obj2'],
      setObjectsVisible,
    });

    render(<PropertiesPanel />);
    fireEvent.click(screen.getByTestId('batch-visible'));

    expect(setObjectsVisible).toHaveBeenCalledWith(['obj1', 'obj2'], true);
    expect(screen.queryByTestId('batch-locked')).toBeNull();
  });

  it('shows Group and Align actions for a multi-selection', () => {
    const base = makeProject();
    const project = {
      ...base,
      objects: [
        base.objects[0],
        { ...base.objects[0], id: 'obj2', name: 'Rect2' },
      ],
    };
    const groupObjects = vi.fn().mockResolvedValue(undefined);
    const alignObjects = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({
      project,
      selectedObjectIds: ['obj1', 'obj2'],
      groupObjects,
      alignObjects,
    });

    render(<PropertiesPanel />);
    const groupButton = screen.getByRole('button', { name: 'Group' });
    const ungroupButton = screen.getByRole('button', { name: 'Ungroup' });
    expect(groupButton.className).toContain('w-7');
    expect(ungroupButton.className).toContain('w-7');
    expect((ungroupButton as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(groupButton);
    fireEvent.click(screen.getByRole('button', { name: 'Align Left' }));

    expect(groupObjects).toHaveBeenCalledWith(['obj1', 'obj2']);
    expect(alignObjects).toHaveBeenCalledWith(['obj1', 'obj2'], 'left');
  });

  it('shows the complete icon-based Boolean operation set and routes each action', () => {
    const base = makeProject();
    const project = {
      ...base,
      objects: [
        base.objects[0],
        { ...base.objects[0], id: 'obj2', name: 'Rect2' },
      ],
    };
    const booleanWeld = vi.fn().mockResolvedValue(undefined);
    const booleanUnion = vi.fn().mockResolvedValue(undefined);
    const booleanSubtract = vi.fn().mockResolvedValue(undefined);
    const booleanIntersection = vi.fn().mockResolvedValue(undefined);
    const booleanExclude = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({
      project,
      selectedObjectIds: ['obj1', 'obj2'],
      booleanWeld,
      booleanUnion,
      booleanSubtract,
      booleanIntersection,
      booleanExclude,
    });

    render(<PropertiesPanel />);

    const operationNames = ['Union', 'Subtract A − (B…)', 'Subtract B − (A…)', 'Intersect', 'Exclude'];
    operationNames.forEach((name) => {
      const button = screen.getByRole('button', { name });
      expect(button.className).toContain('w-9');
      expect(button.querySelector('svg')).not.toBeNull();
      fireEvent.click(button);
    });

    expect(booleanWeld).not.toHaveBeenCalled();
    expect(booleanUnion).toHaveBeenCalledWith('obj1', 'obj2');
    expect(booleanSubtract).toHaveBeenNthCalledWith(1, 'obj1', 'obj2');
    expect(booleanSubtract).toHaveBeenNthCalledWith(2, 'obj2', 'obj1');
    expect(booleanIntersection).toHaveBeenCalledWith('obj1', 'obj2');
    expect(booleanExclude).toHaveBeenCalledWith('obj1', 'obj2');
  });

  it('supports all boolean operations for three or more selected tool-layer shapes', () => {
    const base = makeProject();
    const toolLayer = makeLayer({
      id: 'tool',
      name: 'T1',
      operation: 'tool',
      color_tag: '#DA0B3F',
      is_tool_layer: true,
    });
    const objects = ['obj1', 'obj2', 'obj3'].map((id, index) => makeProjectObject({
      ...base.objects[0],
      id,
      name: `Circle ${index + 1}`,
      layer_id: toolLayer.id,
    }));
    const booleanUnionMany = vi.fn().mockResolvedValue(undefined);
    const booleanSubtractMany = vi.fn().mockResolvedValue(undefined);
    const booleanIntersectionMany = vi.fn().mockResolvedValue(undefined);
    const booleanExcludeMany = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({
      project: { ...base, layers: [toolLayer], objects },
      selectedObjectIds: objects.map((object) => object.id),
      booleanUnionMany,
      booleanSubtractMany,
      booleanIntersectionMany,
      booleanExcludeMany,
    });

    render(<PropertiesPanel />);

    fireEvent.click(screen.getByRole('button', { name: 'Union' }));
    fireEvent.click(screen.getByRole('button', { name: 'Subtract A − (B…)' }));
    fireEvent.click(screen.getByRole('button', { name: 'Subtract B − (A…)' }));
    fireEvent.click(screen.getByRole('button', { name: 'Intersect' }));
    fireEvent.click(screen.getByRole('button', { name: 'Exclude' }));

    expect(booleanUnionMany).toHaveBeenCalledWith(['obj1', 'obj2', 'obj3']);
    expect(booleanSubtractMany).toHaveBeenNthCalledWith(1, ['obj1', 'obj2', 'obj3']);
    expect(booleanSubtractMany).toHaveBeenNthCalledWith(2, ['obj3', 'obj1', 'obj2']);
    expect(booleanIntersectionMany).toHaveBeenCalledWith(['obj1', 'obj2', 'obj3']);
    expect(booleanExcludeMany).toHaveBeenCalledWith(['obj1', 'obj2', 'obj3']);
  });

  it('offers inside and outside masks for an image selected with a closed shape', () => {
    const base = makeProject();
    const image = makeProjectObject({
      id: 'image1',
      name: 'Image',
      layer_id: 'l1',
      data: {
        type: 'raster_image' as const,
        asset_key: 'asset1',
        original_width_px: 100,
        original_height_px: 100,
        masks: [],
      },
    });
    const assignImageMask = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({
      project: { ...base, objects: [image, base.objects[0]] },
      selectedObjectIds: ['image1', 'obj1'],
      assignImageMask,
    });

    render(<PropertiesPanel />);

    expect(screen.getByTestId('image-mask-section')).toBeDefined();
    expect(screen.queryByText('Boolean Operations')).toBeNull();

    fireEvent.click(screen.getByRole('button', { name: 'Keep Inside' }));
    fireEvent.click(screen.getByRole('button', { name: 'Keep Outside' }));

    expect(assignImageMask).toHaveBeenNthCalledWith(1, 'image1', ['obj1'], 'keep_inside');
    expect(assignImageMask).toHaveBeenNthCalledWith(2, 'image1', ['obj1'], 'keep_outside');
  });

  it('explains why an open path cannot be used as an image mask', () => {
    const base = makeProject({
      data: { type: 'vector_path' as const, path_data: 'M0 0L10 10', closed: false },
    });
    const image = makeProjectObject({
      id: 'image1',
      name: 'Image',
      layer_id: 'l1',
      data: {
        type: 'raster_image' as const,
        asset_key: 'asset1',
        original_width_px: 100,
        original_height_px: 100,
        masks: [],
      },
    });
    useProjectStore.setState({
      project: { ...base, objects: [image, base.objects[0]] },
      selectedObjectIds: ['image1', 'obj1'],
    });

    render(<PropertiesPanel />);

    expect(screen.getByText('Image masks require closed vector shapes')).toBeDefined();
    expect((screen.getByRole('button', { name: 'Keep Inside' }) as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByRole('button', { name: 'Keep Outside' }) as HTMLButtonElement).disabled).toBe(true);
  });

  it('only shows Distribute actions when at least three objects are selected', () => {
    const base = makeProject();
    const project = {
      ...base,
      objects: [
        base.objects[0],
        { ...base.objects[0], id: 'obj2', name: 'Rect2' },
        { ...base.objects[0], id: 'obj3', name: 'Rect3' },
      ],
    };
    const distributeObjects = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({
      project,
      selectedObjectIds: ['obj1', 'obj2', 'obj3'],
      distributeObjects,
    });

    render(<PropertiesPanel />);
    fireEvent.click(screen.getByRole('button', { name: 'Distribute H-Centered' }));

    expect(distributeObjects).toHaveBeenCalledWith(['obj1', 'obj2', 'obj3'], 'h_centered');
  });

  it('shows Ungroup when a group object is selected', () => {
    const project = makeProject({
      data: { type: 'group' as const, children: ['child-a', 'child-b'] },
    });
    const ungroupObjects = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({
      project,
      selectedObjectIds: ['obj1'],
      ungroupObjects,
    });

    render(<PropertiesPanel />);
    const groupButton = screen.getByRole('button', { name: 'Group' }) as HTMLButtonElement;
    const ungroupButton = screen.getByRole('button', { name: 'Ungroup' }) as HTMLButtonElement;
    expect(groupButton.disabled).toBe(true);
    expect(ungroupButton.disabled).toBe(false);
    fireEvent.click(ungroupButton);

    expect(ungroupObjects).toHaveBeenCalledWith('obj1');
  });
});
