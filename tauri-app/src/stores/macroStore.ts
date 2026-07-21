import { create } from 'zustand';
import type { MacroDefinition } from '../types/macro';
import { macroService } from '../services/macroService';
import { useNotificationStore } from './notificationStore';
import i18n from '../i18n';
import { wrapBackendError } from '../i18n/errors';

interface MacroStoreState {
  macros: MacroDefinition[];
  loading: boolean;
  error: string | null;

  loadMacros: () => Promise<void>;
  saveMacro: (macroDef: MacroDefinition) => Promise<boolean>;
  deleteMacro: (macroId: string) => Promise<void>;
  runMacro: (macroId: string) => Promise<void>;
}

export const useMacroStore = create<MacroStoreState>((set, get) => ({
  macros: [],
  loading: false,
  error: null,

  loadMacros: async () => {
    try {
      set({ loading: true, error: null });
      const macros = (await macroService.getMacros()) ?? [];
      set({ macros, loading: false });
    } catch (e) {
      const msg = String(e);
      set({ error: msg, loading: false });
      useNotificationStore.getState().push(wrapBackendError(msg), 'error');
    }
  },

  saveMacro: async (macroDef) => {
    try {
      await macroService.saveMacro(macroDef);
      await get().loadMacros();
      return true;
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      useNotificationStore.getState().push(wrapBackendError(msg), 'error');
      return false;
    }
  },

  deleteMacro: async (macroId) => {
    try {
      await macroService.deleteMacro(macroId);
      await get().loadMacros();
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      useNotificationStore.getState().push(wrapBackendError(msg), 'error');
    }
  },

  runMacro: async (macroId) => {
    try {
      await macroService.runMacro(macroId);
      useNotificationStore.getState().push(i18n.t('notifications.macro_executed'), 'success');
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      useNotificationStore.getState().push(wrapBackendError(msg), 'error');
    }
  },
}));
