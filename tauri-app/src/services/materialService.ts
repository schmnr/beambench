import { invoke } from '@tauri-apps/api/core';
import type { MaterialPreset } from '../types/material';

export interface MaterialApplyWarning {
  code: 'multi_entry_layer_targeted_primary';
  message: string;
}

export interface MaterialApplyResponse {
  applied_layer_id: string;
  targeted_entry_id: string;
  warnings: MaterialApplyWarning[];
}

export const materialService = {
  async getPresets(): Promise<MaterialPreset[]> {
    return invoke<MaterialPreset[]>('get_material_presets');
  },

  // backend `save_material_preset` returns `()`, not the preset.
  async savePreset(preset: MaterialPreset): Promise<void> {
    return invoke<void>('save_material_preset', { preset });
  },

  async deletePreset(presetId: string): Promise<void> {
    return invoke<void>('delete_material_preset', { presetId });
  },

  async applyPreset(presetId: string, layerId: string): Promise<MaterialApplyResponse> {
    return invoke<MaterialApplyResponse>('apply_material_preset', { presetId, layerId });
  },
};
