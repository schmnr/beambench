import { create, type StateCreator } from 'zustand';
import type { ArtLibraryItem, LoadedArtLibrary } from '../types/artLibrary';
import { artLibraryService } from '../services/artLibraryService';
import { useNotificationStore } from './notificationStore';
import { projectService } from '../services/projectService';
import { usePreviewStore } from './previewStore';
import { useProjectStore } from './projectStore';
import { useUndoStore } from './undoStore';
import { generateArtLibraryThumbnail } from '../components/panels/artLibraryThumbnails';

const notify = (msg: string, type: 'success' | 'error' | 'warning' | 'info' = 'info') =>
  useNotificationStore.getState().push(msg, type);

const repairingThumbnails = new Set<string>();

type DragEffect = 'copy' | 'move';

interface ArtLibraryDragState {
  sourceLibraryId: string;
  itemId: string;
  dropEffect: DragEffect;
  dropAllowed: boolean;
  targetLibraryId: string | null;
}

interface ArtLibraryState {
  libraries: LoadedArtLibrary[];
  selectedLibraryId: string | null;
  searchQuery: string;
  selectedCategory: string | null;
  dragState: ArtLibraryDragState | null;

  loadLibraries: () => Promise<void>;
  createLibrary: (path: string, name: string) => Promise<LoadedArtLibrary | null>;
  loadLibrary: (path: string) => Promise<LoadedArtLibrary | null>;
  unloadLibrary: (libraryId: string) => Promise<void>;
  saveLibraryAs: (libraryId: string, path: string) => Promise<LoadedArtLibrary | null>;
  renameLibrary: (libraryId: string, name: string) => Promise<void>;
  deleteLibrary: (libraryId: string) => Promise<void>;
  addFileItem: (
    libraryId: string,
    filePath: string,
    name: string,
    category: string,
    tags: string[],
  ) => Promise<void>;
  addFileItems: (
    libraryId: string,
    items: Array<{ filePath: string; name: string }>,
    category: string,
    tags: string[],
  ) => Promise<void>;
  addSelectionItem: (
    libraryId: string,
    name: string,
    category: string,
    tags: string[],
  ) => Promise<void>;
  renameItem: (libraryId: string, itemId: string, name: string) => Promise<void>;
  removeItem: (libraryId: string, itemId: string) => Promise<boolean>;
  insertToProject: (
    libraryId: string,
    itemId: string,
    drop?: { x: number; y: number },
  ) => Promise<void>;
  moveItem: (
    sourceLibraryId: string,
    itemId: string,
    targetLibraryId: string,
    removeSource: boolean,
  ) => Promise<void>;
  setSelectedLibrary: (libraryId: string | null) => void;
  setSearchQuery: (q: string) => void;
  setSelectedCategory: (cat: string | null) => void;
  setDragState: (dragState: ArtLibraryDragState | null) => void;
}

function syncSelection(
  selectedLibraryId: string | null,
  libraries: LoadedArtLibrary[],
): Pick<ArtLibraryState, 'selectedLibraryId' | 'selectedCategory'> {
  const stillExists = selectedLibraryId
    ? libraries.some((library) => library.library_id === selectedLibraryId)
    : false;
  return {
    selectedLibraryId: stillExists ? selectedLibraryId : libraries[0]?.library_id ?? null,
    selectedCategory: null,
  };
}

type ArtLibrarySet = Parameters<StateCreator<ArtLibraryState>>[0];

async function refreshLibraries(set: ArtLibrarySet, get: () => ArtLibraryState): Promise<void> {
  const state = await artLibraryService.getArtLibraries();
  set((current) => ({
    libraries: state.libraries,
    ...syncSelection(current.selectedLibraryId, state.libraries),
  }));
  for (const warning of state.warnings) {
    notify(warning, 'warning');
  }
  void repairMissingThumbnails(get);
}

async function repairMissingThumbnails(get: () => ArtLibraryState): Promise<void> {
  const libraries = get().libraries;
  for (const library of libraries) {
    for (const item of library.items) {
      if (item.thumbnail || repairingThumbnails.has(item.id)) continue;
      repairingThumbnails.add(item.id);
      void (async () => {
        try {
          const thumbnail = await generateArtLibraryThumbnail(item);
          if (thumbnail) {
            await artLibraryService.commitArtLibraryThumbnail(library.library_id, item.id, thumbnail);
          }
        } catch {
          // Leave placeholder rendering in place if repair fails.
        } finally {
          repairingThumbnails.delete(item.id);
        }
      })();
    }
  }
}

