import { create } from 'zustand';
import type { MaterialPreset } from '../types/material';
import { materialService } from '../services/materialService';
import { useNotificationStore } from './notificationStore';
import i18n from '../i18n';
import { wrapBackendError } from '../i18n/errors';
import { useProjectStore } from './projectStore';
import { usePreviewStore } from './previewStore';

interface MaterialStoreState {
  presets: MaterialPreset[];
  loading: boolean;
  error: string | null;

  loadPresets: () => Promise<void>;
  savePreset: (preset: MaterialPreset) => Promise<boolean>;
  deletePreset: (presetId: string) => Promise<void>;
  applyPreset: (presetId: string, layerId: string) => Promise<void>;
}

export const useMaterialStore = create<MaterialStoreState>((set, get) => ({
  presets: [],
  loading: false,
  error: null,

  loadPresets: async () => {
    try {
      set({ loading: true, error: null });
      const presets = await materialService.getPresets();
      set({ presets, loading: false });
    } catch (e) {
      const msg = String(e);
      set({ error: msg, loading: false });
      useNotificationStore.getState().push(wrapBackendError(msg), 'error');
    }
  },

  savePreset: async (preset) => {
    try {
      await materialService.savePreset(preset);
      await get().loadPresets();
      return true;
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      useNotificationStore.getState().push(wrapBackendError(msg), 'error');
      return false;
    }
  },

  deletePreset: async (presetId) => {
    try {
      await materialService.deletePreset(presetId);
      await get().loadPresets();
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      useNotificationStore.getState().push(wrapBackendError(msg), 'error');
    }
  },

  applyPreset: async (presetId, layerId) => {
    try {
      const response = await materialService.applyPreset(presetId, layerId);
      // loadProject refreshes project state, undo buttons, and preview
      await useProjectStore.getState().loadProject();
      // Mark dirty — the backend sets project.dirty but it's serde-skipped
      const currentProject = useProjectStore.getState().project;
      if (currentProject) {
        useProjectStore.setState({ project: { ...currentProject, dirty: true } });
      }
      usePreviewStore.getState().invalidate();
      useNotificationStore.getState().push(i18n.t('notifications.material_preset_applied'), 'success');
      for (const warning of response.warnings) {
        useNotificationStore.getState().push(warning.message, 'warning');
      }
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      useNotificationStore.getState().push(wrapBackendError(msg), 'error');
    }
  },
}));
