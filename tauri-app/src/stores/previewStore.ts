import { create } from 'zustand';
import type { PreviewData } from '../types/preview';
import { previewService } from '../services/previewService';
import { useNotificationStore } from './notificationStore';
import { wrapBackendError } from '../i18n/errors';
import { sessionJobOptions } from '../types/jobOptions';

export type PreviewState = 'idle' | 'generating' | 'current' | 'stale' | 'error';

const AUTO_REFRESH_MAX_DURATION_MS = 250;
const AUTO_REFRESH_DEBOUNCE_MS = 500;
const PREVIEW_GENERATION_DIALOG_REVEAL_MS = 300;
const DEFAULT_PREVIEW_GENERATION_DIALOG_TITLE = 'Generating preview...';
const OFFSET_FILL_GENERATION_DIALOG_TITLE = 'Generating offset fills...';

function now(): number {
  return typeof performance !== 'undefined' ? performance.now() : Date.now();
}

interface PreviewStoreState {
  state: PreviewState;
  data: PreviewData | null;
  revisionHash: string | null;
  error: string | null;
  showPreview: boolean;
  previewWindowOpen: boolean;
  previewGenerationDialogVisible: boolean;
  previewGenerationDialogTitle: string;
  manualRefreshRequired: boolean;
  interactionActive: boolean;
  lastSuccessfulDurationMs: number | null;
  pendingInteractionRefresh: boolean;

  generatePreview: () => Promise<boolean>;
  cancelPreviewGeneration: () => Promise<void>;
  refreshPreview: () => Promise<boolean>;
  invalidate: () => void;
  togglePreview: () => void;
  clearPreview: () => void;
  openPreviewWindow: () => void;
  closePreviewWindow: () => void;
  setInteractionActive: (active: boolean) => void;
}

let debounceTimer: ReturnType<typeof setTimeout> | null = null;
let previewGenerationRevealTimer: ReturnType<typeof setTimeout> | null = null;
let lastToastedWarnings: string[] = [];
let previewEpoch = 0;
let latestRequestId = 0;

function clearPreviewGenerationRevealTimer(): void {
  if (previewGenerationRevealTimer) {
    clearTimeout(previewGenerationRevealTimer);
    previewGenerationRevealTimer = null;
  }
}

function isEmptyPlanError(msg: string): boolean {
  return msg.includes('Cannot build plan from empty project')
    || msg.includes('Plan generation failed: Cannot build plan from empty project')
    || msg.includes('EmptyPlan');
}

function isPlanningCancelledError(msg: string): boolean {
  return msg.includes('Plan generation cancelled');
}

async function hasEstimatedLongOffsetFill(): Promise<boolean> {
  const { useProjectStore } = await import('./projectStore');
  const project = useProjectStore.getState().project;
  if (!project) return false;

  let totalEstimate = 0;
  let warned = false;
  for (const layer of project.layers) {
    if (!layer.enabled || layer.is_tool_layer) continue;
    const objects = project.objects.filter((object) =>
      object.layer_id === layer.id && object.visible && !object.locked
    );
    if (objects.length === 0) continue;

    for (const entry of layer.entries) {
      if (!entry.output_enabled || entry.operation !== 'offset_fill') continue;
      const spacing = entry.raster_settings?.line_interval_mm ?? 0.5;
      if (spacing <= 0) continue;
      const entryEstimate = objects.reduce((sum, object) => {
        const width = Math.max(0, object.bounds.max.x - object.bounds.min.x);
        const height = Math.max(0, object.bounds.max.y - object.bounds.min.y);
        return sum + Math.ceil(Math.min(width, height) / (2 * spacing));
      }, 0);
      totalEstimate += entryEstimate;
      if (entryEstimate > 256) warned = true;
    }
  }

  return warned || totalEstimate > 512;
}

function schedulePreviewGenerationDialog(requestId: number, epochAtStart: number): void {
  clearPreviewGenerationRevealTimer();
  previewGenerationRevealTimer = setTimeout(() => {
    previewGenerationRevealTimer = null;
    const state = usePreviewStore.getState();
    if (
      requestId === latestRequestId
      && epochAtStart === previewEpoch
      && state.state === 'generating'
    ) {
      usePreviewStore.setState({ previewGenerationDialogVisible: true });
    }
  }, PREVIEW_GENERATION_DIALOG_REVEAL_MS);
}

