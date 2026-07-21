import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, cleanup, fireEvent, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { LayerList } from '../LayerList';
import { useProjectStore } from '../../../stores/projectStore';
import { useUiStore } from '../../../stores/uiStore';
import { useNotificationStore } from '../../../stores/notificationStore';
import { projectService } from '../../../services/projectService';
import { useAppStore } from '../../../stores/appStore';
import type { Layer, OperationType, ProjectObject } from '../../../types/project';
import { makeAppSettings, makeLayer as makeFixtureLayer, makeProject, makeProjectObject, makeRasterSettings, type LayerFixtureOverrides } from '../../../test-utils/projectFixtures';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockReturnValue(new Promise(() => {})) }));

function makeLayer(overrides: LayerFixtureOverrides = {}): Layer {
  return makeFixtureLayer({
    id: 'layer-1',
    name: 'Cut Layer',
    operation: 'line' as OperationType,
    color_tag: '#FF0000',
    speed_mm_min: 3000,
    power_percent: 80,
    ...overrides,
  });
}

const initialState = useProjectStore.getState();
const initialUiState = useUiStore.getState();
const initialNotificationState = useNotificationStore.getState();
const initialAppState = useAppStore.getState();

afterEach(() => {
  cleanup();
  useProjectStore.setState(initialState, true);
  useUiStore.setState(initialUiState, true);
  useNotificationStore.setState(initialNotificationState, true);
  useAppStore.setState(initialAppState, true);
  vi.restoreAllMocks();
});

