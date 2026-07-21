import { describe, it, expect, vi, beforeEach } from 'vitest';

import { useArtLibraryStore } from '../artLibraryStore';
import { useProjectStore } from '../projectStore';
import { usePreviewStore } from '../previewStore';
import { useUndoStore } from '../undoStore';
import { makeLayer, makeProject, makeProjectObject } from '../../test-utils/projectFixtures';
import type { LoadedArtLibrary, ArtLibraryItem } from '../../types/artLibrary';

vi.mock('../../services/artLibraryService', () => ({
  artLibraryService: {
    getArtLibraries: vi.fn(),
    createArtLibrary: vi.fn(),
    loadArtLibrary: vi.fn(),
    unloadArtLibrary: vi.fn(),
    saveArtLibraryAs: vi.fn(),
    renameArtLibrary: vi.fn(),
    deleteArtLibrary: vi.fn(),
    addArtLibraryItem: vi.fn(),
    addSelectionToArtLibrary: vi.fn(),
    renameArtLibraryItem: vi.fn(),
    removeArtLibraryItem: vi.fn(),
    commitArtLibraryThumbnail: vi.fn(),
    moveArtLibraryItem: vi.fn(),
    insertArtLibraryItemToProject: vi.fn(),
  },
}));

vi.mock('../../services/projectService', () => ({
  projectService: {
    getProject: vi.fn(),
  },
}));

vi.mock('../../components/panels/artLibraryThumbnails', () => ({
  generateArtLibraryThumbnail: vi.fn().mockResolvedValue(null),
}));

import { artLibraryService } from '../../services/artLibraryService';
import { projectService } from '../../services/projectService';

const mockedService = artLibraryService as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockedProjectService = projectService as unknown as {
  getProject: ReturnType<typeof vi.fn>;
};

const initialProjectState = useProjectStore.getState();
const initialPreviewState = usePreviewStore.getState();
const initialUndoState = useUndoStore.getState();

function makeItem(overrides: Partial<ArtLibraryItem> = {}): ArtLibraryItem {
  return {
    id: 'item-1',
    kind: 'external_file',
    name: 'Star',
    category: 'General',
    tags: ['star'],
    source_filename: 'star.svg',
    media_type: 'image/svg+xml',
    data: 'abc',
    thumbnail: 'thumb',
    created_at: '2026-01-01T00:00:00Z',
    ...overrides,
  };
}

function makeLibrary(overrides: Partial<LoadedArtLibrary> = {}): LoadedArtLibrary {
  return {
    format_version: '1.0',
    library_id: 'library-1',
    name: 'Shapes',
    items: [],
    path: '/tmp/Shapes.bbart',
    ...overrides,
  };
}

beforeEach(() => {
  useArtLibraryStore.setState({
    libraries: [],
    selectedLibraryId: null,
    searchQuery: '',
    selectedCategory: null,
    dragState: null,
  });
  useProjectStore.setState(initialProjectState, true);
  usePreviewStore.setState(initialPreviewState, true);
  useUndoStore.setState(initialUndoState, true);
  vi.clearAllMocks();
});

