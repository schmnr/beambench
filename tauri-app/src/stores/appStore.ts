import { create } from 'zustand';
import type { AppStatus, AppSettings } from '../types/commands';
import { appService } from '../services/appService';
import type { AppSettingsUpdate } from '../services/appService';
import { uiThemeController } from '../theme';

let settingsMutationSeq = 0;

export function bumpSettingsMutationSeq(): number {
  settingsMutationSeq += 1;
  return settingsMutationSeq;
}

export function getSettingsMutationSeq(): number {
  return settingsMutationSeq;
}

function hydrateSettingsIntoUi(settings: AppSettings, seq: number = settingsMutationSeq): void {
  if (seq !== settingsMutationSeq) return;
  uiThemeController.sync(settings.ui_theme ?? 'dark');

  void import('./uiStore').then(({ useUiStore, viewStyleFromRenderOptions }) => {
    if (seq !== settingsMutationSeq) return;

    const ui = useUiStore.getState();
    ui.setGridSpacing(settings.grid_spacing_mm);
    ui.setNudgeSteps({
      normal: settings.nudge_step_mm,
      fine: settings.nudge_step_fine_mm,
      coarse: settings.nudge_step_coarse_mm,
    });
    ui.setViewStyle(viewStyleFromRenderOptions({
      antialiasing: settings.antialiasing,
      filledRendering: settings.filled_rendering,
    }));
  });
}

interface AppStoreState {
  status: AppStatus | null;
  settings: AppSettings | null;
  loading: boolean;
  error: string | null;

  fetchStatus: () => Promise<void>;
  fetchSettings: () => Promise<void>;
  applySettings: (settings: AppSettings) => void;
  updateSettings: (updates: AppSettingsUpdate) => Promise<void>;
}

export const useAppStore = create<AppStoreState>((set) => ({
  status: null,
  settings: null,
  loading: false,
  error: null,

  fetchStatus: async () => {
    try {
      set({ loading: true, error: null });
      const status = await appService.getStatus();
      set({ status, loading: false });
    } catch (e) {
      set({ error: String(e), loading: false });
    }
  },

  fetchSettings: async () => {
    try {
      const seqAtStart = settingsMutationSeq;
      const settings = await appService.getSettings();
      if (seqAtStart === settingsMutationSeq) {
        set({ settings });
        hydrateSettingsIntoUi(settings, seqAtStart);
      }
    } catch (e) {
      set({ error: String(e) });
    }
  },

  applySettings: (settings) => {
    const seq = bumpSettingsMutationSeq();
    set({ settings });
    hydrateSettingsIntoUi(settings, seq);
  },

  updateSettings: async (updates) => {
    try {
      const seq = bumpSettingsMutationSeq();
      const settings = await appService.updateSettings(updates);
      if (settings && seq === settingsMutationSeq) {
        set({ settings });
        hydrateSettingsIntoUi(settings, seq);
      }
    } catch (e) {
      set({ error: String(e) });
      throw e;
    }
  },
}));