describe('LayerList', () => {
  it('renders table header with column labels', () => {
    const layer = makeLayer();
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [], assets: [] }),
    });

    render(<LayerList />);

    const header = screen.getByTestId('layer-table-header');
    expect(header).toBeDefined();
    expect(header.textContent).toContain('#');
    expect(header.textContent).toContain('Mode');
    expect(header.textContent).toContain('Spd/Pwr');
  });

  it('displays layer IDs as C00, C01, T1 based on color', () => {
    const layers = [
      makeLayer({ id: 'l1', name: '', color_tag: '#000000' }), // C00 Black
      makeLayer({ id: 'l2', name: '', color_tag: '#FF0000' }), // C01 Red
      makeLayer({ id: 'l3', name: '', color_tag: '#DA0B3F' }), // T1 Tool
    ];
    useProjectStore.setState({
      project: makeProject({ layers, objects: [], assets: [] }),
    });

    render(<LayerList />);

    const labels = screen.getAllByTestId('layer-label');
    expect(labels[0].textContent).toBe('C00');
    expect(labels[1].textContent).toBe('C01');
    expect(labels[2].textContent).toBe('T1');
  });

  it('mode dropdown changes layer operation', async () => {
    const layer = makeLayer({ id: 'l1', operation: 'line' });
    const updateCutEntrySpy = vi.spyOn(projectService, 'updateCutEntry').mockResolvedValue({
      ...layer.entries[0],
      operation: 'fill',
    });
    const loadProjectSpy = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [], assets: [] }),
      loadProject: loadProjectSpy,
    });

    render(<LayerList />);

    const modeSelect = screen.getByTestId('mode-select');
    expect((modeSelect as HTMLSelectElement).value).toBe('line');

    fireEvent.change(modeSelect, { target: { value: 'fill' } });
    await waitFor(() => {
      expect(updateCutEntrySpy).toHaveBeenCalledWith('l1', layer.entries[0].id, {
        operation: 'fill',
      });
    });
    expect(loadProjectSpy).toHaveBeenCalledWith({ invalidatePreview: true });
  });

  it('image layers render plain-text "Image" label, not a dropdown', () => {
    const layer = makeLayer({ id: 'l1', operation: 'image' });
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [], assets: [] }),
    });

    render(<LayerList />);

    // Image layers must NOT render the mode dropdown — image mode is immutable.
    expect(screen.queryByTestId('mode-select')).toBeNull();
    const label = screen.getByTestId('mode-image-label');
    expect(label.textContent).toBe('Image');
  });

  it('tool layers render as frame-only rows without output or air controls', () => {
    const layer = makeLayer({
      id: 't1',
      name: 'T1',
      color_tag: '#DA0B3F',
      operation: 'tool',
      is_tool_layer: true,
    });
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [], assets: [] }),
      selectedLayerId: 't1',
    });

    render(<LayerList />);

    expect(screen.getByTestId('layer-label').textContent).toBe('T1');
    expect(screen.getByTestId('color-swatch').textContent).toBe('T1');
    expect(screen.getByTestId('mode-tool-label').textContent).toBe('Tool');
    expect(screen.getByTestId('speed-power').textContent).toBe('Frame');
    expect(screen.getByTestId('frame-toggle')).toBeDefined();
    expect(screen.queryByTestId('mode-select')).toBeNull();
    expect(screen.queryByTestId('output-toggle')).toBeNull();
    expect(screen.getByTestId('show-toggle')).toBeDefined();
    expect(screen.queryByTestId('air-toggle')).toBeNull();
    expect(screen.queryByTestId('quick-edit')).toBeNull();
  });

  it('tool layer Frame toggle updates the global job-bounds setting', async () => {
    const layer = makeLayer({
      id: 't1',
      color_tag: '#DA0B3F',
      operation: 'tool',
      is_tool_layer: true,
    });
    const updateSettings = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [], assets: [] }),
      selectedLayerId: 't1',
    });
    useAppStore.setState({
      settings: makeAppSettings({ include_tool_layers_in_job_bounds: true }),
      updateSettings,
    });

    render(<LayerList />);

    fireEvent.click(screen.getByTestId('frame-toggle'));

    await waitFor(() => {
      expect(updateSettings).toHaveBeenCalledWith({
        include_tool_layers_in_job_bounds: false,
      });
    });
  });

  it('double-clicking a tool layer does not open the cut settings editor', () => {
    const layer = makeLayer({
      id: 't1',
      color_tag: '#DA0B3F',
      operation: 'tool',
      is_tool_layer: true,
    });
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [], assets: [] }),
    });

    render(<LayerList />);

    fireEvent.doubleClick(screen.getByTestId('layer-row'));

    expect(screen.queryByTestId('cut-settings-overlay')).toBeNull();
  });

  it('stale auto-generated names collapse to current family label after recolor', () => {
    // Layer was created as C02 (Image) but later recolored to C00's palette color.
    const layer = makeLayer({
      id: 'l1',
      name: 'C02 (Image)',
      operation: 'image',
      color_tag: '#000000', // C00 color
    });
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [], assets: [] }),
    });

    render(<LayerList />);

    // Should display the current family label "C00", not the stale "C02 (Image)"
    const labels = screen.getAllByTestId('layer-label');
    expect(labels[0].textContent).toBe('C00');
  });

  it('keeps Lbrn source palette labels when its palette order differs', () => {
    const blue = makeLayer({
      id: 'lb-blue',
      name: 'C01',
      color_tag: '#0000FF',
      order_index: 1,
    });
    const green = makeLayer({
      id: 'lb-green',
      name: 'C03',
      color_tag: '#00E000',
      order_index: 3,
    });
    useProjectStore.setState({
      project: makeProject({ layers: [blue, green], objects: [] }),
    });

    render(<LayerList />);

    const labels = screen.getAllByTestId('layer-label');
    expect(labels[0].textContent).toBe('C01');
    expect(labels[1].textContent).toBe('C03');
    const swatches = screen.getAllByTestId('color-swatch');
    expect(swatches[0].textContent).toBe('01');
    expect(swatches[1].textContent).toBe('03');
  });

  it('two layers can share the same color tag (Image + Line on C00)', () => {
    const imageLayer = makeLayer({
      id: 'l1',
      name: 'Image',
      operation: 'image',
      color_tag: '#000000',
      order_index: 0,
    });
    const lineLayer = makeLayer({
      id: 'l2',
      name: 'Line',
      operation: 'line',
      color_tag: '#000000',
      order_index: 1,
    });
    useProjectStore.setState({
      project: makeProject({ layers: [imageLayer, lineLayer], objects: [] }),
    });

    render(<LayerList />);

    const labels = screen.getAllByTestId('layer-label');
    // Both rows should display "C00" because both are tagged with the first palette color.
    expect(labels[0].textContent).toBe('C00');
    expect(labels[1].textContent).toBe('C00');
    const swatches = screen.getAllByTestId('color-swatch');
    expect(swatches[0].textContent).toBe('00');
    expect(swatches[1].textContent).toBe('00');
    // Image layer renders plain label, Line layer renders dropdown.
    expect(screen.getByTestId('mode-image-label').textContent).toBe('Image');
    expect(screen.getByTestId('mode-select')).toBeDefined();
  });

  it('collapses auto-generated family names like C00 (Line) back to C00 in the Layer column', () => {
    const imageLayer = makeLayer({
      id: 'l1',
      name: 'C00 (Image)',
      operation: 'image',
      color_tag: '#000000',
      order_index: 0,
    });
    const lineLayer = makeLayer({
      id: 'l2',
      name: 'C00 (Line)',
      operation: 'line',
      color_tag: '#000000',
      order_index: 1,
    });
    useProjectStore.setState({
      project: makeProject({ layers: [imageLayer, lineLayer], objects: [] }),
    });

    render(<LayerList />);

    const labels = screen.getAllByTestId('layer-label');
    expect(labels[0].textContent).toBe('C00');
    expect(labels[1].textContent).toBe('C00');
  });

  it('output toggle circle works', () => {
    const layer = makeLayer({ id: 'l1', enabled: true });
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [] }),
    });

    const updateLayerSpy = vi.fn();
    useProjectStore.setState({ updateLayer: updateLayerSpy });

    render(<LayerList />);

    const outputToggle = screen.getByTestId('output-toggle');
    fireEvent.click(outputToggle);

    expect(updateLayerSpy).toHaveBeenCalledWith('l1', { enabled: false });
  });

  it('show toggle circle works', async () => {
    const layer = makeLayer({ id: 'l1', visible: true });
    const setLayerVisibleSpy = vi.spyOn(projectService, 'setLayerVisible').mockResolvedValue(true);
    const loadProjectSpy = vi.spyOn(useProjectStore.getState(), 'loadProject').mockResolvedValue(undefined);
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [] }),
    });

    render(<LayerList />);

    const showToggle = screen.getByTestId('show-toggle');
    fireEvent.click(showToggle);
    await waitFor(() => {
      expect(setLayerVisibleSpy).toHaveBeenCalledWith('l1', false);
    });
    expect(loadProjectSpy).toHaveBeenCalledWith({ invalidatePreview: true });
  });

  it('show toggle surfaces backend failures', async () => {
    const pushSpy = vi.fn();
    useNotificationStore.setState({ push: pushSpy });
    vi.spyOn(projectService, 'setLayerVisible').mockRejectedValue(new Error('show failed'));

    const layer = makeLayer({ id: 'l1', visible: true });
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [] }),
    });

    render(<LayerList />);
    fireEvent.click(screen.getByTestId('show-toggle'));

    await waitFor(() => {
      expect(pushSpy).toHaveBeenCalledWith(expect.stringContaining('show failed'), 'error');
    });
  });

  it('air toggle circle works', async () => {
    const layer = makeLayer({ id: 'l1', air_assist: false });
    const loadProjectSpy = vi.spyOn(useProjectStore.getState(), 'loadProject').mockResolvedValue(undefined);
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [] }),
    });

    render(<LayerList />);

    const airToggle = screen.getByTestId('air-toggle');
    fireEvent.click(airToggle);
    await waitFor(() => {
      expect(loadProjectSpy).toHaveBeenCalledWith({ invalidatePreview: true });
    });
    loadProjectSpy.mockRestore();
  });

  it('air toggle surfaces backend failures', async () => {
    const pushSpy = vi.fn();
    useNotificationStore.setState({ push: pushSpy });
    vi.spyOn(projectService, 'setLayerAirAssist').mockRejectedValue(new Error('air failed'));

    const layer = makeLayer({ id: 'l1', air_assist: false });
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [] }),
    });

    render(<LayerList />);
    fireEvent.click(screen.getByTestId('air-toggle'));

    await waitFor(() => {
      expect(pushSpy).toHaveBeenCalledWith(expect.stringContaining('air failed'), 'error');
    });
  });

  it('displays speed/power summary', () => {
    const layer = makeLayer({ speed_mm_min: 5000, power_percent: 65 });
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [] }),
    });

    render(<LayerList />);

    const summary = screen.getByTestId('speed-power');
    expect(summary.textContent).toBe('5000/65%');
  });

  it('quick-edit speed field calls updateLayer', () => {
    const layer = makeLayer({ id: 'l1', speed_mm_min: 3000 });
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [] }),
      selectedLayerId: 'l1',
    });

    const updateCutEntrySpy = vi.fn();
    useProjectStore.setState({ updateCutEntry: updateCutEntrySpy });

    render(<LayerList />);

    const quickEdit = screen.getByTestId('quick-edit');
    const inputs = quickEdit.querySelectorAll('input[type="number"]');
    expect(inputs.length).toBeGreaterThanOrEqual(1);

    // First number input is Speed
    fireEvent.change(inputs[0], { target: { value: '4000' } });
    expect(updateCutEntrySpy).toHaveBeenCalledWith('l1', layer.entries[0].id, { speed_mm_min: 4000 });
  });

  it('converts quick-edit speed through the selected speed time unit', () => {
    const layer = makeLayer({ id: 'l1', speed_mm_min: 3000, power_percent: 65 });
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [] }),
      selectedLayerId: 'l1',
    });
    useAppStore.setState({
      settings: makeAppSettings({ speed_time_unit: 'seconds' }),
    });

    const updateCutEntrySpy = vi.fn();
    useProjectStore.setState({ updateCutEntry: updateCutEntrySpy });

    render(<LayerList />);

    expect(screen.getByTestId('speed-power').textContent).toBe('50/65%');
    expect(screen.getByText('Speed (mm/sec)')).toBeDefined();

    const quickEdit = screen.getByTestId('quick-edit');
    const inputs = quickEdit.querySelectorAll('input[type="number"]');
    expect((inputs[0] as HTMLInputElement).value).toBe('50');

    fireEvent.change(inputs[0], { target: { value: '75' } });
    expect(updateCutEntrySpy).toHaveBeenCalledWith('l1', layer.entries[0].id, { speed_mm_min: 4500 });
  });

  it('quick-edit interval field updates raster_settings for image layers', () => {
    const layer = makeLayer({
      id: 'img1',
      operation: 'image',
      raster_settings: makeRasterSettings({
        mode: 'grayscale',
        overscan_mm: 0,
        line_interval_mm: 0.1,
      }),
    });
    const updateCutEntrySpy = vi.fn();
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [] }),
      selectedLayerId: 'img1',
      updateCutEntry: updateCutEntrySpy,
    });

    render(<LayerList />);

    const quickEdit = screen.getByTestId('quick-edit');
    const inputs = quickEdit.querySelectorAll('input[type="number"]');
    expect(inputs.length).toBeGreaterThanOrEqual(5);

    // Speed, Passes, Max Pwr, Interval, Min Pwr
    fireEvent.change(inputs[3], { target: { value: '0.2' } });

    expect(updateCutEntrySpy).toHaveBeenCalledWith('img1', layer.entries[0].id, {
      raster_settings: expect.objectContaining({
        line_interval_mm: 0.2,
        dpi: 127,
      }),
    });
  });

  it('double-click opens CutSettingsEditor dialog', () => {
    const layer = makeLayer({ id: 'l1', name: 'Cut Layer' });
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [] }),
    });

    render(<LayerList />);

    expect(screen.queryByTestId('cut-settings-overlay')).toBeNull();

    const layerRow = screen.getByTestId('layer-row');
    fireEvent.doubleClick(layerRow);

    expect(screen.getByTestId('cut-settings-overlay')).toBeDefined();
    expect(screen.getByTestId('layer-name-input')).toBeDefined();
  });

  it('color swatch displays the layer color (read-only)', () => {
    const layer = makeLayer({ id: 'l1', color_tag: '#FF0000' });
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [] }),
    });

    render(<LayerList />);

    const swatch = screen.getByTestId('color-swatch');
    expect(swatch.style.backgroundColor).toBe('rgb(255, 0, 0)');
    // Swatch is a div, not a button — not clickable
    expect(swatch.tagName).toBe('DIV');
  });

  it('shows empty state when no layers', () => {
    useProjectStore.setState({
      project: makeProject({ layers: [], objects: [] }),
    });

    render(<LayerList />);
    const emptyRow = screen.getByTestId('empty-layer-row');
    expect(emptyRow).toBeDefined();
    expect(emptyRow.textContent).toContain('Draw or import to create a layer');
  });

  it('setPassCount updates vector_settings.passes for line layers via updateCutEntry', async () => {
    const layer = makeLayer({
      id: 'l1',
      operation: 'line',
      raster_settings: null,
      vector_settings: {
        passes: 1,
        perforation_enabled: false,
        perforation_on_ms: 0,
        perforation_off_ms: 0,
      },
    });
    const loadProjectSpy = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [] }),
      selectedLayerId: 'l1',
      loadProject: loadProjectSpy,
    });

    render(<LayerList />);

    const quickEdit = screen.getByTestId('quick-edit');
    const inputs = quickEdit.querySelectorAll('input[type="number"]');
    // Passes input is the second number field (after Speed)
    const passesInput = inputs[1];
    expect(passesInput).toBeDefined();

    fireEvent.change(passesInput, { target: { value: '3' } });

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('update_cut_entry', {
        layerId: 'l1',
        entryId: layer.entries[0].id,
        patch: expect.objectContaining({
          vector_settings: expect.objectContaining({
            passes: 3,
          }),
        }),
      });
    });
    expect(loadProjectSpy).toHaveBeenCalledWith({ invalidatePreview: true });
  });

  it('mode dropdown surfaces backend failures', async () => {
    const pushSpy = vi.fn();
    useNotificationStore.setState({ push: pushSpy });
    vi.spyOn(projectService, 'updateCutEntry').mockRejectedValue(new Error('mode failed'));

    const layer = makeLayer({ id: 'l1', operation: 'line' });
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [] }),
    });

    render(<LayerList />);
    fireEvent.change(screen.getByTestId('mode-select'), { target: { value: 'fill' } });

    await waitFor(() => {
      expect(pushSpy).toHaveBeenCalledWith(expect.stringContaining('mode failed'), 'error');
    });
  });

  it('vector pass quick-edit surfaces failures and reloads layer state', async () => {
    const pushSpy = vi.fn();
    const loadProjectSpy = vi.fn().mockResolvedValue(undefined);
    useNotificationStore.setState({ push: pushSpy });
    useProjectStore.setState({ loadProject: loadProjectSpy });
    vi.spyOn(projectService, 'updateCutEntry').mockRejectedValue(new Error('passes failed'));

    const layer = makeLayer({
      id: 'l1',
      operation: 'line',
      raster_settings: null,
      vector_settings: {
        passes: 1,
        perforation_enabled: false,
        perforation_on_ms: 0,
        perforation_off_ms: 0,
      },
    });
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [] }),
      selectedLayerId: 'l1',
    });

    render(<LayerList />);
    const inputs = screen.getByTestId('quick-edit').querySelectorAll('input[type="number"]');
    fireEvent.change(inputs[1], { target: { value: '3' } });

    await waitFor(() => {
      expect(pushSpy).toHaveBeenCalledWith(expect.stringContaining('passes failed'), 'error');
      expect(loadProjectSpy).toHaveBeenCalledWith(undefined);
    });
  });

  it('vector quick-edit setting failures are caught and resync the layer panel', async () => {
    const pushSpy = vi.fn();
    const loadProjectSpy = vi.fn().mockResolvedValue(undefined);
    useNotificationStore.setState({ push: pushSpy });
    useProjectStore.setState({ loadProject: loadProjectSpy });
    vi.spyOn(projectService, 'updateCutEntry').mockRejectedValue(new Error('vector failed'));

    const layer = makeLayer({
      id: 'l1',
      operation: 'line',
      raster_settings: null,
      vector_settings: {
        passes: 1,
        perforation_enabled: false,
        perforation_on_ms: 0,
        perforation_off_ms: 0,
      },
    });
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [] }),
      selectedLayerId: 'l1',
    });

    render(<LayerList />);
    fireEvent.click(screen.getByTestId('perf-toggle'));

    await waitFor(() => {
      expect(pushSpy).toHaveBeenCalledWith(expect.stringContaining('vector failed'), 'error');
      expect(loadProjectSpy).toHaveBeenCalledWith(undefined);
    });
  });

  it('right-click opens context menu with Disable and Select All', async () => {
    const layer = makeLayer({ id: 'l1', enabled: true, visible: true });
    const obj: ProjectObject = makeProjectObject({
      id: 'obj-1',
      name: 'Rect',
      layer_id: 'l1',
      data: { type: 'shape', kind: 'rectangle', width: 10, height: 10, corner_radius: 0 },
    });
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [obj] }),
    });

    render(<LayerList />);

    const layerRow = screen.getByTestId('layer-row');
    fireEvent.contextMenu(layerRow);

    // Context menu renders via microtask so we need to wait
    await waitFor(() => {
      expect(screen.getByTestId('context-menu')).toBeDefined();
    });

    expect(screen.getByText('Disable')).toBeDefined();
    expect(screen.getByText('Hide')).toBeDefined();
    expect(screen.getByText('Select All on Layer')).toBeDefined();
  });

  it('shift-left-click selects every object on the layer', () => {
    const layer = makeLayer({ id: 'l1' });
    const obj1: ProjectObject = makeProjectObject({
      id: 'obj-1',
      name: 'R1',
      layer_id: 'l1',
      data: { type: 'shape', kind: 'rectangle', width: 10, height: 10, corner_radius: 0 },
    });
    const obj2: ProjectObject = { ...obj1, id: 'obj-2', name: 'R2', created_at: '2026-01-01T00:00:01Z' };
    const obj3: ProjectObject = { ...obj1, id: 'obj-3', name: 'R3', layer_id: 'other-layer', created_at: '2026-01-01T00:00:02Z' };
    const selectObjectsSpy = vi.fn();
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [obj1, obj2, obj3] }),
      selectObjects: selectObjectsSpy,
    });

    render(<LayerList />);

    fireEvent.click(screen.getByTestId('layer-row'), { shiftKey: true });

    expect(selectObjectsSpy).toHaveBeenCalledWith(['obj-2', 'obj-1']);
  });

  it('shift-right-click flashes the layer without opening the context menu', async () => {
    const layer = makeLayer({ id: 'l1' });
    const flashLayerSpy = vi.fn();
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [] }),
    });
    useUiStore.setState({ flashLayer: flashLayerSpy });

    render(<LayerList />);

    fireEvent.contextMenu(screen.getByTestId('layer-row'), { shiftKey: true });

    expect(flashLayerSpy).toHaveBeenCalledWith('l1');
    await Promise.resolve();
    expect(screen.queryByTestId('context-menu')).toBeNull();
  });

  it('Disable toggles layer enabled via context menu', async () => {
    const layer = makeLayer({ id: 'l1', enabled: true });
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [] }),
    });

    const updateLayerSpy = vi.fn();
    useProjectStore.setState({ updateLayer: updateLayerSpy });

    render(<LayerList />);

    const layerRow = screen.getByTestId('layer-row');
    fireEvent.contextMenu(layerRow);

    await waitFor(() => {
      expect(screen.getByTestId('context-menu')).toBeDefined();
    });

    fireEvent.click(screen.getByText('Disable'));
    expect(updateLayerSpy).toHaveBeenCalledWith('l1', { enabled: false });
  });

  it('keeps the rename input open when layer rename fails', async () => {
    const layer = makeLayer({ id: 'l1', name: 'Original Name' });
    const updateLayerSpy = vi.fn().mockResolvedValue(false);
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [] }),
      updateLayer: updateLayerSpy,
    });

    render(<LayerList />);

    fireEvent.contextMenu(screen.getByTestId('layer-row'));
    await waitFor(() => {
      expect(screen.getByTestId('context-menu')).toBeDefined();
    });
    fireEvent.click(screen.getByText('Rename'));

    const input = screen.getByTestId('rename-input');
    fireEvent.change(input, { target: { value: 'Renamed Layer' } });
    fireEvent.keyDown(input, { key: 'Enter' });

    await waitFor(() => {
      expect(updateLayerSpy).toHaveBeenCalledWith('l1', { name: 'Renamed Layer' });
    });
    expect(screen.getByTestId('rename-input')).toBeDefined();
    expect((screen.getByTestId('rename-input') as HTMLInputElement).value).toBe('Renamed Layer');
  });

  it('locks a layer through the batch lock action instead of per-object updates', () => {
    const layer = makeLayer({ id: 'l1' });
    const obj1: ProjectObject = makeProjectObject({
      id: 'obj-1',
      name: 'R1',
      layer_id: 'l1',
      data: { type: 'shape', kind: 'rectangle', width: 10, height: 10, corner_radius: 0 },
    });
    const obj2: ProjectObject = { ...obj1, id: 'obj-2', name: 'R2' };
    const lockObjectsSpy = vi.fn();
    const updateObjectSpy = vi.fn();
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [obj1, obj2] }),
      selectedLayerId: 'l1',
      lockObjects: lockObjectsSpy,
      updateObject: updateObjectSpy,
    });

    render(<LayerList />);

    fireEvent.click(screen.getByTestId('lock-layer'));

    expect(lockObjectsSpy).toHaveBeenCalledWith(['obj-1', 'obj-2']);
    expect(updateObjectSpy).not.toHaveBeenCalled();
  });

  it('M4: pastes the full entries[] stack via paste_layer_entries (not the old narrow updateCutEntry)', async () => {
    const srcLayer = makeLayer({
      id: 'l1',
      operation: 'line',
      speed_mm_min: 3200,
      power_percent: 55,
      air_assist: true,
    });
    const dstLayer = makeLayer({ id: 'l2', name: 'Target', order_index: 1, air_assist: false });
    const pasteLayerEntriesSpy = vi
      .spyOn(projectService, 'pasteLayerEntries')
      .mockResolvedValue({
        ...dstLayer,
        entries: [
          {
            ...dstLayer.entries[0],
            id: 'fresh-id', // backend mints new ids
            operation: 'line',
            speed_mm_min: 3200,
            power_percent: 55,
            air_assist: true,
          },
        ],
      });
    const updateCutEntrySpy = vi.spyOn(projectService, 'updateCutEntry');
    useProjectStore.setState({
      project: makeProject({ layers: [srcLayer, dstLayer], objects: [] }),
    });

    render(<LayerList />);

    const rows = screen.getAllByTestId('layer-row');
    fireEvent.contextMenu(rows[0]);
    await waitFor(() => {
      expect(screen.getByText('Copy Settings')).toBeDefined();
    });
    fireEvent.click(screen.getByText('Copy Settings'));

    fireEvent.contextMenu(rows[1]);
    await waitFor(() => {
      expect(screen.getByText('Paste Settings')).toBeDefined();
    });
    fireEvent.click(screen.getByText('Paste Settings'));

    await waitFor(() => {
      expect(pasteLayerEntriesSpy).toHaveBeenCalledTimes(1);
      const [layerId, templates] = pasteLayerEntriesSpy.mock.calls[0];
      expect(layerId).toBe('l2');
      // Full stack — no ids in the template (backend mints fresh ones).
      expect(Array.isArray(templates)).toBe(true);
      expect(templates).toHaveLength(srcLayer.entries.length);
      expect((templates[0] as { id?: string }).id).toBeUndefined();
      expect(templates[0]).toMatchObject({
        operation: 'line',
        speed_mm_min: 3200,
        power_percent: 55,
        air_assist: true,
      });
    });
    // Old narrow path must not be called.
    expect(updateCutEntrySpy).not.toHaveBeenCalled();
  });

  it('reports an error when toggling visibility fails', async () => {
    const layer = makeLayer({ id: 'l1', visible: true });
    vi.spyOn(projectService, 'setLayerVisible').mockRejectedValue(new Error('visibility failed'));
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [] }),
    });

    render(<LayerList />);

    fireEvent.click(screen.getByTestId('show-toggle'));

    await waitFor(() => {
      const notifications = useNotificationStore.getState().notifications;
      expect(notifications[notifications.length - 1]?.message).toContain('Failed to update layer visibility');
    });
  });

  it('reports an error when toggling air assist fails', async () => {
    const layer = makeLayer({ id: 'l1', air_assist: false });
    vi.spyOn(projectService, 'setLayerAirAssist').mockRejectedValue(new Error('air failed'));
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [] }),
    });

    render(<LayerList />);

    fireEvent.click(screen.getByTestId('air-toggle'));

    await waitFor(() => {
      const notifications = useNotificationStore.getState().notifications;
      expect(notifications[notifications.length - 1]?.message).toContain('Failed to update layer air assist');
    });
  });

  it('reports an error when changing mode fails', async () => {
    const layer = makeLayer({ id: 'l1', operation: 'line' });
    vi.spyOn(projectService, 'updateCutEntry').mockRejectedValue(new Error('mode failed'));
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [] }),
    });

    render(<LayerList />);

    fireEvent.change(screen.getByTestId('mode-select'), { target: { value: 'fill' } });

    await waitFor(() => {
      const notifications = useNotificationStore.getState().notifications;
      expect(notifications[notifications.length - 1]?.message).toContain('Failed to update layer mode');
    });
  });

  it('Select All on Layer selects correct objects', async () => {
    const layer = makeLayer({ id: 'l1' });
    const obj1: ProjectObject = makeProjectObject({
      id: 'obj-1',
      name: 'R1',
      layer_id: 'l1',
      data: { type: 'shape', kind: 'rectangle', width: 10, height: 10, corner_radius: 0 },
    });
    const obj2: ProjectObject = { ...obj1, id: 'obj-2', name: 'R2', created_at: '2026-01-01T00:00:01Z' };
    const obj3: ProjectObject = { ...obj1, id: 'obj-3', name: 'R3', layer_id: 'other-layer', created_at: '2026-01-01T00:00:02Z' };
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [obj1, obj2, obj3] }),
    });

    const selectObjectsSpy = vi.fn();
    useProjectStore.setState({ selectObjects: selectObjectsSpy });

    render(<LayerList />);

    const layerRow = screen.getByTestId('layer-row');
    fireEvent.contextMenu(layerRow);

    await waitFor(() => {
      expect(screen.getByTestId('context-menu')).toBeDefined();
    });

    fireEvent.click(screen.getByText('Select All on Layer'));
    expect(selectObjectsSpy).toHaveBeenCalledWith(['obj-1', 'obj-2']);
  });
});
