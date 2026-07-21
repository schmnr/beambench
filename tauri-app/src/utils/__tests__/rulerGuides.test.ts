import { describe, expect, it } from 'vitest';
import {
  buildRulerGuideGeometry,
  normalizeProjectRulerGuides,
  normalizeRulerGuideObject,
} from '../rulerGuides';
import { makeProject, makeProjectObject } from '../../test-utils/projectFixtures';

describe('rulerGuides', () => {
  it('re-stretches a vertical guide to the current workspace on load', () => {
    const guide = makeProjectObject({
      id: 'guide',
      bounds: { min: { x: 120, y: 0 }, max: { x: 120, y: 300 } },
      data: {
        type: 'vector_path',
        path_data: 'M 120 0 L 120 300',
        closed: false,
        ruler_guide_axis: 'vertical',
      },
    });
    const project = makeProject({
      workspace: { bed_width_mm: 500, bed_height_mm: 400, origin: 'top_left' },
      objects: [guide],
    });

    const normalized = normalizeProjectRulerGuides(project);
    expect(normalized.objects[0].bounds).toEqual({
      min: { x: 120, y: 0 },
      max: { x: 120, y: 400 },
    });
    expect(normalized.objects[0].data.type).toBe('vector_path');
    if (normalized.objects[0].data.type === 'vector_path') {
      expect(normalized.objects[0].data.path_data).toBe('M 120 0 L 120 400');
    }
  });

  it('clamps out-of-bounds guide positions to the workspace edge', () => {
    const guide = makeProjectObject({
      id: 'guide',
      bounds: { min: { x: 500, y: 0 }, max: { x: 500, y: 300 } },
      data: {
        type: 'vector_path',
        path_data: 'M 500 0 L 500 300',
        closed: false,
        ruler_guide_axis: 'vertical',
      },
    });

    const normalized = normalizeRulerGuideObject(guide, {
      bed_width_mm: 400,
      bed_height_mm: 300,
      origin: 'top_left',
    });
    expect(normalized.bounds).toEqual({
      min: { x: 400, y: 0 },
      max: { x: 400, y: 300 },
    });
    expect(normalized.data.type).toBe('vector_path');
    if (normalized.data.type === 'vector_path') {
      expect(normalized.data.path_data).toBe('M 400 0 L 400 300');
    }
  });

  it('leaves non-guide objects unchanged during normalization', () => {
    const obj = makeProjectObject({
      id: 'obj',
      data: {
        type: 'vector_path',
        path_data: 'M 0 0 L 10 10',
        closed: false,
      },
    });

    expect(normalizeRulerGuideObject(obj, {
      bed_width_mm: 400,
      bed_height_mm: 300,
      origin: 'top_left',
    })).toEqual(obj);
  });

  it('builds full-span horizontal and vertical guide geometry', () => {
    expect(buildRulerGuideGeometry('vertical', 42, {
      bed_width_mm: 400,
      bed_height_mm: 300,
      origin: 'top_left',
    })).toEqual({
      path_data: 'M 42 0 L 42 300',
      bounds: {
        min: { x: 42, y: 0 },
        max: { x: 42, y: 300 },
      },
    });

    expect(buildRulerGuideGeometry('horizontal', 17, {
      bed_width_mm: 400,
      bed_height_mm: 300,
      origin: 'top_left',
    })).toEqual({
      path_data: 'M 0 17 L 400 17',
      bounds: {
        min: { x: 0, y: 17 },
        max: { x: 400, y: 17 },
      },
    });
  });
});
