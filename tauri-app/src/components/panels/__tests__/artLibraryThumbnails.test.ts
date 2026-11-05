import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { drawObject } from '../../../canvas/drawObjects';
import { makeLayer, makeProjectObject } from '../../../test-utils/projectFixtures';
import type { ArtLibraryItem, ArtLibrarySelectionSnapshot } from '../../../types/artLibrary';
import { generateArtLibraryThumbnail } from '../artLibraryThumbnails';

vi.mock('../../../canvas/drawObjects', () => ({ drawObject: vi.fn() }));

function makeSnapshotItem(snapshot: Partial<ArtLibrarySelectionSnapshot> = {}): ArtLibraryItem {
  return {
    id: 'selection-1',
    kind: 'selection_snapshot',
    name: 'Rectangle',
    category: 'General',
    tags: [],
    source_filename: '',
    media_type: 'application/vnd.beambench.art-snapshot+json',
    data: btoa(JSON.stringify({
      format_version: '1.0',
      objects: [makeProjectObject()],
      layer_templates: [makeLayer()],
      ...snapshot,
    })),
    created_at: '2026-01-01T00:00:00Z',
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue({
    clearRect: vi.fn(),
  } as unknown as CanvasRenderingContext2D);
  vi.spyOn(HTMLCanvasElement.prototype, 'toDataURL').mockReturnValue('data:image/png;base64,cG5n');
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe('art library thumbnails', () => {
  it.each([undefined, []])('renders vector snapshots with assets=%s', async (assets) => {
    // The backend omits empty arrays from vector-only snapshots, including saved libraries.
    const item = makeSnapshotItem({ assets });
    const originalData = item.data;
    await expect(generateArtLibraryThumbnail(item)).resolves.toBe('cG5n');
    expect(drawObject).toHaveBeenCalledOnce();
    expect(vi.mocked(drawObject).mock.calls[0][4]?.size).toBe(0);
    expect(item.data).toBe(originalData);
  });

  it('loads embedded images into the drawing cache', async () => {
    vi.stubGlobal('URL', {
      createObjectURL: vi.fn().mockReturnValue('blob:thumbnail'),
      revokeObjectURL: vi.fn(),
    });
    class ThumbnailImage {
      onload: (() => void) | null = null;
      set src(_value: string) {
        queueMicrotask(() => this.onload?.());
      }
    }
    vi.stubGlobal('Image', ThumbnailImage);
    const item = makeSnapshotItem({
      objects: [makeProjectObject({ data: {
        type: 'raster_image', asset_key: 'asset-hash', original_width_px: 1, original_height_px: 1,
        masks: [],
      } })],
      assets: [{ hash: 'asset-hash', media_type: 'image/png', data: 'cG5n' }],
    });

    await expect(generateArtLibraryThumbnail(item)).resolves.toBe('cG5n');
    expect(vi.mocked(drawObject).mock.calls[0][4]?.get('asset-hash')).toBeInstanceOf(ThumbnailImage);
    expect(URL.revokeObjectURL).toHaveBeenCalledWith('blob:thumbnail');
  });
});
