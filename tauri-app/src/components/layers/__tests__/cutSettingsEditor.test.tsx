import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { CutSettingsEditor } from '../CutSettingsEditor';
import { useProjectStore } from '../../../stores/projectStore';
import { useMachineStore } from '../../../stores/machineStore';
import { useAppStore } from '../../../stores/appStore';
import type { Layer } from '../../../types/project';
import {
  makeAppSettings,
  makeLayer as makeFixtureLayer,
  makeMachineProfile,
  makeProject,
  makeRasterSettings,
  makeVectorSettings,
} from '../../../test-utils/projectFixtures';

function makeLayer(overrides: Parameters<typeof makeFixtureLayer>[0] = {}): Layer {
  return makeFixtureLayer({
    id: 'layer-1',
    name: 'Test Layer',
    operation: 'line',
    color_tag: '#FF0000',
    speed_mm_min: 3000,
    power_percent: 80,
    ...overrides,
  });
}

const initialState = useProjectStore.getState();
const initialMachineState = useMachineStore.getState();
const initialAppState = useAppStore.getState();

afterEach(() => {
  cleanup();
  useProjectStore.setState(initialState, true);
  useMachineStore.setState(initialMachineState, true);
  useAppStore.setState(initialAppState, true);
  vi.restoreAllMocks();
});

