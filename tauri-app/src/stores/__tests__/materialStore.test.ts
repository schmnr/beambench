import { describe, it, expect, vi, beforeEach } from 'vitest';
import { useMaterialStore } from '../materialStore';
import { useNotificationStore } from '../notificationStore';
import { useProjectStore } from '../projectStore';

vi.mock('../../services/materialService', () => ({
  materialService: {
    getPresets: vi.fn(),
    savePreset: vi.fn(),
    deletePreset: vi.fn(),
    applyPreset: vi.fn(),
  },
}));

vi.mock('../../services/projectService', () => ({
  projectService: {
    getProject: vi.fn(),
    getUndoState: vi.fn().mockResolvedValue({ can_undo: false, can_redo: false }),
  },
}));

vi.mock('../previewStore', () => ({
  usePreviewStore: {
    getState: () => ({
      invalidate: vi.fn(),
    }),
  },
}));

import { materialService } from '../../services/materialService';
import { projectService } from '../../services/projectService';
import type { MaterialPreset } from '../../types/material';

const mockedMaterial = materialService as {
  getPresets: ReturnType<typeof vi.fn>;
  savePreset: ReturnType<typeof vi.fn>;
  deletePreset: ReturnType<typeof vi.fn>;
  applyPreset: ReturnType<typeof vi.fn>;
};
const mockedProject = projectService as unknown as {
  getProject: ReturnType<typeof vi.fn>;
};

const samplePreset: MaterialPreset = {
  id: 'p1',
  name: '3mm Plywood',
  material: 'Wood',
  thickness_mm: 3,
  operation: 'cut',
  speed_mm_min: 600,
  power_percent: 80,
  passes: 1,
  notes: '',
  category: '',
};

describe('materialStore', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useMaterialStore.setState({ presets: [], loading: false, error: null });
    useNotificationStore.setState({ notifications: [] });
    useProjectStore.setState({ project: null });
  });

  it('loadPresets fetches and sets presets', async () => {
    mockedMaterial.getPresets.mockResolvedValue([samplePreset]);

    await useMaterialStore.getState().loadPresets();

    const state = useMaterialStore.getState();
    expect(state.presets).toEqual([samplePreset]);
    expect(state.loading).toBe(false);
    expect(state.error).toBeNull();
  });

  it('savePreset saves and reloads', async () => {
    mockedMaterial.savePreset.mockResolvedValue(samplePreset);
    mockedMaterial.getPresets.mockResolvedValue([samplePreset]);

    await expect(useMaterialStore.getState().savePreset(samplePreset)).resolves.toBe(true);

    expect(mockedMaterial.savePreset).toHaveBeenCalledWith(samplePreset);
    expect(mockedMaterial.getPresets).toHaveBeenCalled();
    expect(useMaterialStore.getState().presets).toEqual([samplePreset]);
  });

  it('deletePreset removes and reloads', async () => {
    mockedMaterial.deletePreset.mockResolvedValue(undefined);
    mockedMaterial.getPresets.mockResolvedValue([]);

    await useMaterialStore.getState().deletePreset('p1');

    expect(mockedMaterial.deletePreset).toHaveBeenCalledWith('p1');
    expect(useMaterialStore.getState().presets).toEqual([]);
  });

  it('applyPreset calls service and notifies', async () => {
    mockedMaterial.applyPreset.mockResolvedValue({
      applied_layer_id: 'layer1',
      targeted_entry_id: 'entry1',
      warnings: [],
    });
    mockedProject.getProject.mockResolvedValue({
      metadata: { format_version: '1', app_version: '0.1.0', project_id: 'p1', project_name: 'Test', created_at: '', modified_at: '' },
      workspace: { bed_width_mm: 400, bed_height_mm: 400, origin: 'top_left' },
      layers: [{
        id: 'layer1',
        name: 'L1',
        entries: [{
          id: 'entry1',
          operation: 'line',
          speed_mm_min: 800,
          power_percent: 70,
          raster_settings: null,
          vector_settings: null,
          air_assist: false,
          power_min_percent: 0,
          z_offset_mm: 0,
          gcode_prefix: '',
          gcode_suffix: '',
          output_enabled: true,
        }],
        operation: 'line',
        enabled: true,
        order_index: 0,
        color_tag: '#ff0000',
        speed_mm_min: 800,
        power_percent: 70,
        raster_settings: null,
        vector_settings: null,
        visible: true,
        air_assist: false,
        power_min_percent: 0,
        z_offset_mm: 0,
        gcode_prefix: '',
        gcode_suffix: '',
        is_tool_layer: false,
      }],
      objects: [],
      assets: [],
    });

    await useMaterialStore.getState().applyPreset('p1', 'layer1');

    expect(mockedMaterial.applyPreset).toHaveBeenCalledWith('p1', 'layer1');
    expect(mockedProject.getProject).toHaveBeenCalled();
    expect(useProjectStore.getState().project?.dirty).toBe(true);
    const notifications = useNotificationStore.getState().notifications;
    expect(notifications).toHaveLength(1);
    expect(notifications[0].type).toBe('success');
  });

  it('handles errors with notification', async () => {
    mockedMaterial.getPresets.mockRejectedValue('Network error');

    await useMaterialStore.getState().loadPresets();

    const state = useMaterialStore.getState();
    expect(state.error).toBe('Network error');
    expect(state.loading).toBe(false);
    const notifications = useNotificationStore.getState().notifications;
    expect(notifications).toHaveLength(1);
    expect(notifications[0].type).toBe('error');
  });

  it('savePreset returns false when the save fails', async () => {
    mockedMaterial.savePreset.mockRejectedValue('Save failed');

    await expect(useMaterialStore.getState().savePreset(samplePreset)).resolves.toBe(false);

    expect(useMaterialStore.getState().error).toBe('Save failed');
  });
});