async function persistThumbnail(libraryId: string, item: ArtLibraryItem): Promise<void> {
  const thumbnail = await generateArtLibraryThumbnail(item);
  if (!thumbnail) return;
  await artLibraryService.commitArtLibraryThumbnail(libraryId, item.id, thumbnail);
}

function formatErrorMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  return String(error);
}

export const useArtLibraryStore = create<ArtLibraryState>((set, get) => ({
  libraries: [],
  selectedLibraryId: null,
  searchQuery: '',
  selectedCategory: null,
  dragState: null,

  loadLibraries: async () => {
    try {
      await refreshLibraries(set, get);
    } catch (e) {
      notify(`Failed to load art libraries: ${e}`, 'error');
    }
  },

  createLibrary: async (path, name) => {
    try {
      const library = await artLibraryService.createArtLibrary(path, name);
      await refreshLibraries(set, get);
      set({ selectedLibraryId: library.library_id });
      notify(`Library "${library.name}" created`, 'success');
      return library;
    } catch (e) {
      notify(`Failed to create library: ${e}`, 'error');
      return null;
    }
  },

  loadLibrary: async (path) => {
    try {
      const library = await artLibraryService.loadArtLibrary(path);
      await refreshLibraries(set, get);
      set({ selectedLibraryId: library.library_id });
      notify(`Loaded "${library.name}"`, 'success');
      return library;
    } catch (e) {
      notify(`Failed to load library: ${e}`, 'error');
      return null;
    }
  },

  unloadLibrary: async (libraryId) => {
    try {
      await artLibraryService.unloadArtLibrary(libraryId);
      await refreshLibraries(set, get);
      notify('Library unloaded', 'success');
    } catch (e) {
      notify(`Failed to unload library: ${e}`, 'error');
    }
  },

  saveLibraryAs: async (libraryId, path) => {
    try {
      const library = await artLibraryService.saveArtLibraryAs(libraryId, path);
      await refreshLibraries(set, get);
      set({ selectedLibraryId: library.library_id });
      notify(`Saved "${library.name}"`, 'success');
      return library;
    } catch (e) {
      notify(`Failed to save library: ${e}`, 'error');
      return null;
    }
  },

  renameLibrary: async (libraryId, name) => {
    try {
      const library = await artLibraryService.renameArtLibrary(libraryId, name);
      set((state) => ({
        libraries: state.libraries.map((entry) => (entry.library_id === libraryId ? library : entry)),
      }));
      if (library.save_error) {
        notify(`Library renamed, but save failed: ${library.save_error}`, 'error');
      }
    } catch (e) {
      notify(`Failed to rename library: ${e}`, 'error');
    }
  },

  deleteLibrary: async (libraryId) => {
    try {
      await artLibraryService.deleteArtLibrary(libraryId);
      await refreshLibraries(set, get);
      notify('Library deleted', 'success');
    } catch (e) {
      notify(`Failed to delete library: ${e}`, 'error');
    }
  },

  addFileItem: async (libraryId, filePath, name, category, tags) => {
    await get().addFileItems(libraryId, [{ filePath, name }], category, tags);
  },

  addFileItems: async (libraryId, items, category, tags) => {
    const queuedItems = items.filter((item) => item.filePath);
    if (queuedItems.length === 0) return;

    const addedNames: string[] = [];
    const duplicateNames: string[] = [];
    const failedItems: Array<{ name: string; error: string }> = [];

    for (const item of queuedItems) {
      try {
        const result = await artLibraryService.addArtLibraryItem(
          libraryId,
          item.name,
          category,
          tags,
          item.filePath,
        );
        if (result.duplicate) {
          duplicateNames.push(item.name);
          continue;
        }
        const createdItem = result.item;
        await persistThumbnail(libraryId, createdItem);
        addedNames.push(item.name);
      } catch (error) {
        failedItems.push({ name: item.name, error: formatErrorMessage(error) });
      }
    }

    if (addedNames.length > 0) {
      await refreshLibraries(set, get);
      notify(
        addedNames.length === 1
          ? `Added "${addedNames[0]}" to library`
          : `Added ${addedNames.length} items to library`,
        'success',
      );
    }

    if (duplicateNames.length > 0) {
      notify(
        duplicateNames.length === 1
          ? `Skipped duplicate "${duplicateNames[0]}"`
          : `Skipped ${duplicateNames.length} duplicate items`,
        'warning',
      );
    }

    if (failedItems.length > 0) {
      const firstFailure = failedItems[0];
      notify(
        failedItems.length === 1
          ? `Failed to add "${firstFailure.name}" to library: ${firstFailure.error}`
          : `Failed to add ${failedItems.length} items to library (first error: ${firstFailure.error})`,
        'error',
      );
    }
  },

  addSelectionItem: async (libraryId, name, category, tags) => {
    try {
      const selectedIds = useProjectStore.getState().selectedObjectIds;
      if (selectedIds.length === 0) {
        notify('Select artwork on the canvas first', 'warning');
        return;
      }
      const item = await artLibraryService.addSelectionToArtLibrary(
        libraryId,
        selectedIds,
        name,
        category,
        tags,
      );
      await persistThumbnail(libraryId, item);
      await refreshLibraries(set, get);
      notify(`Captured "${name}"`, 'success');
    } catch (e) {
      notify(`Failed to add selection: ${e}`, 'error');
    }
  },

  renameItem: async (libraryId, itemId, name) => {
    try {
      const library = await artLibraryService.renameArtLibraryItem(libraryId, itemId, name);
      set((state) => ({
        libraries: state.libraries.map((entry) => (entry.library_id === libraryId ? library : entry)),
      }));
      if (library.save_error) {
        notify(`Item renamed, but save failed: ${library.save_error}`, 'error');
      }
    } catch (e) {
      notify(`Failed to rename item: ${e}`, 'error');
    }
  },

  removeItem: async (libraryId, itemId) => {
    try {
      const library = await artLibraryService.removeArtLibraryItem(libraryId, itemId);
      set((state) => ({
        libraries: state.libraries.map((entry) => (entry.library_id === libraryId ? library : entry)),
      }));
      if (library.save_error) {
        notify(`Item removed, but save failed: ${library.save_error}`, 'error');
      }
      return true;
    } catch (e) {
      notify(`Failed to remove item: ${e}`, 'error');
      return false;
    }
  },

  insertToProject: async (libraryId, itemId, drop) => {
    try {
      const projectState = useProjectStore.getState();
      if (!projectState.project) {
        notify('Create or open a project before inserting art', 'warning');
        return;
      }
      const fallbackLayerId = projectState.selectedLayerId ?? projectState.project.layers[0]?.id;

      const importedObjects = await artLibraryService.insertArtLibraryItemToProject(
        libraryId,
        itemId,
        fallbackLayerId,
        drop?.x,
        drop?.y,
      );
      const refreshed = await projectService.getProject();
      if (!refreshed || importedObjects.length === 0) {
        throw new Error('Art library insert did not create any project objects');
      }

      const destLayerIds: string[] = [];
      const destCounts = new Map<string, number>();
      for (const obj of importedObjects) {
        const layerId = obj.layer_id;
        if (!destCounts.has(layerId)) destLayerIds.push(layerId);
        destCounts.set(layerId, (destCounts.get(layerId) ?? 0) + 1);
      }

      let selectedLayerId = fallbackLayerId ?? null;
      if ((!selectedLayerId || !destCounts.has(selectedLayerId)) && destLayerIds.length > 0) {
        selectedLayerId = destLayerIds[0];
      }

      useProjectStore.setState({
        project: { ...refreshed, dirty: true },
        selectedLayerId,
        selectedObjectIds: importedObjects.map((obj) => obj.id),
      });
      usePreviewStore.getState().invalidate();
      await useUndoStore.getState().refresh();
      notify('Item inserted into project', 'success');
    } catch (e) {
      notify(`Failed to insert item: ${e}`, 'error');
    }
  },

  moveItem: async (sourceLibraryId, itemId, targetLibraryId, removeSource) => {
    try {
      await artLibraryService.moveArtLibraryItem(sourceLibraryId, itemId, targetLibraryId, removeSource);
      await refreshLibraries(set, get);
    } catch (e) {
      notify(`Failed to move item: ${e}`, 'error');
    }
  },

  setSelectedLibrary: (selectedLibraryId) => set({ selectedLibraryId, selectedCategory: null }),
  setSearchQuery: (searchQuery) => set({ searchQuery }),
  setSelectedCategory: (selectedCategory) => set({ selectedCategory }),
  setDragState: (dragState) => set({ dragState }),
}));
