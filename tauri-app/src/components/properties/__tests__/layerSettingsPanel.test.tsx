import { afterEach, describe, expect, it, vi } from 'vitest';
import { render, screen, cleanup, fireEvent } from '@testing-library/react';
import { LayerSettingsPanel } from '../LayerSettingsPanel';
import { useProjectStore } from '../../../stores/projectStore';
import {
  makeLayer,
  makeProject,
  makeRasterSettings,
  makeVectorSettings,
} from '../../../test-utils/projectFixtures';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockReturnValue(new Promise(() => {})) }));

const initialState = useProjectStore.getState();

afterEach(() => {
  cleanup();
  useProjectStore.setState(initialState, true);
});

describe('LayerSettingsPanel', () => {
  it('uses a pressed wind icon for air assist instead of a switch', () => {
    const layer = makeLayer({ id: 'l1', air_assist: true });
    const updateCutEntry = vi.fn();
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [], assets: [] }),
      selectedLayerId: 'l1',
      updateCutEntry,
    });

    render(<LayerSettingsPanel />);

    const airAssist = screen.getByRole('button', { name: 'Air Assist' });
    expect(airAssist.getAttribute('aria-pressed')).toBe('true');
    expect(airAssist.querySelector('.lucide-wind')).not.toBeNull();

    fireEvent.click(airAssist);
    expect(updateCutEntry).toHaveBeenCalledWith('l1', 'entry-1', { air_assist: false });
  });

  it('hosts the shared sub-layer stack and exposes raster mode options when expanded', () => {
    const layer = makeLayer({
      id: 'l1',
      name: 'Image Layer',
      operation: 'image',
      raster_settings: makeRasterSettings({ mode: 'floyd_steinberg', overscan_mm: 0 }),
    });
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [], assets: [] }),
      selectedLayerId: 'l1',
    });

    render(<LayerSettingsPanel />);

    expect(screen.getByDisplayValue('Image Layer')).toBeDefined();

    const modeSelect = screen.getAllByRole('combobox')[1];
    const options = Array.from(modeSelect.querySelectorAll('option')).map((option) =>
      option.getAttribute('value'),
    );
    expect(options).toEqual([
      'grayscale',
      'threshold',
      'floyd_steinberg',
      'ordered_dither',
      'stucki',
      'jarvis',
      'sierra',
      'atkinson',
      'halftone',
      'newsprint',
      'sketch',
    ]);
  });

  it('shows the offset fill density and grouping controls in the shared editor', () => {
    const layer = makeLayer({
      id: 'l1',
      name: 'Offset Fill Layer',
      operation: 'offset_fill',
      raster_settings: makeRasterSettings({ line_interval_mm: 0.1, dpi: 254 }),
      vector_settings: makeVectorSettings({ offset_fill_grouping_mode: 'groups_together' }),
    });
    useProjectStore.setState({
      project: makeProject({ layers: [layer], objects: [], assets: [] }),
      selectedLayerId: 'l1',
    });

    render(<LayerSettingsPanel />);


    expect(screen.getByTestId('offset-fill-mode-graphic')).toBeDefined();
    expect(screen.getByText('Line Interval (mm)')).toBeDefined();
    expect(screen.getByText('Lines per inch')).toBeDefined();
    expect(screen.getByLabelText('Fill all shapes at once')).toBeDefined();
    expect(screen.getByLabelText('Fill groups together')).toBeDefined();
    expect(screen.getByLabelText('Fill shapes individually')).toBeDefined();
    expect(screen.getByText('Bi-directional fill')).toBeDefined();
    expect(screen.getByText('Cross-Hatch')).toBeDefined();
    expect(screen.getByText('Scan Angle (deg)')).toBeDefined();
  });
});
