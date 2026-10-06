import { create } from 'zustand';
import { projectService } from '../services/projectService';
import { decorateProject, getDocumentGeneration, useProjectStore } from './projectStore';
import { usePreviewStore } from './previewStore';
import { useNotificationStore } from './notificationStore';
import { wrapBackendError } from '../i18n/errors';

interface UndoStoreState {
  canUndo: boolean;
  canRedo: boolean;
  refresh: () => Promise<void>;
  undo: () => Promise<void>;
  redo: () => Promise<void>;
  clear: () => void;
}

function syncProjectSelection() {
  const { project, selectedLayerId, selectedObjectIds } = useProjectStore.getState();
  if (!project) {
    useProjectStore.setState({ selectedLayerId: null, selectedObjectIds: [], pendingPaletteColor: null });
    return;
  }

  const layerIds = new Set(project.layers.map((layer) => layer.id));
  const objectIds = new Set(project.objects.map((obj) => obj.id));

  useProjectStore.setState({
    selectedLayerId: selectedLayerId && layerIds.has(selectedLayerId) ? selectedLayerId : null,
    selectedObjectIds: selectedObjectIds.filter((id) => objectIds.has(id)),
    pendingPaletteColor: null,
  });
}

function isBenignHistoryError(error: unknown, action: 'undo' | 'redo') {
  return String(error).toLowerCase().includes(`nothing to ${action}`);
}

export const useUndoStore = create<UndoStoreState>((set) => ({
  canUndo: false,
  canRedo: false,

  refresh: async () => {
    const generation = getDocumentGeneration();
    try {
      const state = await projectService.getUndoState();
      if (generation !== getDocumentGeneration()) return;
      set({
        canUndo: state.can_undo,
        canRedo: state.can_redo,
      });
    } catch {
      if (generation !== getDocumentGeneration()) return;
      set({ canUndo: false, canRedo: false });
    }
  },

  undo: async () => {
    const generation = getDocumentGeneration();
    try {
      const project = await projectService.undoProject();
      if (generation !== getDocumentGeneration()) return;
      useProjectStore.setState({ project: decorateProject(project) });
      syncProjectSelection();
      usePreviewStore.getState().invalidate();
      const state = await projectService.getUndoState();
      if (generation !== getDocumentGeneration()) return;
      set({
        canUndo: state.can_undo,
        canRedo: state.can_redo,
      });
    } catch (error) {
      if (generation !== getDocumentGeneration()) return;
      await useUndoStore.getState().refresh();
      if (!isBenignHistoryError(error, 'undo')) {
        useNotificationStore.getState().push(wrapBackendError(String(error)), 'error');
      }
    }
  },

  redo: async () => {
    const generation = getDocumentGeneration();
    try {
      const project = await projectService.redoProject();
      if (generation !== getDocumentGeneration()) return;
      useProjectStore.setState({ project: decorateProject(project) });
      syncProjectSelection();
      usePreviewStore.getState().invalidate();
      const state = await projectService.getUndoState();
      if (generation !== getDocumentGeneration()) return;
      set({
        canUndo: state.can_undo,
        canRedo: state.can_redo,
      });
    } catch (error) {
      if (generation !== getDocumentGeneration()) return;
      await useUndoStore.getState().refresh();
      if (!isBenignHistoryError(error, 'redo')) {
        useNotificationStore.getState().push(wrapBackendError(String(error)), 'error');
      }
    }
  },

  clear: () => {
    set({ canUndo: false, canRedo: false });
  },
}));
