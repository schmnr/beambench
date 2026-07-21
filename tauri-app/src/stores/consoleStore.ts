import { create } from 'zustand';
import type { ConsoleEntry } from '../types/console';
import { machineService } from '../services/machineService';
import { useNotificationStore } from './notificationStore';
import { wrapBackendError } from '../i18n/errors';

interface ConsoleStoreState {
  entries: ConsoleEntry[];
  inputHistory: string[];
  historyIndex: number;

  sendCommand: (line: string) => Promise<boolean>;
  refreshLog: () => Promise<void>;
  clearLog: () => Promise<void>;
  historyUp: () => string;
  historyDown: () => string;
}

let logRevision = 0;

export const useConsoleStore = create<ConsoleStoreState>((set, get) => ({
  entries: [],
  inputHistory: [],
  historyIndex: -1,

  sendCommand: async (line) => {
    try {
      await machineService.sendGcodeLine(line);
      const { inputHistory } = get();
      set({ inputHistory: [...inputHistory, line], historyIndex: -1 });
      await get().refreshLog();
      return true;
    } catch (e) {
      useNotificationStore.getState().push(wrapBackendError(String(e)), 'error');
      return false;
    }
  },

  refreshLog: async () => {
    const revision = logRevision;
    try {
      const entries = await machineService.getConsoleLog();
      if (revision === logRevision) {
        set({ entries });
      }
    } catch (e) {
      useNotificationStore.getState().push(wrapBackendError(String(e)), 'error');
    }
  },

  clearLog: async () => {
    try {
      await machineService.clearConsoleLog();
      logRevision += 1;
      set({ entries: [] });
    } catch (e) {
      useNotificationStore.getState().push(wrapBackendError(String(e)), 'error');
    }
  },

  historyUp: () => {
    const { inputHistory, historyIndex } = get();
    if (inputHistory.length === 0) return '';
    const newIndex = historyIndex === -1
      ? inputHistory.length - 1
      : Math.max(0, historyIndex - 1);
    set({ historyIndex: newIndex });
    return inputHistory[newIndex];
  },

  historyDown: () => {
    const { inputHistory, historyIndex } = get();
    if (historyIndex === -1) return '';
    const newIndex = historyIndex + 1;
    if (newIndex >= inputHistory.length) {
      set({ historyIndex: -1 });
      return '';
    }
    set({ historyIndex: newIndex });
    return inputHistory[newIndex];
  },
}));