describe('CutSettingsEditor', () => {
  it.each([
    ['mm', 'minutes', 6000],
    ['mm', 'seconds', 100],
    ['inches', 'minutes', 236.2],
    ['inches', 'seconds', 3.94],
  ] as const)('uses the profile maximum in %s per %s and reaches the exact machine speed', (display_unit, speed_time_unit, displayMax) => {
    const layer = makeLayer();
    const updateCutEntry = vi.fn().mockResolvedValue(true);
    const profile = makeMachineProfile({ max_speed_mm_min: 6000 });
    useMachineStore.setState({ profiles: [profile], activeProfileId: profile.id });
    useAppStore.setState({ settings: makeAppSettings({ display_unit, speed_time_unit }) });
    useProjectStore.setState({ project: makeProject({ layers: [layer] }), updateCutEntry });

    render(<CutSettingsEditor layerId={layer.id} onClose={vi.fn()} />);
    const slider = screen.getByTestId(`sub-layer-speed-slider-${layer.entries[0].id}`) as HTMLInputElement;
    const field = screen.getByRole('spinbutton', { name: /^Speed / }) as HTMLInputElement;
    expect(Number(slider.max)).toBe(displayMax);
    expect(Number(field.max)).toBe(displayMax);

    fireEvent.change(slider, { target: { value: slider.max } });
    expect(slider.validity.stepMismatch).toBe(false);
    expect(updateCutEntry).toHaveBeenLastCalledWith(layer.id, layer.entries[0].id, { speed_mm_min: 6000 });

    fireEvent.change(field, { target: { value: String(displayMax * 2) } });
    expect(updateCutEntry).toHaveBeenLastCalledWith(layer.id, layer.entries[0].id, { speed_mm_min: 6000 });
  });

  it('updates speed limits when saving or switching profiles without rewriting project speeds', () => {
    const layer = makeLayer({ speed_mm_min: 12000 });
    const updateCutEntry = vi.fn();
    const first = makeMachineProfile({ id: 'first', max_speed_mm_min: 6000 });
    const second = makeMachineProfile({ id: 'second', max_speed_mm_min: 600000 });
    useMachineStore.setState({ profiles: [first, second], activeProfileId: first.id });
    useProjectStore.setState({ project: makeProject({ layers: [layer] }), updateCutEntry });
    render(<CutSettingsEditor layerId={layer.id} onClose={vi.fn()} />);
    const slider = screen.getByTestId(`sub-layer-speed-slider-${layer.entries[0].id}`) as HTMLInputElement;
    const field = screen.getByRole('spinbutton', { name: 'Speed (mm/min)' }) as HTMLInputElement;
    expect(slider.max).toBe('6000');
    expect(field.value).toBe('12000');
    expect(screen.getByRole('status').textContent).toBe('Requested speed exceeds the machine limit: 6000 mm/min');

    act(() => useMachineStore.setState({ profiles: [{ ...first, max_speed_mm_min: 7305 }, second] }));
    expect(slider.max).toBe('7305');
    expect(screen.getByRole('status').textContent).toContain('7305 mm/min');
    act(() => useMachineStore.setState({ activeProfileId: second.id }));
    expect(slider.max).toBe('600000');
    expect(field.max).toBe('600000');
    expect(field.value).toBe('12000');
    expect(screen.queryByRole('status')).toBeNull();
    expect(updateCutEntry).not.toHaveBeenCalled();
    expect(useProjectStore.getState().project?.layers[0].entries[0].speed_mm_min).toBe(12000);
  });

  it('reaches a profile maximum that does not align with the numeric step', () => {
    const layer = makeLayer();
    const profile = makeMachineProfile({ max_speed_mm_min: 7305 });
    const updateCutEntry = vi.fn();
    useMachineStore.setState({ profiles: [profile], activeProfileId: profile.id });
    useProjectStore.setState({ project: makeProject({ layers: [layer] }), updateCutEntry });
    render(<CutSettingsEditor layerId={layer.id} onClose={vi.fn()} />);
    const slider = screen.getByTestId(`sub-layer-speed-slider-${layer.entries[0].id}`) as HTMLInputElement;
    fireEvent.change(slider, { target: { value: slider.max } });
    expect(slider.validity.stepMismatch).toBe(false);
    expect(updateCutEntry).toHaveBeenLastCalledWith(layer.id, layer.entries[0].id, { speed_mm_min: 7305 });
  });

  it.each([undefined, 0, -1, NaN, Infinity])('keeps a usable fallback when the profile maximum is %s', (maxSpeed) => {
    const layer = makeLayer({ speed_mm_min: 600000 });
    const profile = makeMachineProfile({ max_speed_mm_min: maxSpeed });
    useMachineStore.setState({ profiles: maxSpeed === undefined ? [] : [profile], activeProfileId: profile.id });
    useProjectStore.setState({ project: makeProject({ layers: [layer] }) });
    render(<CutSettingsEditor layerId={layer.id} onClose={vi.fn()} />);
    const slider = screen.getByTestId(`sub-layer-speed-slider-${layer.entries[0].id}`) as HTMLInputElement;
    expect(slider.max).toBe('50000');
    expect(screen.queryByRole('status')).toBeNull();
  });

  it('renders the shared stacked sub-layer editor inside the dialog', () => {
    const layer = makeLayer();
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [], assets: [] }),
    });

    render(<CutSettingsEditor layerId="layer-1" onClose={vi.fn()} />);

    expect(screen.getByTestId('cut-settings-overlay')).toBeDefined();
    expect(screen.getByTestId('layer-name-input')).toBeDefined();
    expect(screen.getByTestId('sub-layer-card-0')).toBeDefined();
    expect(screen.getByTestId('add-sub-layer')).toBeDefined();
  });

  it('renaming the layer routes through projectStore.updateLayer', () => {
    const layer = makeLayer();
    const updateLayer = vi.fn().mockResolvedValue(true);
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [], assets: [] }),
      updateLayer,
    });

    render(<CutSettingsEditor layerId="layer-1" onClose={vi.fn()} />);
    fireEvent.change(screen.getByTestId('layer-name-input'), {
      target: { value: 'Renamed Layer' },
    });

    expect(updateLayer).toHaveBeenCalledWith('layer-1', { name: 'Renamed Layer' });
  });

  it('adding a sub-layer calls projectStore.addCutEntry after the last entry', () => {
    const layer = makeLayer({
      entries: [
        {
          ...makeLayer().entries[0],
          id: 'entry-a',
        },
      ],
    });
    const addCutEntry = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [], assets: [] }),
      addCutEntry,
    });

    render(<CutSettingsEditor layerId="layer-1" onClose={vi.fn()} />);
    fireEvent.click(screen.getByTestId('add-sub-layer'));

    expect(addCutEntry).toHaveBeenCalledWith('layer-1', 'entry-a');
  });

  it('changing a sub-layer mode patches the targeted cut entry', () => {
    const layer = makeLayer();
    const updateCutEntry = vi.fn().mockResolvedValue(true);
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [], assets: [] }),
      updateCutEntry,
    });

    render(<CutSettingsEditor layerId="layer-1" onClose={vi.fn()} />);
    fireEvent.change(screen.getByDisplayValue('Line'), {
      target: { value: 'fill' },
    });

    expect(updateCutEntry).toHaveBeenCalledWith(
      'layer-1',
      layer.entries[0].id,
      expect.objectContaining({
        operation: 'fill',
        vector_settings: null,
      }),
    );
    expect(updateCutEntry.mock.calls[0][2].raster_settings).not.toBeNull();
  });

  it('converts speed fields through the selected speed time unit', () => {
    const layer = makeLayer({ speed_mm_min: 3000 });
    const updateCutEntry = vi.fn().mockResolvedValue(true);
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [], assets: [] }),
      updateCutEntry,
    });
    useAppStore.setState({
      settings: makeAppSettings({ speed_time_unit: 'seconds' }),
    });

    render(<CutSettingsEditor layerId="layer-1" onClose={vi.fn()} />);

    expect(screen.getByText('Speed (mm/sec)')).toBeDefined();
    const speedInput = screen.getByRole('spinbutton', { name: 'Speed (mm/sec)' });
    fireEvent.change(speedInput, { target: { value: '75' } });

    expect(updateCutEntry).toHaveBeenCalledWith('layer-1', layer.entries[0].id, {
      speed_mm_min: 4500,
    });
  });

  it('provides sliders for speed and passes that update the targeted cut entry', () => {
    const layer = makeLayer({
      speed_mm_min: 3000,
      vector_settings: makeVectorSettings({ passes: 2 }),
    });
    const updateCutEntry = vi.fn().mockResolvedValue(true);
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [], assets: [] }),
      updateCutEntry,
    });

    render(<CutSettingsEditor layerId="layer-1" onClose={vi.fn()} />);

    fireEvent.change(screen.getByTestId(`sub-layer-speed-slider-${layer.entries[0].id}`), {
      target: { value: '4500' },
    });
    expect(updateCutEntry).toHaveBeenCalledWith('layer-1', layer.entries[0].id, {
      speed_mm_min: 4500,
    });

    const passesSlider = screen.getByTestId(`sub-layer-passes-slider-${layer.entries[0].id}`);
    expect(passesSlider.getAttribute('max')).toBe('50');
    fireEvent.change(passesSlider, {
      target: { value: '4' },
    });
    expect(updateCutEntry).toHaveBeenCalledWith(
      'layer-1',
      layer.entries[0].id,
      expect.objectContaining({
        vector_settings: expect.objectContaining({ passes: 4 }),
      }),
    );
  });

  it('vector-backed mode picker only offers line, fill, and offset fill', () => {
    const layer = makeLayer();
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [], assets: [] }),
    });

    render(<CutSettingsEditor layerId="layer-1" onClose={vi.fn()} />);

    const modeSelect = screen.getByDisplayValue('Line');
    const options = Array.from(modeSelect.querySelectorAll('option')).map((option) =>
      option.getAttribute('value'),
    );
    expect(options).toEqual(['line', 'fill', 'offset_fill']);
  });

  it('legacy cut entries render on the line surface and cannot switch back to cut or score', () => {
    const layer = makeLayer({
      operation: 'cut',
      vector_settings: makeVectorSettings(),
    });
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [], assets: [] }),
    });

    render(<CutSettingsEditor layerId="layer-1" onClose={vi.fn()} />);

    const modeSelect = screen.getByDisplayValue('Line');
    const options = Array.from(modeSelect.querySelectorAll('option')).map((option) =>
      option.getAttribute('value'),
    );
    expect(options).toEqual(['line', 'fill', 'offset_fill']);
    expect(screen.getByText('Perforation')).toBeDefined();
  });

  it('switching to offset fill keeps both raster and vector settings available', () => {
    const layer = makeLayer();
    const updateCutEntry = vi.fn().mockResolvedValue(true);
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [], assets: [] }),
      updateCutEntry,
    });

    render(<CutSettingsEditor layerId="layer-1" onClose={vi.fn()} />);
    fireEvent.change(screen.getByDisplayValue('Line'), {
      target: { value: 'offset_fill' },
    });

    expect(updateCutEntry).toHaveBeenCalledWith(
      'layer-1',
      layer.entries[0].id,
      expect.objectContaining({
        operation: 'offset_fill',
      }),
    );
    expect(updateCutEntry.mock.calls[0][2].raster_settings).not.toBeNull();
    expect(updateCutEntry.mock.calls[0][2].vector_settings).not.toBeNull();
  });

  it('offset fill line interval changes update raster settings', () => {
    const layer = makeLayer({
      operation: 'offset_fill',
      raster_settings: makeRasterSettings({ line_interval_mm: 0.1, dpi: 254 }),
      vector_settings: makeVectorSettings(),
    });
    const updateCutEntry = vi.fn().mockResolvedValue(true);
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [], assets: [] }),
      updateCutEntry,
    });

    render(<CutSettingsEditor layerId="layer-1" onClose={vi.fn()} />);
    const interval = screen.getByDisplayValue('0.1');
    fireEvent.change(interval, {
      target: { value: '0.08' },
    });
    fireEvent.blur(interval);

    expect(updateCutEntry).toHaveBeenCalledWith(
      'layer-1',
      layer.entries[0].id,
      expect.objectContaining({
        raster_settings: expect.objectContaining({
          line_interval_mm: 0.08,
          dpi: Math.round(25.4 / 0.08),
        }),
      }),
    );
  });

  it('offset fill hides perforation and gcode prefix/suffix fields', () => {
    const layer = makeLayer({
      operation: 'offset_fill',
      raster_settings: makeRasterSettings({ line_interval_mm: 0.1, dpi: 254 }),
      vector_settings: makeVectorSettings(),
    });
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [], assets: [] }),
    });

    render(<CutSettingsEditor layerId="layer-1" onClose={vi.fn()} />);

    expect(screen.queryByText('Perforation')).toBeNull();
    expect(screen.queryByText('G-code Prefix')).toBeNull();
    expect(screen.queryByText('G-code Suffix')).toBeNull();
  });

  it('hides min power and z offset for the default GRBL vector settings surface', () => {
    const layer = makeLayer();
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [], assets: [] }),
    });
    useMachineStore.setState({
      profiles: [makeMachineProfile({ id: 'profile-1', firmware_type: 'grbl', supports_z_moves: false })],
      activeProfileId: 'profile-1',
    });

    render(<CutSettingsEditor layerId="layer-1" onClose={vi.fn()} />);

    expect(screen.queryByText('Min Power (%)')).toBeNull();
    expect(screen.queryByText('Z Offset (mm)')).toBeNull();
  });

  it('shows min power for DSP-style profiles and z offset only when the profile supports Z moves', () => {
    const layer = makeLayer();
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [], assets: [] }),
    });
    useMachineStore.setState({
      profiles: [makeMachineProfile({ id: 'profile-1', firmware_type: 'ruida', supports_z_moves: true })],
      activeProfileId: 'profile-1',
    });

    render(<CutSettingsEditor layerId="layer-1" onClose={vi.fn()} />);

    expect(screen.getByText('Min Power (%)')).toBeDefined();
    expect(screen.getByText('Z Offset (mm)')).toBeDefined();
  });

  it('shows min power for grayscale image entries even on non-DSP profiles', () => {
    const layer = makeLayer({
      operation: 'image',
      raster_settings: makeRasterSettings({ mode: 'grayscale' }),
      vector_settings: null,
    });
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [], assets: [] }),
    });
    useMachineStore.setState({
      profiles: [makeMachineProfile({ id: 'profile-1', firmware_type: 'grbl', supports_z_moves: false })],
      activeProfileId: 'profile-1',
    });

    render(<CutSettingsEditor layerId="layer-1" onClose={vi.fn()} />);

    expect(screen.getByText('Min Power (%)')).toBeDefined();
    expect(screen.queryByText('Z Offset (mm)')).toBeNull();
  });

  it('layer switch arrows use onSwitchLayer and close button closes the dialog', () => {
    const first = makeLayer({ id: 'layer-1', name: 'First' });
    const second = makeLayer({ id: 'layer-2', name: 'Second' });
    const onClose = vi.fn();
    const onSwitchLayer = vi.fn();
    useProjectStore.setState({
      project: makeProject({ layers: [first, second], objects: [], assets: [] }),
    });

    render(
      <CutSettingsEditor
        layerId="layer-1"
        onClose={onClose}
        onSwitchLayer={onSwitchLayer}
      />,
    );

    fireEvent.click(screen.getByText('→'));
    expect(onSwitchLayer).toHaveBeenCalledWith('layer-2');

    fireEvent.click(screen.getByTestId('close-btn'));
    expect(onClose).toHaveBeenCalled();
  });
});
