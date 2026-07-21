import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, cleanup, fireEvent } from '@testing-library/react';

import { MaterialLibrary } from '../MaterialLibrary.js';
import { useMaterialStore } from '../../../stores/materialStore.js';
import { useProjectStore } from '../../../stores/projectStore.js';
import type { MaterialPreset } from '../../../types/material.js';
import type { Project } from '../../../types/project.js';
import { makeLayer, makeProject, makeRasterSettings } from '../../../test-utils/projectFixtures.js';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockReturnValue(new Promise(() => {})) }));

const initialMaterialState = useMaterialStore.getState();
const initialProjectState = useProjectStore.getState();

function createProjectWithRasterLayer(): Project {
  return makeProject({
    metadata: {
      format_version: '1',
      app_version: 'test',
      project_id: 'p1',
      project_name: 'Test Project',
      created_at: '2026-03-25T00:00:00Z',
      modified_at: '2026-03-25T00:00:00Z',
    },
    workspace: {
      bed_width_mm: 300,
      bed_height_mm: 200,
      origin: 'top_left',
    },
    layers: [makeLayer({
      id: 'l1',
      name: 'Raster Layer',
      operation: 'fill',
      color_tag: '#ff0000',
      speed_mm_min: 1800,
      power_percent: 55,
      raster_settings: makeRasterSettings({
        mode: 'grayscale',
        scan_angle: 45,
        overscan_mm: 2,
        line_interval_mm: 0.1,
        flood_fill: true,
        angle_passes: 3,
        angle_increment_deg: 30,
      }),
      vector_settings: null,
    })],
    objects: [],
    assets: [],
  });
}

const samplePresets: MaterialPreset[] = [
  {
    id: 'p1',
    name: 'Plywood 3mm',
    material: 'Wood',
    thickness_mm: 3,
    operation: 'fill',
    speed_mm_min: 1000,
    power_percent: 50,
    passes: 1,
    dpi: 254,
    raster_mode: 'grayscale',
    line_interval_mm: 0.1,
    scan_angle: 45,
    bidirectional: true,
    overscan_mm: 2,
    flood_fill: true,
    angle_passes: 2,
    angle_increment_deg: 30,
    notes: '',
    category: '',
  },
  {
    id: 'p2',
    name: 'Acrylic 5mm',
    material: 'Plastic',
    thickness_mm: 5,
    operation: 'line',
    speed_mm_min: 800,
    power_percent: 60,
    passes: 2,
    notes: '',
    category: '',
  },
];

function seedStores(overrides?: {
  presets?: MaterialPreset[];
  savePreset?: ReturnType<typeof vi.fn>;
  loadPresets?: ReturnType<typeof vi.fn>;
  deletePreset?: ReturnType<typeof vi.fn>;
  applyPreset?: ReturnType<typeof vi.fn>;
  project?: Project | null;
  selectedLayerId?: string | null;
}) {
  useMaterialStore.setState({
    ...useMaterialStore.getState(),
    presets: overrides?.presets ?? samplePresets,
    loadPresets: overrides?.loadPresets ?? vi.fn().mockResolvedValue(undefined),
    savePreset: overrides?.savePreset ?? vi.fn().mockResolvedValue(undefined),
    deletePreset: overrides?.deletePreset ?? vi.fn().mockResolvedValue(undefined),
    applyPreset: overrides?.applyPreset ?? vi.fn().mockResolvedValue(undefined),
  });
  useProjectStore.setState({
    ...useProjectStore.getState(),
    project: overrides?.project ?? null,
    selectedLayerId: overrides?.selectedLayerId ?? null,
  });
}

function optionLabels(select: HTMLElement): string[] {
  return Array.from((select as HTMLSelectElement).options).map((option) => option.textContent ?? '');
}

afterEach(() => {
  cleanup();
  useMaterialStore.setState(initialMaterialState, true);
  useProjectStore.setState(initialProjectState, true);
});