describe('artLibraryStore', () => {
  it('initial state is empty', () => {
    const state = useArtLibraryStore.getState();
    expect(state.libraries).toEqual([]);
    expect(state.selectedLibraryId).toBeNull();
    expect(state.searchQuery).toBe('');
    expect(state.selectedCategory).toBeNull();
  });

  it('loadLibraries populates state and selects the first library id', async () => {
    mockedService.getArtLibraries.mockResolvedValue({
      libraries: [
        makeLibrary(),
        makeLibrary({ library_id: 'library-2', name: 'Icons', path: '/tmp/Icons.bbart' }),
      ],
      warnings: [],
    });

    await useArtLibraryStore.getState().loadLibraries();

    const state = useArtLibraryStore.getState();
    expect(state.libraries).toHaveLength(2);
    expect(state.selectedLibraryId).toBe('library-1');
  });

  it('createLibrary adds and selects the returned library', async () => {
    mockedService.createArtLibrary.mockResolvedValue(
      makeLibrary({ library_id: 'library-9', name: 'New Lib', path: '/tmp/New Lib.bbart' }),
    );
    mockedService.getArtLibraries.mockResolvedValue({
      libraries: [makeLibrary({ library_id: 'library-9', name: 'New Lib', path: '/tmp/New Lib.bbart' })],
      warnings: [],
    });

    await expect(
      useArtLibraryStore.getState().createLibrary('/tmp/New Lib.bbart', 'New Lib'),
    ).resolves.toMatchObject({ library_id: 'library-9', name: 'New Lib' });
    expect(mockedService.createArtLibrary).toHaveBeenCalledWith('/tmp/New Lib.bbart', 'New Lib');
    expect(useArtLibraryStore.getState().selectedLibraryId).toBe('library-9');
  });

  it('createLibrary returns null when creation fails', async () => {
    mockedService.createArtLibrary.mockRejectedValue('boom');

    await expect(
      useArtLibraryStore.getState().createLibrary('/tmp/New Lib.bbart', 'New Lib'),
    ).resolves.toBeNull();

    expect(useArtLibraryStore.getState().selectedLibraryId).toBeNull();
  });

  it('deleteLibrary clears selection if the active library disappears after refresh', async () => {
    useArtLibraryStore.setState({
      libraries: [makeLibrary()],
      selectedLibraryId: 'library-1',
    });
    mockedService.deleteArtLibrary.mockResolvedValue(undefined);
    mockedService.getArtLibraries.mockResolvedValue({ libraries: [], warnings: [] });

    await useArtLibraryStore.getState().deleteLibrary('library-1');

    expect(useArtLibraryStore.getState().selectedLibraryId).toBeNull();
  });

  it('setSearchQuery updates search', () => {
    useArtLibraryStore.getState().setSearchQuery('star');
    expect(useArtLibraryStore.getState().searchQuery).toBe('star');
  });

  it('setSelectedCategory updates category', () => {
    useArtLibraryStore.getState().setSelectedCategory('Shapes');
    expect(useArtLibraryStore.getState().selectedCategory).toBe('Shapes');
  });

  it('addFileItems imports multiple files and refreshes once', async () => {
    const firstItem = makeItem({ id: 'item-1', name: 'alpha', source_filename: 'alpha.svg' });
    const secondItem = makeItem({ id: 'item-2', name: 'beta', source_filename: 'beta.png' });

    mockedService.addArtLibraryItem
      .mockResolvedValueOnce({ item: firstItem, duplicate: false })
      .mockResolvedValueOnce({ item: secondItem, duplicate: false });
    mockedService.getArtLibraries.mockResolvedValue({
      libraries: [makeLibrary({ items: [firstItem, secondItem] })],
      warnings: [],
    });

    await useArtLibraryStore.getState().addFileItems(
      'library-1',
      [
        { filePath: '/tmp/alpha.svg', name: 'alpha' },
        { filePath: '/tmp/beta.png', name: 'beta' },
      ],
      'General',
      [],
    );

    expect(mockedService.addArtLibraryItem).toHaveBeenNthCalledWith(
      1,
      'library-1',
      'alpha',
      'General',
      [],
      '/tmp/alpha.svg',
    );
    expect(mockedService.addArtLibraryItem).toHaveBeenNthCalledWith(
      2,
      'library-1',
      'beta',
      'General',
      [],
      '/tmp/beta.png',
    );
    expect(mockedService.getArtLibraries).toHaveBeenCalledTimes(1);
    expect(useArtLibraryStore.getState().libraries[0]?.items).toEqual([firstItem, secondItem]);
  });

  it('addFileItems skips duplicate files without refreshing the library', async () => {
    const existingItem = makeItem({ id: 'item-1', name: 'alpha', source_filename: 'alpha.svg' });

    mockedService.addArtLibraryItem.mockResolvedValue({
      item: existingItem,
      duplicate: true,
    });

    await useArtLibraryStore.getState().addFileItems(
      'library-1',
      [{ filePath: '/tmp/alpha.svg', name: 'alpha' }],
      'General',
      [],
    );

    expect(mockedService.addArtLibraryItem).toHaveBeenCalledWith(
      'library-1',
      'alpha',
      'General',
      [],
      '/tmp/alpha.svg',
    );
    expect(mockedService.getArtLibraries).not.toHaveBeenCalled();
  });

  it('insertToProject imports the item, refreshes project state, and selects the inserted object', async () => {
    const invalidate = vi.fn();
    const refresh = vi.fn().mockResolvedValue(undefined);

    usePreviewStore.setState({ invalidate });
    useUndoStore.setState({ refresh });
    const lineLayer = makeLayer({
      id: 'layer-1',
      name: 'Line',
      operation: 'line',
      color_tag: '#000000',
      vector_settings: null,
    });
    const imageLayer = makeLayer({
      id: 'layer-2',
      name: 'Image',
      operation: 'image',
      order_index: 1,
      color_tag: '#000000',
      raster_settings: null,
      vector_settings: null,
    });
    useProjectStore.setState({
      project: makeProject({
        metadata: {
          format_version: '1',
          app_version: '0.1.0',
          project_id: 'proj-1',
          project_name: 'Test',
          created_at: '',
          modified_at: '',
        },
        workspace: { bed_width_mm: 400, bed_height_mm: 300, origin: 'top_left' },
        layers: [lineLayer, imageLayer],
        objects: [],
        assets: [],
      }),
      selectedLayerId: 'layer-1',
      selectedObjectIds: [],
    });

    mockedService.insertArtLibraryItemToProject.mockResolvedValue([
      { id: 'obj-1', layer_id: 'layer-2' },
    ]);
    mockedProjectService.getProject.mockResolvedValue(
      makeProject({
        metadata: {
          format_version: '1',
          app_version: '0.1.0',
          project_id: 'proj-1',
          project_name: 'Test',
          created_at: '',
          modified_at: '',
        },
        workspace: { bed_width_mm: 400, bed_height_mm: 300, origin: 'top_left' },
        layers: [lineLayer, imageLayer],
        objects: [makeProjectObject({ id: 'obj-1', layer_id: 'layer-2' })],
        assets: [],
      }),
    );

    await useArtLibraryStore.getState().insertToProject('library-1', 'item-1');

    expect(mockedService.insertArtLibraryItemToProject).toHaveBeenCalledWith(
      'library-1',
      'item-1',
      'layer-1',
      undefined,
      undefined,
    );
    expect(useProjectStore.getState().selectedObjectIds).toEqual(['obj-1']);
    expect(useProjectStore.getState().selectedLayerId).toBe('layer-2');
    expect(invalidate).toHaveBeenCalledOnce();
    expect(refresh).toHaveBeenCalledOnce();
  });

  it('insertToProject forwards drop coordinates to the backend insert command', async () => {
    const invalidate = vi.fn();
    const refresh = vi.fn().mockResolvedValue(undefined);

    usePreviewStore.setState({ invalidate });
    useUndoStore.setState({ refresh });
    const lineLayer = makeLayer({
      id: 'layer-1',
      name: 'Line',
      operation: 'line',
      color_tag: '#000000',
      vector_settings: null,
    });
    useProjectStore.setState({
      project: makeProject({
        metadata: {
          format_version: '1',
          app_version: '0.1.0',
          project_id: 'proj-drop',
          project_name: 'Drop Test',
          created_at: '',
          modified_at: '',
        },
        workspace: { bed_width_mm: 400, bed_height_mm: 300, origin: 'top_left' },
        layers: [lineLayer],
        objects: [],
        assets: [],
      }),
      selectedLayerId: 'layer-1',
      selectedObjectIds: [],
    });
    mockedService.insertArtLibraryItemToProject.mockResolvedValue([
      { id: 'obj-drop', layer_id: 'layer-1' },
    ]);
    mockedProjectService.getProject.mockResolvedValue(
      makeProject({
        metadata: {
          format_version: '1',
          app_version: '0.1.0',
          project_id: 'proj-drop',
          project_name: 'Drop Test',
          created_at: '',
          modified_at: '',
        },
        workspace: { bed_width_mm: 400, bed_height_mm: 300, origin: 'top_left' },
        layers: [lineLayer],
        objects: [makeProjectObject({ id: 'obj-drop', layer_id: 'layer-1' })],
        assets: [],
      }),
    );

    await useArtLibraryStore
      .getState()
      .insertToProject('library-1', 'item-1', { x: 100, y: 200 });

    expect(mockedService.insertArtLibraryItemToProject).toHaveBeenCalledWith(
      'library-1',
      'item-1',
      'layer-1',
      100,
      200,
    );
    expect(invalidate).toHaveBeenCalledOnce();
    expect(refresh).toHaveBeenCalledOnce();
  });

  it('insertToProject does not call the backend when there is no project layer', async () => {
    useProjectStore.setState({
      project: null,
      selectedLayerId: null,
      selectedObjectIds: [],
    });

    await expect(
      useArtLibraryStore.getState().insertToProject('library-1', 'item-1'),
    ).resolves.toBeUndefined();

    expect(mockedService.insertArtLibraryItemToProject).not.toHaveBeenCalled();
    expect(mockedProjectService.getProject).not.toHaveBeenCalled();
  });

  it('insertToProject still imports when a project exists but no active layer is selected', async () => {
    const invalidate = vi.fn();
    const refresh = vi.fn().mockResolvedValue(undefined);

    usePreviewStore.setState({ invalidate });
    useUndoStore.setState({ refresh });
    useProjectStore.setState({
      project: makeProject({
        metadata: {
          format_version: '1',
          app_version: '0.1.0',
          project_id: 'proj-2',
          project_name: 'Layerless Test',
          created_at: '',
          modified_at: '',
        },
        workspace: { bed_width_mm: 400, bed_height_mm: 300, origin: 'top_left' },
        layers: [],
        objects: [],
        assets: [],
      }),
      selectedLayerId: null,
      selectedObjectIds: [],
    });

    mockedService.insertArtLibraryItemToProject.mockResolvedValue([
      { id: 'obj-9', layer_id: 'layer-created' },
    ]);
    mockedProjectService.getProject.mockResolvedValue(
      makeProject({
        metadata: {
          format_version: '1',
          app_version: '0.1.0',
          project_id: 'proj-2',
          project_name: 'Layerless Test',
          created_at: '',
          modified_at: '',
        },
        workspace: { bed_width_mm: 400, bed_height_mm: 300, origin: 'top_left' },
        layers: [makeLayer({ id: 'layer-created', name: 'Line', operation: 'line' })],
        objects: [makeProjectObject({ id: 'obj-9', layer_id: 'layer-created' })],
        assets: [],
      }),
    );

    await useArtLibraryStore.getState().insertToProject('library-1', 'item-1');

    expect(mockedService.insertArtLibraryItemToProject).toHaveBeenCalledWith(
      'library-1',
      'item-1',
      undefined,
      undefined,
      undefined,
    );
    expect(useProjectStore.getState().selectedLayerId).toBe('layer-created');
    expect(useProjectStore.getState().selectedObjectIds).toEqual(['obj-9']);
    expect(invalidate).toHaveBeenCalledOnce();
    expect(refresh).toHaveBeenCalledOnce();
  });

  it('removeItem returns false when deletion fails', async () => {
    mockedService.removeArtLibraryItem.mockRejectedValue('boom');

    await expect(useArtLibraryStore.getState().removeItem('library-1', 'item-1')).resolves.toBe(
      false,
    );
  });
});
