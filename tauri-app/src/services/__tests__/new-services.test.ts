import { describe, it, expect, vi, beforeEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

import { materialService } from '../materialService';
import { macroService } from '../macroService';
import type { MaterialPreset } from '../../types/material';
import type { MacroDefinition } from '../../types/macro';

beforeEach(() => {
  vi.mocked(invoke).mockReset();
});

describe('materialService', () => {
  it('getPresets invokes correct command', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    const result = await materialService.getPresets();
    expect(invoke).toHaveBeenCalledWith('get_material_presets');
    expect(result).toEqual([]);
  });

  it('savePreset invokes with preset data', async () => {
    const preset: MaterialPreset = { id: 'p1', name: 'Plywood', material: 'Wood', thickness_mm: 3, operation: 'line', speed_mm_min: 1000, power_percent: 50, passes: 1, notes: '', category: '' };
    vi.mocked(invoke).mockResolvedValue(preset);
    await materialService.savePreset(preset);
    expect(invoke).toHaveBeenCalledWith('save_material_preset', { preset });
  });

  it('applyPreset invokes with correct params', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await materialService.applyPreset('p1', 'layer-1');
    expect(invoke).toHaveBeenCalledWith('apply_material_preset', { presetId: 'p1', layerId: 'layer-1' });
  });
});

describe('macroService', () => {
  const macro: MacroDefinition = {
    id: 'macro-1',
    name: 'Home',
    description: 'Homes the machine',
    commands: ['$H'],
  };

  it('getMacros invokes correct command', async () => {
    vi.mocked(invoke).mockResolvedValue([macro]);
    const result = await macroService.getMacros();
    expect(invoke).toHaveBeenCalledWith('get_macros');
    expect(result).toEqual([macro]);
  });

  it('runMacro invokes with macro ID', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await macroService.runMacro('macro-1');
    expect(invoke).toHaveBeenCalledWith('run_macro', { macroId: 'macro-1' });
  });
});