describe('MaterialLibrary', () => {
  it('renders preset list with names', () => {
    seedStores();
    render(<MaterialLibrary />);
    expect(screen.getByText('Plywood 3mm')).toBeDefined();
    expect(screen.getByText('Acrylic 5mm')).toBeDefined();
  });

  it('search filters presets', () => {
    seedStores();
    render(<MaterialLibrary />);
    const input = screen.getByPlaceholderText('Search materials...');
    fireEvent.change(input, { target: { value: 'Acrylic' } });
    expect(screen.queryByText('Plywood 3mm')).toBeNull();
    expect(screen.getByText('Acrylic 5mm')).toBeDefined();
  });

  it('apply calls applyPreset with selected layer', () => {
    const applyPreset = vi.fn();
    seedStores({ applyPreset, selectedLayerId: 'l1' });
    render(<MaterialLibrary />);
    fireEvent.click(screen.getAllByTitle('Apply')[0]);
    expect(applyPreset).toHaveBeenCalledWith('p1', 'l1');
  });

  it('delete calls deletePreset', () => {
    const deletePreset = vi.fn();
    seedStores({ deletePreset });
    render(<MaterialLibrary />);
    fireEvent.click(screen.getAllByTitle('Delete')[0]);
    expect(deletePreset).toHaveBeenCalledWith('p1');
  });

  it('duplicate preserves phase 4 raster fields', () => {
    const savePreset = vi.fn();
    seedStores({ savePreset });
    render(<MaterialLibrary />);

    fireEvent.click(screen.getAllByTitle('Duplicate')[0]);

    expect(savePreset).toHaveBeenCalledWith(
      expect.objectContaining({
        name: 'Plywood 3mm (Copy)',
        dpi: 254,
        raster_mode: 'grayscale',
        line_interval_mm: 0.1,
        scan_angle: 45,
        bidirectional: true,
        overscan_mm: 2,
        flood_fill: true,
        angle_passes: 2,
        angle_increment_deg: 30,
      }),
    );
  });

  it('edit preserves hidden phase 4 raster fields', () => {
    const savePreset = vi.fn();
    seedStores({ savePreset });
    render(<MaterialLibrary />);

    fireEvent.click(screen.getAllByTitle('Edit')[0]);
    fireEvent.change(screen.getByPlaceholderText('Name'), { target: { value: 'Updated Preset' } });
    fireEvent.click(screen.getByText('Save'));

    expect(savePreset).toHaveBeenCalledWith(
      expect.objectContaining({
        id: 'p1',
        name: 'Updated Preset',
        dpi: 254,
        raster_mode: 'grayscale',
        line_interval_mm: 0.1,
        scan_angle: 45,
        bidirectional: true,
        overscan_mm: 2,
        flood_fill: true,
        angle_passes: 2,
        angle_increment_deg: 30,
      }),
    );
  });

  it('limits editable operations to current material modes', () => {
    seedStores();
    render(<MaterialLibrary />);

    const filter = screen.getByTestId('operation-filter');
    expect(optionLabels(filter)).toEqual(['All Ops', 'Line', 'Fill', 'Offset Fill']);

    fireEvent.click(screen.getAllByTitle('Edit')[0]);
    const select = screen.getByTestId('operation-select');
    expect(optionLabels(select)).toEqual(['Line', 'Fill', 'Offset Fill']);
  });

  it('preserves an existing unsupported operation without offering it for new presets', () => {
    seedStores({
      presets: [
        {
          ...samplePresets[1],
          id: 'legacy-cut',
          operation: 'cut',
        },
      ],
    });
    render(<MaterialLibrary />);

    fireEvent.click(screen.getByTitle('Edit'));
    const select = screen.getByTestId('operation-select') as HTMLSelectElement;

    expect(select.value).toBe('cut');
    expect(optionLabels(select)).toEqual(['Line', 'Fill', 'Offset Fill', 'Cut (existing)']);
  });

  it('keeps the editor open when saving fails', async () => {
    const savePreset = vi.fn().mockResolvedValue(false);
    seedStores({ savePreset });
    render(<MaterialLibrary />);

    fireEvent.click(screen.getAllByTitle('Edit')[0]);
    fireEvent.change(screen.getByPlaceholderText('Name'), { target: { value: 'Updated Preset' } });
    fireEvent.click(screen.getByText('Save'));

    expect(savePreset).toHaveBeenCalledOnce();
    expect(await screen.findByDisplayValue('Updated Preset')).toBeDefined();
    expect(screen.getByText('Cancel')).toBeDefined();
  });

  it('create from layer captures phase 4 raster fields', () => {
    const savePreset = vi.fn();
    seedStores({
      savePreset,
      project: createProjectWithRasterLayer(),
      selectedLayerId: 'l1',
    });
    render(<MaterialLibrary />);

    fireEvent.click(screen.getByTestId('create-from-layer'));

    expect(savePreset).toHaveBeenCalledWith(
      expect.objectContaining({
        operation: 'fill',
        speed_mm_min: 1800,
        power_percent: 55,
        passes: 1,
        dpi: 254,
        raster_mode: null, // image-only field — null for fill layers
        line_interval_mm: 0.1,
        scan_angle: 45,
        bidirectional: true,
        overscan_mm: 2,
        flood_fill: true,
        angle_passes: 3,
        angle_increment_deg: 30,
        pass_through: null,
        halftone_cells_per_inch: null,
        halftone_angle_deg: null,
        newsprint_angle_deg: null,
        newsprint_frequency: null,
      }),
    );
  });

  it('guards switching presets when the current edit is dirty', () => {
    seedStores();
    render(<MaterialLibrary />);

    fireEvent.click(screen.getAllByTitle('Edit')[0]);
    fireEvent.change(screen.getByPlaceholderText('Name'), { target: { value: 'Dirty Preset' } });
    fireEvent.click(screen.getAllByTitle('Edit')[0]);

    expect(screen.getByText('Discard unsaved material preset changes?')).toBeDefined();
    expect(screen.getByDisplayValue('Dirty Preset')).toBeDefined();

    fireEvent.click(screen.getByRole('button', { name: 'Keep Editing' }));
    expect(screen.queryByText('Discard unsaved material preset changes?')).toBeNull();
    expect(screen.getByDisplayValue('Dirty Preset')).toBeDefined();
  });
});
