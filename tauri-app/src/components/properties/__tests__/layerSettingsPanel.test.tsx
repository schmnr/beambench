import { afterEach, describe, expect, it, vi } from 'vitest';
import { render, screen, cleanup } from '@testing-library/react';
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
