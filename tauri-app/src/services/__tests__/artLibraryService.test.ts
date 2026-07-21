import { describe, it, expect, vi, beforeEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

import { artLibraryService } from '../artLibraryService';

describe('artLibraryService', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it('normalizes the current load-state payload shape', async () => {
    vi.mocked(invoke).mockResolvedValue({
      libraries: [
        {
          format_version: '1.0',
          library_id: 'library-1',
          name: 'Shapes',
          items: [],
          path: '/tmp/Shapes.bbart',
        },
      ],
      warnings: ['migrated'],
    });

    await expect(artLibraryService.getArtLibraries()).resolves.toEqual({
      libraries: [
        {
          format_version: '1.0',
          library_id: 'library-1',
          name: 'Shapes',
          items: [],
          path: '/tmp/Shapes.bbart',
        },
      ],
      warnings: ['migrated'],
    });
  });

  it('accepts the legacy bare-array payload shape', async () => {
    vi.mocked(invoke).mockResolvedValue([
      {
        format_version: '1.0',
        library_id: 'library-1',
        name: 'Shapes',
        items: [],
        path: '/tmp/Shapes.bbart',
      },
    ]);

    await expect(artLibraryService.getArtLibraries()).resolves.toEqual({
      libraries: [
        {
          format_version: '1.0',
          library_id: 'library-1',
          name: 'Shapes',
          items: [],
          path: '/tmp/Shapes.bbart',
        },
      ],
      warnings: [],
    });
  });

  it('falls back to an empty state when the payload is missing', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);

    await expect(artLibraryService.getArtLibraries()).resolves.toEqual({
      libraries: [],
      warnings: [],
    });
  });

  it('omits layerId when inserting an art library item without an active layer', async () => {
    vi.mocked(invoke).mockResolvedValue([]);

    await artLibraryService.insertArtLibraryItemToProject('library-1', 'item-1');

    expect(invoke).toHaveBeenCalledWith('insert_art_library_item_to_project', {
      libraryId: 'library-1',
      itemId: 'item-1',
      dropX: undefined,
      dropY: undefined,
    });
  });
});
