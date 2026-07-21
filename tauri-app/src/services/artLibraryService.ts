import type { ProjectObject } from '../types/project';
import { invoke } from '@tauri-apps/api/core';
import type {
  ArtLibraryAddItemResult,
  ArtLibraryItem,
  ArtLibraryLoadState,
  LoadedArtLibrary,
} from '../types/artLibrary';

function isLoadedArtLibrary(value: unknown): value is LoadedArtLibrary {
  return !!value && typeof value === 'object' && 'library_id' in value && 'items' in value;
}

function normalizeArtLibraryLoadState(payload: unknown): ArtLibraryLoadState {
  if (Array.isArray(payload)) {
    return {
      libraries: payload.filter(isLoadedArtLibrary),
      warnings: [],
    };
  }

  if (payload && typeof payload === 'object') {
    const candidate = payload as Partial<ArtLibraryLoadState>;
    return {
      libraries: Array.isArray(candidate.libraries)
        ? candidate.libraries.filter(isLoadedArtLibrary)
        : [],
      warnings: Array.isArray(candidate.warnings)
        ? candidate.warnings.filter((warning): warning is string => typeof warning === 'string')
        : [],
    };
  }

  return { libraries: [], warnings: [] };
}

export const artLibraryService = {
  async getArtLibraries(): Promise<ArtLibraryLoadState> {
    return normalizeArtLibraryLoadState(await invoke<unknown>('get_art_libraries'));
  },

  async createArtLibrary(path: string, name: string): Promise<LoadedArtLibrary> {
    return invoke<LoadedArtLibrary>('create_art_library', { path, name });
  },

  async loadArtLibrary(path: string): Promise<LoadedArtLibrary> {
    return invoke<LoadedArtLibrary>('load_art_library', { path });
  },

  async unloadArtLibrary(libraryId: string): Promise<void> {
    return invoke<void>('unload_art_library', { libraryId });
  },

  async saveArtLibraryAs(libraryId: string, path: string): Promise<LoadedArtLibrary> {
    return invoke<LoadedArtLibrary>('save_art_library_as', { libraryId, path });
  },

  async renameArtLibrary(libraryId: string, name: string): Promise<LoadedArtLibrary> {
    return invoke<LoadedArtLibrary>('rename_art_library', { libraryId, name });
  },

  async deleteArtLibrary(libraryId: string): Promise<void> {
    return invoke<void>('delete_art_library', { libraryId });
  },

  async addArtLibraryItem(
    libraryId: string,
    name: string,
    category: string,
    tags: string[],
    filePath: string,
  ): Promise<ArtLibraryAddItemResult> {
    return invoke<ArtLibraryAddItemResult>('add_art_library_item', {
      libraryId,
      name,
      category,
      tags,
      filePath,
    });
  },

  async addSelectionToArtLibrary(
    libraryId: string,
    objectIds: string[],
    name: string,
    category: string,
    tags: string[],
  ): Promise<ArtLibraryItem> {
    return invoke<ArtLibraryItem>('add_selection_to_art_library', {
      libraryId,
      objectIds,
      name,
      category,
      tags,
    });
  },

  async renameArtLibraryItem(libraryId: string, itemId: string, name: string): Promise<LoadedArtLibrary> {
    return invoke<LoadedArtLibrary>('rename_art_library_item', { libraryId, itemId, name });
  },

  async removeArtLibraryItem(libraryId: string, itemId: string): Promise<LoadedArtLibrary> {
    return invoke<LoadedArtLibrary>('remove_art_library_item', { libraryId, itemId });
  },

  async commitArtLibraryThumbnail(
    libraryId: string,
    itemId: string,
    thumbnail: string | null,
  ): Promise<LoadedArtLibrary> {
    return invoke<LoadedArtLibrary>('commit_art_library_thumbnail', {
      libraryId,
      itemId,
      thumbnail,
    });
  },

  async moveArtLibraryItem(
    sourceLibraryId: string,
    itemId: string,
    targetLibraryId: string,
    removeSource: boolean,
  ): Promise<void> {
    return invoke<void>('move_art_library_item', {
      sourceLibraryId,
      itemId,
      targetLibraryId,
      removeSource,
    });
  },

  async insertArtLibraryItemToProject(
    libraryId: string,
    itemId: string,
    layerId?: string,
    dropX?: number,
    dropY?: number,
  ): Promise<ProjectObject[]> {
    const payload: Record<string, unknown> = {
      libraryId,
      itemId,
      dropX,
      dropY,
    };
    if (layerId) payload.layerId = layerId;
    return invoke<ProjectObject[]>('insert_art_library_item_to_project', payload);
  },
};
