import { create } from 'zustand';
import type {
  MergeFieldInfo,
  VariableTextConfig,
  VariableTextMode,
  VariableTextSource,
} from '../types/variableText';
import type { ProjectObject } from '../types/project';
import { variableTextService } from '../services/variableTextService';
import { useNotificationStore } from './notificationStore';
import {
  defaultVariableTextConfig,
  defaultVariableTextSource,
  detectVariableTextWarnings,
  resetVariableTextCurrent,
  stepVariableTextCurrent,
  templateHasVariableText,
} from '../utils/variableText';
import { wrapBackendError } from '../i18n/errors';

const notifyError = (msg: string) => useNotificationStore.getState().push(wrapBackendError(msg), 'error');

interface VariableTextState {
  template: string | null;
  source: VariableTextSource | null;
  mode: VariableTextMode | null;
  offset: number;
  mergeFields: MergeFieldInfo[];
  warnings: string[];
  previewText: string | null;
  previewRow: number;
  jobStartTime: string | null;

  setTemplate: (text: string) => void;
  hydrateFromObject: (obj: ProjectObject) => void;
  ensureSource: () => void;
  loadCsv: (path: string) => Promise<void>;
  clearCsv: () => void;
  clearSource: () => void;
  refreshPreview: (objectId: string) => Promise<void>;
  parseMergeFields: (text?: string) => Promise<void>;
  setSerialStart: (start: number) => void;
  setSerialIncrement: (inc: number) => void;
  setSerialPadding: (padding: number) => void;
  setTotalCopies: (n: number) => void;
  setPreviewRow: (row: number) => void;
  setCurrent: (current: number) => void;
  setStart: (start: number) => void;
  setEnd: (end: number) => void;
  setAdvanceBy: (advanceBy: number) => void;
  setAutoAdvance: (autoAdvance: boolean) => void;
  setMode: (mode: VariableTextMode | null) => void;
  setOffset: (offset: number) => void;
  previous: () => void;
  next: () => void;
  reset: () => void;
  advanceSerial: () => void;
  buildConfig: () => VariableTextConfig | null;
  captureJobStartTime: () => void;
  clearJobStartTime: () => void;
}

function computeWarnings(template: string | null, mode: VariableTextMode | null): string[] {
  if (!template) return [];
  return detectVariableTextWarnings(template, mode);
}

function updateSource(current: VariableTextSource | null, patch: Partial<VariableTextSource>): VariableTextSource {
  const next = {
    ...(current ?? defaultVariableTextSource()),
    ...patch,
  };
  next.currentRow = next.current;
  return next;
}

