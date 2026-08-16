import { describe, expect, it } from 'vitest';
import { makeLayer, makeProject, makeProjectObject } from '../test-utils/projectFixtures';
import { formatWorkspaceBoundsError } from './previewBoundsError';

describe('formatWorkspaceBoundsError', () => {
  it('maps machine-space Y bounds to the visual edge for a bottom-left origin', () => {
    const result = formatWorkspaceBoundsError({
      details: {
        kind: 'bounds_exceeded',
        workspace_origin: 'bottom_left',
        violation: {
          axis: 'y',
          boundary: 'min',
          amount_mm: 2.5,
        },
      },
    }, makeProject({
      workspace: { bed_width_mm: 400, bed_height_mm: 300, origin: 'bottom_left' },
      objects: [],
    }));

    expect(result?.message).toContain('2.5 mm');
    expect(result?.message).toContain('bottom edge');
    expect(result?.sourceObjectId).toBeNull();
  });

  it('ignores errors without structured bounds details', () => {
    expect(formatWorkspaceBoundsError(new Error('Preview failed'), null)).toBeNull();
  });

  it('formats the overrun in the user display unit', () => {
    const result = formatWorkspaceBoundsError({
      details: {
        kind: 'bounds_exceeded',
        violation: {
          axis: 'x',
          boundary: 'max',
          amount_mm: 25.4,
        },
      },
    }, makeProject(), 'inches');

    expect(result?.message).toContain('1 in');
    expect(result?.message).not.toContain('25.4 mm');
  });

  it('selects the only visible output object when raster details omit a source id', () => {
    const layer = makeLayer({ id: 'image-layer', operation: 'image' });
    const object = makeProjectObject({
      id: 'bitmap-1',
      layer_id: layer.id,
      data: {
        type: 'raster_image',
        asset_key: 'asset-1',
        original_width_px: 100,
        original_height_px: 100,
      },
    });
    const result = formatWorkspaceBoundsError({
      details: {
        kind: 'bounds_exceeded',
        violation: { axis: 'y', boundary: 'min', amount_mm: 127.2 },
      },
    }, makeProject({ layers: [layer], objects: [object] }));

    expect(result?.sourceObjectId).toBe('bitmap-1');
    expect(result?.message).toContain('planned position');
  });
});