export const usePreviewStore = create<PreviewStoreState>((set, get) => ({
  state: 'idle',
  data: null,
  revisionHash: null,
  error: null,
  showPreview: false,
  previewWindowOpen: false,
  previewGenerationDialogVisible: false,
  previewGenerationDialogTitle: DEFAULT_PREVIEW_GENERATION_DIALOG_TITLE,
  manualRefreshRequired: false,
  interactionActive: false,
  lastSuccessfulDurationMs: null,
  pendingInteractionRefresh: false,

  generatePreview: async () => {
    const requestId = ++latestRequestId;
    const epochAtStart = previewEpoch;
    const startedAt = now();
    clearPreviewGenerationRevealTimer();
    try {
      set({
        state: 'generating',
        error: null,
        previewGenerationDialogVisible: false,
        previewGenerationDialogTitle: DEFAULT_PREVIEW_GENERATION_DIALOG_TITLE,
        manualRefreshRequired: false,
        pendingInteractionRefresh: false,
      });
      schedulePreviewGenerationDialog(requestId, epochAtStart);
      void hasEstimatedLongOffsetFill()
        .then((hasLongOffsetFill) => {
          if (
            hasLongOffsetFill
            && requestId === latestRequestId
            && epochAtStart === previewEpoch
            && usePreviewStore.getState().state === 'generating'
          ) {
            usePreviewStore.setState({ previewGenerationDialogTitle: OFFSET_FILL_GENERATION_DIALOG_TITLE });
          }
        })
        .catch(() => undefined);
      const [{ useUiStore }, { useProjectStore }] = await Promise.all([
        import('./uiStore'),
        import('./projectStore'),
      ]);
      const data = await previewService.generatePreview(
        sessionJobOptions(useUiStore.getState().jobOptions, useProjectStore.getState().selectedObjectIds),
      );
      if (requestId !== latestRequestId || epochAtStart !== previewEpoch) {
        return false;
      }
      clearPreviewGenerationRevealTimer();
      set({
        state: 'current',
        data,
        revisionHash: data.revision_hash,
        previewGenerationDialogVisible: false,
        previewGenerationDialogTitle: DEFAULT_PREVIEW_GENERATION_DIALOG_TITLE,
        lastSuccessfulDurationMs: now() - startedAt,
        manualRefreshRequired: false,
        pendingInteractionRefresh: false,
      });
      // Surface plan warnings and failed-entry omissions only when the set changes.
      const messages = [...data.warnings, ...data.failed_entries];
      const sorted = [...messages].sort();
      const changed = sorted.length !== lastToastedWarnings.length
        || sorted.some((w, i) => w !== lastToastedWarnings[i]);
      if (changed) {
        lastToastedWarnings = sorted;
        for (const w of data.warnings) {
          useNotificationStore.getState().push(w, 'warning');
        }
        for (const failure of data.failed_entries) {
          useNotificationStore.getState().push(failure, 'error');
        }
      }
      return true;
    } catch (e) {
      if (requestId !== latestRequestId || epochAtStart !== previewEpoch) {
        return false;
      }
      clearPreviewGenerationRevealTimer();
      const msg = String(e);
      if (isEmptyPlanError(msg)) {
        lastToastedWarnings = [];
        previewEpoch += 1;
        set({
          state: 'idle',
          data: null,
          revisionHash: null,
          error: null,
          showPreview: false,
          previewWindowOpen: false,
          previewGenerationDialogVisible: false,
          previewGenerationDialogTitle: DEFAULT_PREVIEW_GENERATION_DIALOG_TITLE,
          manualRefreshRequired: false,
          interactionActive: false,
          lastSuccessfulDurationMs: null,
          pendingInteractionRefresh: false,
        });
        return false;
      }
      if (isPlanningCancelledError(msg)) {
        set({
          state: 'stale',
          error: null,
          previewGenerationDialogVisible: false,
          previewGenerationDialogTitle: DEFAULT_PREVIEW_GENERATION_DIALOG_TITLE,
          manualRefreshRequired: true,
          pendingInteractionRefresh: false,
        });
        return false;
      }
      set({
        state: 'error',
        error: msg,
        previewGenerationDialogVisible: false,
        previewGenerationDialogTitle: DEFAULT_PREVIEW_GENERATION_DIALOG_TITLE,
      });
      useNotificationStore.getState().push(wrapBackendError(msg), 'error');
      return false;
    }
  },

  cancelPreviewGeneration: async () => {
    latestRequestId += 1;
    previewEpoch += 1;
    clearPreviewGenerationRevealTimer();
    if (debounceTimer) {
      clearTimeout(debounceTimer);
      debounceTimer = null;
    }
    await previewService.cancelPlanning();
    set({
      state: 'stale',
      error: null,
      previewGenerationDialogVisible: false,
      previewGenerationDialogTitle: DEFAULT_PREVIEW_GENERATION_DIALOG_TITLE,
      manualRefreshRequired: true,
      pendingInteractionRefresh: false,
    });
  },

  refreshPreview: async () => get().generatePreview(),

  invalidate: () => {
    const {
      previewWindowOpen,
      state,
      interactionActive,
      lastSuccessfulDurationMs,
    } = get();

    if (state === 'idle' && !previewWindowOpen) return;

    previewEpoch += 1;
    const visible = previewWindowOpen;
    const canAutoRefresh =
      visible &&
      !interactionActive &&
      (lastSuccessfulDurationMs === null || lastSuccessfulDurationMs <= AUTO_REFRESH_MAX_DURATION_MS);

    set({
      state: 'stale',
      manualRefreshRequired: visible && !interactionActive && !canAutoRefresh,
      pendingInteractionRefresh: visible && interactionActive,
    });

    // Auto-regenerate with debounce when overlay or window is visible
    if (debounceTimer) {
      clearTimeout(debounceTimer);
      debounceTimer = null;
    }
    if (canAutoRefresh) {
      debounceTimer = setTimeout(() => {
        debounceTimer = null;
        void get().generatePreview();
      }, AUTO_REFRESH_DEBOUNCE_MS);
    }
  },

  togglePreview: () => {
    const { previewWindowOpen } = get();
    if (previewWindowOpen) {
      get().closePreviewWindow();
      return;
    }
    get().openPreviewWindow();
  },

  clearPreview: () => {
    if (debounceTimer) {
      clearTimeout(debounceTimer);
      debounceTimer = null;
    }
    clearPreviewGenerationRevealTimer();
    lastToastedWarnings = [];
    previewEpoch += 1;
    set({
      state: 'idle',
      data: null,
      revisionHash: null,
      error: null,
      showPreview: false,
      previewWindowOpen: false,
      previewGenerationDialogVisible: false,
      previewGenerationDialogTitle: DEFAULT_PREVIEW_GENERATION_DIALOG_TITLE,
      manualRefreshRequired: false,
      interactionActive: false,
      lastSuccessfulDurationMs: null,
      pendingInteractionRefresh: false,
    });
  },

  openPreviewWindow: () => {
    const { state, data } = get();
    if (state === 'current' && data) {
      set({ previewWindowOpen: true, showPreview: false });
    } else {
      set({ previewWindowOpen: false, showPreview: false });
    }
    // Generate data if needed — also retry on error
    if (state === 'idle' || state === 'stale' || state === 'error') {
      void get().generatePreview().then((ready) => {
        if (ready) {
          set({ previewWindowOpen: true, showPreview: false });
        }
      });
    }
  },

  closePreviewWindow: () => {
    if (get().state === 'generating') {
      void get().cancelPreviewGeneration();
    }
    set({ previewWindowOpen: false, showPreview: false });
  },

  setInteractionActive: (active: boolean) => {
    const {
      state,
      previewWindowOpen,
      pendingInteractionRefresh,
      lastSuccessfulDurationMs,
    } = get();

    set({ interactionActive: active });

    if (active || !pendingInteractionRefresh || state !== 'stale' || !previewWindowOpen) {
      return;
    }

    const canAutoRefresh =
      lastSuccessfulDurationMs === null || lastSuccessfulDurationMs <= AUTO_REFRESH_MAX_DURATION_MS;
    if (debounceTimer) {
      clearTimeout(debounceTimer);
      debounceTimer = null;
    }
    if (canAutoRefresh) {
      debounceTimer = setTimeout(() => {
        debounceTimer = null;
        void get().generatePreview();
      }, AUTO_REFRESH_DEBOUNCE_MS);
      set({ manualRefreshRequired: false, pendingInteractionRefresh: false });
      return;
    }
    set({ manualRefreshRequired: true, pendingInteractionRefresh: false });
  },
}));
