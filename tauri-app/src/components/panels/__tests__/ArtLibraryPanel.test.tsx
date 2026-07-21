import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor, act } from '@testing-library/react';
import { open, save } from '@tauri-apps/plugin-dialog';

import { ArtLibraryPanel } from '../ArtLibraryPanel';
import { useArtLibraryStore } from '../../../stores/artLibraryStore';
import { useProjectStore } from '../../../stores/projectStore';
import type { ArtLibraryItem, LoadedArtLibrary } from '../../../types/artLibrary';
import { makeLayer, makeProject } from '../../../test-utils/projectFixtures';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockReturnValue(new Promise(() => {})),
}));
vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn(),
  save: vi.fn(),
}));

vi.mock('../../../services/artLibraryService', () => ({
  artLibraryService: {
    getArtLibraries: vi.fn().mockResolvedValue({ libraries: [], warnings: [] }),
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

function makeItem(overrides: Partial<ArtLibraryItem> = {}): ArtLibraryItem {
  return {
    id: 'item-1',
    kind: 'external_file',
    name: 'Test -Kerf width card',
    category: 'General',
    tags: ['kerf'],
    source_filename: 'test-kerf.svg',
    media_type: 'image/svg+xml',
    data: 'abc',
    created_at: '2026-01-01T00:00:00Z',
    ...overrides,
  };
}

function makeSnapshotItem(overrides: Partial<ArtLibraryItem> = {}): ArtLibraryItem {
  return makeItem({
    id: 'snapshot-1',
    kind: 'selection_snapshot',
    name: 'General Text',
    media_type: 'application/vnd.beambench.art-snapshot+json',
    data: btoa(JSON.stringify({
      format_version: 'bbart/snapshot-v1',
      objects: [
        {
          id: 'obj-1',
          layer_id: 'layer-1',
          z_index: 0,
          name: 'Text',
          locked: false,
          hidden: false,
          transform: { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 },
          bounds: { min: { x: 0, y: 0 }, max: { x: 25.4, y: 25.4 } },
          data: { type: 'shape', kind: 'rectangle', width: 25.4, height: 25.4, corner_radius: 0 },
        },
      ],
      layer_templates: [],
      assets: [],
      source_text_metadata: [],
    })),
    ...overrides,
  });
}

function makeLibrary(overrides: Partial<LoadedArtLibrary> = {}): LoadedArtLibrary {
  return {
    format_version: 'bbart/v1',
    library_id: 'library-1',
    name: 'Projects and Art',
    items: [makeItem()],
    path: '/tmp/Projects and Art.bbart',
    ...overrides,
  };
}

beforeEach(() => {
  vi.mocked(open).mockReset();
  vi.mocked(save).mockReset();
  const storage = new Map<string, string>();
  Object.defineProperty(window, 'localStorage', {
    configurable: true,
    value: {
      getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => {
        storage.set(key, String(value));
      },
      removeItem: (key: string) => {
        storage.delete(key);
      },
      clear: () => {
        storage.clear();
      },
    },
  });
  useArtLibraryStore.setState((state) => ({
    ...state,
    libraries: [],
    selectedLibraryId: null,
    searchQuery: '',
    selectedCategory: null,
    dragState: null,
    loadLibraries: vi.fn().mockResolvedValue(undefined),
  }));
  useProjectStore.setState({
    project: null,
    selectedLayerId: null,
    selectedObjectIds: [],
  });
});

async function renderPanel(
  storeOverrides?: Partial<ReturnType<typeof useArtLibraryStore.getState>>,
) {
  if (storeOverrides) {
    act(() => {
      useArtLibraryStore.setState((state) => ({ ...state, ...storeOverrides }));
    });
  }
  await act(async () => {
    render(<ArtLibraryPanel />);
  });
}

describe('ArtLibraryPanel', () => {
  it('renders search, icon-size, and grouped-section controls', async () => {
    const library = makeLibrary({
      items: [makeSnapshotItem()],
    });
    await renderPanel({
      libraries: [library],
      selectedLibraryId: library.library_id,
    });

    expect(screen.getByTestId('art-library-list')).toBeDefined();
    expect(screen.getByPlaceholderText('Search...')).toBeDefined();
    expect(screen.getByTestId('art-library-icon-size')).toBeDefined();
    expect(screen.getByTestId('art-library-icon-size-readout').textContent).toBe('128 x 128');
    expect(screen.getByText('Art Library')).toBeDefined();
    expect(screen.getByText('Graphic')).toBeDefined();
    expect(screen.queryByDisplayValue('All')).toBeNull();
  });

  it('renders the required bottom button clusters and icons', async () => {
    const library = makeLibrary();
    await renderPanel({
      libraries: [library],
      selectedLibraryId: library.library_id,
    });

    expect(screen.getByTestId('art-library-new').querySelector('svg')).not.toBeNull();
    expect(screen.getByTestId('art-library-import').querySelector('svg')).not.toBeNull();
    expect(screen.getByTestId('art-library-delete').querySelector('svg')).not.toBeNull();
    expect(screen.getByTestId('art-library-unload')).toBeDefined();
    expect(screen.getByTestId('art-library-add-to-project')).toBeDefined();
    expect(screen.getByTestId('art-library-import-from-project')).toBeDefined();
  });

  it('shows strong disabled states when no item is selected', async () => {
    const library = makeLibrary();
    await renderPanel({
      libraries: [library],
      selectedLibraryId: library.library_id,
    });

    expect((screen.getByTestId('art-library-add-to-project') as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByTestId('art-library-delete') as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByTestId('art-library-unload') as HTMLButtonElement).disabled).toBe(false);
  });

  it('disables Add Graphic to Project when no project is open', async () => {
    const library = makeLibrary({
      items: [makeSnapshotItem()],
    });
    await renderPanel({
      libraries: [library],
      selectedLibraryId: library.library_id,
    });

    fireEvent.click(screen.getByTestId('art-item-snapshot-1'));

    expect((screen.getByTestId('art-library-add-to-project') as HTMLButtonElement).disabled).toBe(true);

    fireEvent.contextMenu(screen.getByTestId('art-item-snapshot-1'));
    await waitFor(() => {
      expect(screen.getByTestId('context-menu-item-art-insert')).toBeDefined();
    });
    expect((screen.getByTestId('context-menu-item-art-insert') as HTMLButtonElement).disabled).toBe(true);
  });

  it('disables unload when the active library has a save error and shows the error banner', async () => {
    const library = makeLibrary({ save_error: 'Permission denied' });
    await renderPanel({
      libraries: [library],
      selectedLibraryId: library.library_id,
    });

    expect(screen.getByText('Save failed: Permission denied')).toBeDefined();
    expect((screen.getByTestId('art-library-unload') as HTMLButtonElement).disabled).toBe(true);
  });

  it('opens Save As immediately when creating a new library', async () => {
    const createLibrary = vi.fn().mockResolvedValue(null);
    vi.mocked(save).mockResolvedValue('/tmp/New Shapes.bbart');
    await renderPanel({ createLibrary } as never);

    fireEvent.click(screen.getByTestId('art-library-new'));

    await waitFor(() => {
      expect(save).toHaveBeenCalledWith({
        title: 'New Art Library',
        defaultPath: 'Untitled.bbart',
        filters: [{ name: 'Beam Bench Art Library', extensions: ['bbart'] }],
      });
    });
    expect(createLibrary).toHaveBeenCalledWith('/tmp/New Shapes.bbart', 'New Shapes');
  });

  it('loads a library through the Load button', async () => {
    const loadLibrary = vi.fn().mockResolvedValue(null);
    vi.mocked(open).mockResolvedValue('/tmp/Loaded.bbart');
    await renderPanel({ loadLibrary } as never);

    fireEvent.click(screen.getByTestId('art-library-load'));

    await waitFor(() => {
      expect(open).toHaveBeenCalledWith({
        title: 'Load Art Library',
        multiple: false,
        filters: [{ name: 'Beam Bench Art Library', extensions: ['bbart'] }],
      });
    });
    expect(loadLibrary).toHaveBeenCalledWith('/tmp/Loaded.bbart');
  });

  it('imports external artwork from the bottom action bar', async () => {
    const library = makeLibrary({ items: [] });
    const addFileItems = vi.fn().mockResolvedValue(undefined);
    vi.mocked(open).mockResolvedValue(['/tmp/library-art.tga', '/tmp/library-vector.svg']);
    await renderPanel({
      libraries: [library],
      selectedLibraryId: library.library_id,
      addFileItems,
    } as never);

    fireEvent.click(screen.getByTestId('art-library-import'));

    await waitFor(() => {
      expect(open).toHaveBeenCalledWith({
        title: 'Add Art Library Items',
        multiple: true,
        filters: [
          {
            name: 'Artwork',
            extensions: ['svg', 'png', 'jpg', 'jpeg', 'gif', 'bmp', 'webp', 'tif', 'tiff', 'tga', 'dxf', 'pdf', 'ai', 'eps'],
          },
        ],
      });
    });
    expect(addFileItems).toHaveBeenCalledWith(
      'library-1',
      [
        { filePath: '/tmp/library-art.tga', name: 'library-art' },
        { filePath: '/tmp/library-vector.svg', name: 'library-vector' },
      ],
      'General',
      [],
    );
  });

  it('captures project selection from the bottom action bar', async () => {
    const library = makeLibrary();
    const addSelectionItem = vi.fn().mockResolvedValue(undefined);
    await renderPanel({
      libraries: [library],
      selectedLibraryId: library.library_id,
      addSelectionItem,
    } as never);

    fireEvent.click(screen.getByTestId('art-library-import-from-project'));

    await waitFor(() => {
      expect(addSelectionItem).toHaveBeenCalledWith('library-1', 'Selection', 'General', []);
    });
  });

  it('selects an item, updates the footer, and enables Add Graphic to Project', async () => {
    const library = makeLibrary({
      items: [makeSnapshotItem()],
    });
    useProjectStore.setState({
      project: makeProject({
        layers: [makeLayer({ id: 'layer-1', name: 'Line', operation: 'line' })],
        objects: [],
        assets: [],
      }),
      selectedLayerId: 'layer-1',
      selectedObjectIds: [],
    });
    const insertToProject = vi.fn().mockResolvedValue(undefined);
    await renderPanel({
      libraries: [library],
      selectedLibraryId: library.library_id,
      insertToProject,
    } as never);

    fireEvent.click(screen.getByTestId('art-item-snapshot-1'));

    expect(screen.getAllByText('General Text').length).toBeGreaterThan(1);
    expect(screen.getByText('25.4 mm x 25.4 mm')).toBeDefined();
    expect(screen.getAllByText('Graphic').length).toBeGreaterThan(0);
    expect((screen.getByTestId('art-library-add-to-project') as HTMLButtonElement).disabled).toBe(false);

    fireEvent.click(screen.getByTestId('art-library-add-to-project'));
    await waitFor(() => {
      expect(insertToProject).toHaveBeenCalledWith('library-1', 'snapshot-1');
    });
  });

  it('persists icon size locally and restores the readout on remount', async () => {
    const library = makeLibrary();
    const { unmount } = render(<ArtLibraryPanel />);
    act(() => {
      useArtLibraryStore.setState((state) => ({
        ...state,
        libraries: [library],
        selectedLibraryId: library.library_id,
      }));
    });

    fireEvent.change(screen.getByTestId('art-library-icon-size'), { target: { value: '144' } });
    expect(screen.getByTestId('art-library-icon-size-readout').textContent).toBe('144 x 144');

    unmount();

    await renderPanel({
      libraries: [library],
      selectedLibraryId: library.library_id,
    });
    expect(screen.getByTestId('art-library-icon-size-readout').textContent).toBe('144 x 144');
  });

  it('shows item context actions including Add Selection to Library, Rename, and Delete', async () => {
    const library = makeLibrary();
    await renderPanel({
      libraries: [library],
      selectedLibraryId: library.library_id,
    });

    fireEvent.contextMenu(screen.getByText('Test -Kerf width card'));

    await waitFor(() => {
      expect(screen.getByTestId('context-menu')).toBeDefined();
    });
    expect(screen.getByTestId('context-menu-item-art-insert')).toBeDefined();
    expect(screen.getByTestId('context-menu-item-art-add-selection')).toBeDefined();
    expect(screen.getByTestId('context-menu-item-art-rename')).toBeDefined();
    expect(screen.getByTestId('context-menu-item-art-delete')).toBeDefined();
  });

  it('uses inline dialogs for rename and delete instead of browser globals', async () => {
    const library = makeLibrary();
    await renderPanel({
      libraries: [library],
      selectedLibraryId: library.library_id,
    });

    fireEvent.contextMenu(screen.getByText('Test -Kerf width card'));
    await waitFor(() => {
      expect(screen.getByTestId('context-menu-item-art-rename')).toBeDefined();
    });
    fireEvent.click(screen.getByTestId('context-menu-item-art-rename'));

    expect(screen.getByRole('dialog', { name: 'Rename Artwork Item' })).toBeDefined();
    expect((screen.getByTestId('art-library-rename-input') as HTMLInputElement).value).toBe('Test -Kerf width card');

    fireEvent.click(screen.getByText('Cancel'));
    fireEvent.click(screen.getByTestId('art-item-item-1'));
    fireEvent.click(screen.getByTestId('art-library-delete'));
    expect(screen.getByRole('dialog', { name: 'Delete Artwork Item' })).toBeDefined();
  });

  it('keeps the empty state inside the browser area', async () => {
    const library = makeLibrary({ items: [] });
    await renderPanel({
      libraries: [library],
      selectedLibraryId: library.library_id,
    });

    expect(screen.getByText('No items in this library.')).toBeDefined();
    expect(screen.queryByText('Add File...')).toBeNull();
  });
});
