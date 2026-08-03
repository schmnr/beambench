import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, cleanup, fireEvent, waitFor } from '@testing-library/react';
import { LayerList } from '../LayerList';
import { LayerTabs } from '../LayerTabs';
import { useProjectStore } from '../../../stores/projectStore';
import { useUiStore } from '../../../stores/uiStore';
import { useNotificationStore } from '../../../stores/notificationStore';
import { projectService } from '../../../services/projectService';
import { useAppStore } from '../../../stores/appStore';
import type { Layer, OperationType, ProjectObject } from '../../../types/project';
import { makeAppSettings, makeLayer as makeFixtureLayer, makeProject, makeProjectObject, type LayerFixtureOverrides } from '../../../test-utils/projectFixtures';

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
  it('does not expose layer creation in the Run workspace', () => {
    useUiStore.setState({ workspaceMode: 'run' });
    useProjectStore.setState({
      project: makeProject({ layers: [makeLayer({ id: 'l1' })], objects: [] }),
    });

    render(<LayerTabs />);

    expect(screen.queryByTestId('add-layer-tab')).toBeNull();
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

    render(<LayerTabs />);

    const labels = screen.getAllByTestId('tab-label');
    expect(labels[0].textContent).toBe('C00');
    expect(labels[1].textContent).toBe('C01');
    expect(labels[2].textContent).toBe('T1');
  });

  it('keeps inactive layer and add-tab surfaces fully opaque', () => {
    const layers = [
      makeLayer({ id: 'l1', color_tag: '#000000' }),
      makeLayer({ id: 'l2', color_tag: '#FF0000', order_index: 1 }),
    ];
    useUiStore.setState({ workspaceMode: 'design' });
    useProjectStore.setState({
      project: makeProject({ layers, objects: [] }),
      selectedLayerId: 'l1',
    });

    render(<LayerTabs />);

    const inactiveTab = screen.getAllByTestId('layer-tab')[1];
    const addTab = screen.getByTestId('add-layer-tab');
    expect(inactiveTab.className).not.toContain('opacity-80');
    expect(addTab.className).not.toContain('opacity-80');
    expect(inactiveTab.style.background).not.toBe('transparent');
    expect(addTab.style.background).not.toBe('transparent');
  });

  it('tool layers retain the standard color editor while showing tool-only controls', () => {
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

    render(<><LayerTabs /><LayerList /></>);

    expect(screen.getByTestId('tab-label').textContent).toBe('T1');
    expect(screen.queryByTestId('frame-toggle')).toBeNull();
    expect(screen.getByTestId('show-toggle')).toBeDefined();
    expect(screen.queryByTestId('output-toggle')).toBeNull();
    expect(screen.queryByTestId('air-toggle')).toBeNull();
    expect(screen.getByTestId('quick-edit')).toBeDefined();
    expect(screen.getByTestId('quick-edit-color')).toBeDefined();
  });

  it('lets a tool layer change to a normal layer color or another tool color', () => {
    const layer = makeLayer({
      id: 't1',
      name: 'T1',
      color_tag: '#DA0B3F',
      operation: 'tool',
      is_tool_layer: true,
    });
    const updateLayerSpy = vi.fn().mockResolvedValue(true);
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [], assets: [] }),
      selectedLayerId: 't1',
      updateLayer: updateLayerSpy,
    });

    const { rerender } = render(<LayerList />);
    fireEvent.click(screen.getByTestId('quick-edit-color'));
    fireEvent.click(screen.getByTitle('Black'));
    expect(updateLayerSpy).toHaveBeenLastCalledWith('t1', { color_tag: '#000000' });

    rerender(<LayerList />);
    fireEvent.click(screen.getByTestId('quick-edit-color'));
    fireEvent.click(screen.getByTitle('Tool 2'));
    expect(updateLayerSpy).toHaveBeenLastCalledWith('t1', { color_tag: '#00D4FF' });
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

    render(<LayerTabs />);

    fireEvent.doubleClick(screen.getByTestId('layer-tab'));

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

    render(<LayerTabs />);

    // Should display the current family label "C00", not the stale "C02 (Image)"
    const labels = screen.getAllByTestId('tab-label');
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

    render(<LayerTabs />);

    const labels = screen.getAllByTestId('tab-label');
    expect(labels[0].textContent).toBe('C01');
    expect(labels[1].textContent).toBe('C03');
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

    render(<><LayerTabs /><LayerList /></>);

    const labels = screen.getAllByTestId('tab-label');
    // Both rows should display "C00" because both are tagged with the first palette color.
    expect(labels[0].textContent).toBe('C00');
    expect(labels[1].textContent).toBe('C00');
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

    render(<LayerTabs />);

    const labels = screen.getAllByTestId('tab-label');
    expect(labels[0].textContent).toBe('C00');
    expect(labels[1].textContent).toBe('C00');
  });

  it('shows a distinct live mode icon for Line, Fill, Offset Fill, and Image layers', () => {
    const layers = [
      makeLayer({ id: 'line', name: 'C00 (Line)', operation: 'line', color_tag: '#000000' }),
      makeLayer({ id: 'fill', name: 'C01 (Fill)', operation: 'fill', color_tag: '#ff0000', order_index: 1 }),
      makeLayer({ id: 'offset', name: 'C02 (Offset Fill)', operation: 'offset_fill', color_tag: '#00ff00', order_index: 2 }),
      makeLayer({ id: 'image', name: 'C03 (Image)', operation: 'image', color_tag: '#0000ff', order_index: 3 }),
    ];
    useProjectStore.setState({
      project: makeProject({ layers, objects: [] }),
    });

    render(<LayerTabs />);

    expect(screen.getAllByTestId('tab-mode-icon').map((icon) => icon.getAttribute('data-operation'))).toEqual([
      'line',
      'fill',
      'offset_fill',
      'image',
    ]);
  });

  it('uses a pressed lightning icon to control layer output', () => {
    const layer = makeLayer({ id: 'l1', enabled: true });
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [] }),
    });

    const updateLayerSpy = vi.fn();
    useProjectStore.setState({ updateLayer: updateLayerSpy });

    render(<LayerList />);

    const outputToggle = screen.getByTestId('output-toggle');
    expect(outputToggle.getAttribute('aria-pressed')).toBe('true');
    expect(outputToggle.querySelector('.lucide-zap')).not.toBeNull();
    expect(screen.getByTestId('layer-output-show-help').textContent).toContain('Out includes this layer in the laser job');
    fireEvent.click(outputToggle);

    expect(updateLayerSpy).toHaveBeenCalledWith('l1', { enabled: false });
  });

  it('commits a layer name once after editing instead of on every keystroke', async () => {
    const layer = makeLayer({ id: 'l1', name: 'Original' });
    const updateLayerSpy = vi.fn().mockResolvedValue(true);
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [] }),
      selectedLayerId: 'l1',
      updateLayer: updateLayerSpy,
    });

    render(<LayerList />);
    const input = screen.getByTestId('layer-quick-name-input');
    fireEvent.change(input, { target: { value: 'Updated' } });

    expect(updateLayerSpy).not.toHaveBeenCalled();
    fireEvent.blur(input);
    await waitFor(() => {
      expect(updateLayerSpy).toHaveBeenCalledTimes(1);
    });
    expect(updateLayerSpy).toHaveBeenCalledWith('l1', { name: 'Updated' });
  });

  it('activates a tab and reveals its relocated Layers panel without reassigning the current selection', () => {
    const source = makeLayer({ id: 'l1', name: 'Source' });
    const target = makeLayer({ id: 'l2', name: 'Target', order_index: 1 });
    const object = makeProjectObject({ id: 'obj-1', layer_id: source.id });
    const reassignLayerSpy = vi.fn();
    useUiStore.setState({ workspaceMode: 'design' });
    useProjectStore.setState({
      project: makeProject({ layers: [source, target], objects: [object] }),
      selectedLayerId: source.id,
      selectedObjectIds: [object.id],
      reassignLayer: reassignLayerSpy,
    });
    useUiStore.getState().movePanelBetweenZones('cuts_layers', 'top-right', 'middle-left');
    useUiStore.getState().setZoneActiveTab('top-right', 'properties');
    useUiStore.getState().setZoneActiveTab('middle-left', '');

    render(<LayerTabs />);
    fireEvent.click(screen.getAllByTestId('layer-tab')[1]);

    expect(reassignLayerSpy).not.toHaveBeenCalled();
    expect(useProjectStore.getState().selectedLayerId).toBe(target.id);
    expect(useProjectStore.getState().selectedObjectIds).toEqual([object.id]);
    expect(useUiStore.getState().panelLayout.zones['middle-left'].activeTab).toBe('cuts_layers');
    expect(useUiStore.getState().panelLayout.zones['top-right'].activeTab).toBe('properties');
  });

  it('opens the relocated Layers panel after adding a layer without clearing the object selection', async () => {
    const source = makeLayer({ id: 'l1', name: 'Source', color_tag: '#000000' });
    const object = makeProjectObject({ id: 'obj-1', layer_id: source.id });
    const addLayer = vi.fn().mockImplementation(async () => {
      const state = useProjectStore.getState();
      const created = makeLayer({ id: 'l2', name: 'C01 (Line)', color_tag: '#ff0000', order_index: 1 });
      useProjectStore.setState({
        project: state.project ? { ...state.project, layers: [...state.project.layers, created] } : null,
        selectedLayerId: created.id,
      });
    });
    const updateLayer = vi.fn().mockResolvedValue(true);

    useProjectStore.setState({
      project: makeProject({ layers: [source], objects: [object] }),
      selectedLayerId: source.id,
      selectedObjectIds: [object.id],
      addLayer,
      updateLayer,
    });
    useUiStore.getState().movePanelBetweenZones('cuts_layers', 'top-right', 'middle-left');
    useUiStore.getState().setZoneActiveTab('top-right', 'properties');
    useUiStore.getState().setZoneActiveTab('middle-left', '');

    render(<LayerTabs />);
    fireEvent.click(screen.getByTestId('add-layer-tab'));

    await waitFor(() => {
      expect(useUiStore.getState().panelLayout.zones['middle-left'].activeTab).toBe('cuts_layers');
    });
    expect(useUiStore.getState().panelLayout.zones['top-right'].activeTab).toBe('properties');
    expect(useProjectStore.getState().selectedObjectIds).toEqual([object.id]);
  });

  it('uses a pressed eye icon to control layer visibility', async () => {
    const layer = makeLayer({ id: 'l1', visible: true });
    const setLayerVisibleSpy = vi.spyOn(projectService, 'setLayerVisible').mockResolvedValue(true);
    const loadProjectSpy = vi.spyOn(useProjectStore.getState(), 'loadProject').mockResolvedValue(undefined);
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [] }),
    });

    render(<LayerList />);

    const showToggle = screen.getByTestId('show-toggle');
    expect(showToggle.getAttribute('aria-pressed')).toBe('true');
    expect(showToggle.querySelector('.lucide-eye')).not.toBeNull();
    fireEvent.click(showToggle);
    await waitFor(() => {
      expect(setLayerVisibleSpy).toHaveBeenCalledWith('l1', false);
    });
    expect(loadProjectSpy).toHaveBeenCalledWith({ invalidatePreview: true });
  });

  it('shows localized layer opacity for filled operations and persists a completed slider change', () => {
    const layer = makeLayer({ id: 'l1', operation: 'fill', fill_opacity: 0.65 });
    const updateLayerSpy = vi.fn().mockResolvedValue(true);
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [] }),
      selectedLayerId: 'l1',
      updateLayer: updateLayerSpy,
    });

    render(<LayerList />);

    const opacity = screen.getByTestId('layer-fill-opacity') as HTMLInputElement;
    expect(screen.getByText('Opacity (%)')).toBeDefined();
    expect(screen.getByText(/Canvas preview only/)).toBeDefined();
    expect(opacity.value).toBe('65');
    fireEvent.change(opacity, { target: { value: '40' } });
    expect(opacity.value).toBe('40');
    expect(updateLayerSpy).not.toHaveBeenCalled();
    fireEvent.pointerUp(opacity);
    expect(updateLayerSpy).toHaveBeenCalledWith('l1', { fill_opacity: 0.4 });
  });

  it('does not show layer opacity for wireframe operations', () => {
    useProjectStore.setState({
      project: makeProject({ layers: [makeLayer({ id: 'l1', operation: 'line' })], objects: [] }),
      selectedLayerId: 'l1',
    });

    render(<LayerList />);

    expect(screen.queryByTestId('layer-fill-opacity-control')).toBeNull();
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

  it('displays speed/power summary', () => {
    const layer = makeLayer({ speed_mm_min: 5000, power_percent: 65 });
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [] }),
    });

    render(<LayerTabs />);

    const summary = screen.getByTestId('speed-power');
    expect(summary.textContent).toBe('5000/65%');
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

    render(<><LayerTabs /><LayerList /></>);

    // Tab summary and the pass stack's speed field both use the selected unit.
    expect(screen.getByTestId('speed-power').textContent).toBe('50/65%');
    expect(screen.getAllByText('Speed (mm/sec)').length).toBeGreaterThanOrEqual(1);
    void updateCutEntrySpy;
  });

  it('double-click opens CutSettingsEditor dialog', () => {
    const layer = makeLayer({ id: 'l1', name: 'Cut Layer' });
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [] }),
    });

    render(<LayerTabs />);

    expect(screen.queryByTestId('cut-settings-overlay')).toBeNull();

    fireEvent.doubleClick(screen.getByTestId('layer-tab'));

    expect(screen.getByTestId('cut-settings-overlay')).toBeDefined();
    expect(screen.getByTestId('layer-name-input')).toBeDefined();
  });

  it('color swatch shows the layer color and opens the palette picker', () => {
    const layer = makeLayer({ id: 'l1', color_tag: '#FF0000' });
    const updateLayerSpy = vi.fn().mockResolvedValue(true);
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [] }),
      updateLayer: updateLayerSpy,
    });

    render(<LayerList />);

    const swatch = screen.getByTestId('quick-edit-color');
    expect(swatch.style.backgroundColor).toBe('rgb(255, 0, 0)');

    fireEvent.click(swatch);
    const picker = screen.getByTestId('layer-color-picker');
    // 30 regular palette colors + 2 tool colors (dashed; convert to tool layer)
    const colorButtons = picker.querySelectorAll('button');
    expect(colorButtons.length).toBe(32);

    fireEvent.click(colorButtons[2]);
    expect(updateLayerSpy).toHaveBeenCalledWith('l1', { color_tag: expect.stringMatching(/^#/) });
    expect(screen.queryByTestId('layer-color-picker')).toBeNull();
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

    render(<LayerTabs />);

    const layerRow = screen.getByTestId('layer-tab');
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

    render(<LayerTabs />);

    fireEvent.click(screen.getByTestId('layer-tab'), { shiftKey: true });

    expect(selectObjectsSpy).toHaveBeenCalledWith(['obj-2', 'obj-1']);
  });

  it('shift-right-click flashes the layer without opening the context menu', async () => {
    const layer = makeLayer({ id: 'l1' });
    const flashLayerSpy = vi.fn();
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [] }),
    });
    useUiStore.setState({ flashLayer: flashLayerSpy });

    render(<LayerTabs />);

    fireEvent.contextMenu(screen.getByTestId('layer-tab'), { shiftKey: true });

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

    render(<LayerTabs />);

    const layerRow = screen.getByTestId('layer-tab');
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

    render(<LayerTabs />);

    fireEvent.contextMenu(screen.getByTestId('layer-tab'));
    await waitFor(() => {
      expect(screen.getByTestId('context-menu')).toBeDefined();
    });
    fireEvent.click(screen.getByText('Rename'));

    const input = screen.getByTestId('tab-rename-input');
    fireEvent.change(input, { target: { value: 'Renamed Layer' } });
    fireEvent.keyDown(input, { key: 'Enter' });

    await waitFor(() => {
      expect(updateLayerSpy).toHaveBeenCalledWith('l1', { name: 'Renamed Layer' });
    });
    expect(screen.getByTestId('tab-rename-input')).toBeDefined();
    expect((screen.getByTestId('tab-rename-input') as HTMLInputElement).value).toBe('Renamed Layer');
  });

  it('locks a layer through the tab menu batch action instead of per-object updates', async () => {
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

    render(<LayerTabs />);

    fireEvent.contextMenu(screen.getByTestId('layer-tab'));
    fireEvent.click(await screen.findByText('Toggle lock on layer objects'));

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

    render(<LayerTabs />);

    const rows = screen.getAllByTestId('layer-tab');
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

    render(<LayerTabs />);

    const layerRow = screen.getByTestId('layer-tab');
    fireEvent.contextMenu(layerRow);

    await waitFor(() => {
      expect(screen.getByTestId('context-menu')).toBeDefined();
    });

    fireEvent.click(screen.getByText('Select All on Layer'));
    expect(selectObjectsSpy).toHaveBeenCalledWith(['obj-1', 'obj-2']);
  });
});
