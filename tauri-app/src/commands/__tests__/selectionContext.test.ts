import { describe, expect, it } from 'vitest';
import {
  createSelectionContext,
  orderSelectedObjects,
  pickLastSelectedVectorGuide,
} from '../selectionContext';
import type { ProjectObject } from '../../types/project';
import { makeProjectObject, makeTextObjectData } from '../../test-utils/projectFixtures';

function makeObject(id: string, overrides: Partial<ProjectObject> = {}): ProjectObject {
  return makeProjectObject({
    id,
    name: id,
    transform: { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 },
    bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } },
    layer_id: 'layer-1',
    data: { type: 'shape', kind: 'rectangle', width: 10, height: 10, corner_radius: 0 },
    ...overrides,
  });
}

describe('selectionContext helpers', () => {
  it('disables Close Path and Break Apart for a single raster selection', () => {
    const raster = makeObject('img-1', {
      data: { type: 'raster_image', asset_key: 'asset', original_width_px: 100, original_height_px: 100 },
    });

    const ctx = createSelectionContext(['img-1'], [raster], false, []);

    expect(ctx.canClosePath).toBe(false);
    expect(ctx.canBreakApart).toBe(false);
  });

  it('requires raster asset source_path before enabling Refresh Image', () => {
    const raster = makeObject('img-1', {
      data: { type: 'raster_image', asset_key: 'asset-1', original_width_px: 100, original_height_px: 100 },
    });

    const missingSource = createSelectionContext(['img-1'], [raster], false, [], [
      { id: 'asset-1', original_filename: 'image.png', media_type: 'png', byte_size: 10, width_px: 100, height_px: 100, source_path: null },
    ]);
    const withSource = createSelectionContext(['img-1'], [raster], false, [], [
      { id: 'asset-1', original_filename: 'image.png', media_type: 'png', byte_size: 10, width_px: 100, height_px: 100, source_path: '/tmp/image.png' },
    ]);

    expect(missingSource.canRefreshImage).toBe(false);
    expect(withSource.canRefreshImage).toBe(true);
  });

  it('allows Select Contained Shapes only for closed vector-compatible references', () => {
    const openPath = makeObject('open', {
      data: { type: 'vector_path', path_data: 'M0 0 L10 0', closed: false },
    });
    const closedPath = makeObject('closed', {
      data: { type: 'vector_path', path_data: 'M0 0 L10 0 Z', closed: true },
    });

    expect(createSelectionContext(['open'], [openPath], false, []).canSelectContainedShapes).toBe(false);
    expect(createSelectionContext(['closed'], [closedPath], false, []).canSelectContainedShapes).toBe(true);
  });

  it('preserves explicit selection order instead of project order', () => {
    const path1 = makeObject('path-1', {
      data: { type: 'vector_path', path_data: 'M0 0 L10 0', closed: false },
    });
    const text = makeObject('text-1', {
      data: makeTextObjectData({
        content: 'Hello',
        font_family: 'Arial',
        font_size_mm: 5,
      }),
    });
    const path2 = makeObject('path-2', {
      data: { type: 'vector_path', path_data: 'M5 5 L15 5', closed: false },
    });

    const ordered = orderSelectedObjects(['text-1', 'path-1', 'path-2'], [path1, text, path2]);

    expect(ordered.map((obj) => obj.id)).toEqual(['text-1', 'path-1', 'path-2']);
  });

  it('uses the last selected vector as the copy-along-path guide', () => {
    const path1 = makeObject('path-1', {
      data: { type: 'vector_path', path_data: 'M0 0 L10 0', closed: false },
    });
    const text = makeObject('text-1', {
      data: makeTextObjectData({
        content: 'Hello',
        font_family: 'Arial',
        font_size_mm: 5,
      }),
    });
    const path2 = makeObject('path-2', {
      data: { type: 'vector_path', path_data: 'M5 5 L15 5', closed: false },
    });

    const guide = pickLastSelectedVectorGuide(['text-1', 'path-1', 'path-2'], [path1, text, path2]);

    expect(guide?.id).toBe('path-2');
  });
});