export const useVariableTextStore = create<VariableTextState>((set, get) => ({
  template: null,
  source: null,
  mode: null,
  offset: 0,
  mergeFields: [],
  warnings: [],
  previewText: null,
  previewRow: 1,
  jobStartTime: null,

  setTemplate: (text) => {
    const source = get().source ?? (templateHasVariableText(text) ? defaultVariableTextSource() : null);
    const mode = get().mode;
    set({
      template: text,
      source,
      warnings: computeWarnings(text, mode),
    });
    void get().parseMergeFields(text);
  },

  hydrateFromObject: (obj) => {
    if (obj.data.type !== 'text') {
      set({
        template: null,
        source: null,
        mode: null,
        offset: 0,
        mergeFields: [],
        warnings: [],
        previewText: null,
        previewRow: 1,
      });
      return;
    }

    const config = obj.data.variable_text ?? (
      templateHasVariableText(obj.data.content)
        ? defaultVariableTextConfig(obj.data.content)
        : null
    );

    set({
      template: config?.template ?? obj.data.content,
      source: config?.source ?? null,
      mode: config?.mode ?? null,
      offset: config?.offset ?? 0,
      warnings: computeWarnings(config?.template ?? obj.data.content, config?.mode ?? null),
      previewRow: config?.source.current ?? 1,
    });
    void get().parseMergeFields(config?.template ?? obj.data.content);
  },

  ensureSource: () => {
    if (!get().source) {
      set({ source: defaultVariableTextSource() });
    }
  },

  loadCsv: async (path) => {
    try {
      const csvData = await variableTextService.loadCsvFile(path);
      const csvRows = [csvData.headers, ...csvData.rows];
      set((state) => ({
        source: updateSource(state.source, {
          csvPath: path,
          csvData: csvRows,
          current: 0,
          start: 0,
          end: Math.max(0, csvData.rows.length - 1),
          totalCopies: Math.max(1, csvData.rows.length),
        }),
        previewRow: 0,
      }));
    } catch (e) {
      notifyError(`Failed to load CSV: ${e}`);
    }
  },

  clearCsv: () => {
    set((state) => ({
      source: state.source
        ? {
            ...state.source,
            csvPath: null,
            csvData: [],
            current: 1,
            currentRow: 1,
            start: 1,
            end: 1,
          }
        : state.source,
      previewText: null,
      previewRow: 1,
    }));
  },

  clearSource: () => {
    set({
      source: null,
      previewText: null,
      previewRow: 0,
    });
  },

  refreshPreview: async (objectId) => {
    const config = get().buildConfig();
    if (!config) {
      set({ previewText: get().template });
      return;
    }
    try {
      const row = config.source.current ?? config.source.currentRow ?? config.source.start ?? 0;
      const resolved = await variableTextService.resolveVariableText(objectId, config, row);
      set({ previewText: resolved });
    } catch (e) {
      notifyError(`Failed to resolve text: ${e}`);
    }
  },

  parseMergeFields: async (textParam) => {
    const text = textParam ?? get().template;
    if (!text) {
      set({ mergeFields: [] });
      return;
    }
    try {
      const fields = await variableTextService.parseMergeFields(text);
      set({ mergeFields: fields });
    } catch (e) {
      notifyError(`Failed to parse merge fields: ${e}`);
    }
  },

  setSerialStart: (start) => {
    set((state) => ({
      source: updateSource(state.source, { start, current: start, end: Math.max(start, state.source?.end ?? start) }),
    }));
  },

  setSerialIncrement: (inc) => {
    set((state) => ({
      source: updateSource(state.source, { advanceBy: Math.max(1, inc) }),
    }));
  },

  setSerialPadding: (padding) => {
    set((state) => ({
      source: updateSource(state.source, {
        fieldDefaults: {
          ...(state.source?.fieldDefaults ?? {}),
          _serial_padding: String(Math.max(1, padding)),
        },
      }),
    }));
  },

  setTotalCopies: (n) => {
    set((state) => ({
      source: updateSource(state.source, { totalCopies: Math.max(1, n) }),
    }));
  },

  setPreviewRow: (row) => {
    set((state) => ({
      previewRow: row,
      source: state.source ? updateSource(state.source, { current: row }) : state.source,
    }));
  },

  setCurrent: (current) => {
    set((state) => ({
      previewRow: current,
      source: updateSource(state.source, { current }),
    }));
  },

  setStart: (start) => {
    set((state) => ({
      source: updateSource(state.source, { start }),
    }));
  },

  setEnd: (end) => {
    set((state) => ({
      source: updateSource(state.source, { end }),
    }));
  },

  setAdvanceBy: (advanceBy) => {
    set((state) => ({
      source: updateSource(state.source, { advanceBy: Math.max(1, advanceBy) }),
    }));
  },

  setAutoAdvance: (autoAdvance) => {
    set((state) => ({
      source: updateSource(state.source, { autoAdvance }),
    }));
  },

  setMode: (mode) => {
    const template = get().template;
    set({
      mode,
      warnings: computeWarnings(template, mode),
    });
  },

  setOffset: (offset) => set({ offset }),

  previous: () => {
    const source = get().source;
    if (!source) return;
    const current = stepVariableTextCurrent(source, -1);
    set({
      previewRow: current,
      source: updateSource(source, { current }),
    });
  },

  next: () => {
    const source = get().source;
    if (!source) return;
    const current = stepVariableTextCurrent(source, 1);
    set({
      previewRow: current,
      source: updateSource(source, { current }),
    });
  },

  reset: () => {
    const source = get().source;
    if (!source) return;
    const current = resetVariableTextCurrent(source);
    set({
      previewRow: current,
      source: updateSource(source, { current }),
    });
  },

  advanceSerial: () => {
    get().next();
  },

  buildConfig: () => {
    const template = get().template;
    const source = get().source;
    if (!template || !source) return null;
    const persistedSource = { ...source };
    delete persistedSource.currentRow;
    return {
      template,
      mode: get().mode,
      offset: get().offset,
      source: persistedSource,
    };
  },

  captureJobStartTime: () => set({ jobStartTime: new Date().toISOString() }),
  clearJobStartTime: () => set({ jobStartTime: null }),
}));
