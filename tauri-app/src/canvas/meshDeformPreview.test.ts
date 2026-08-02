import { describe, expect, it } from 'vitest';
import { makeProjectObject } from '../test-utils/projectFixtures';
import {
  buildMeshDeformPreviewObjects,
  mapMeshDeformPoint,
} from './meshDeformPreview';

describe('mesh deform live preview', () => {
  it('builds world-space proxy geometry from a transformed vector', () => {
    const object = makeProjectObject({
      id: 'vector',
      layer_id: 'layer1',
      bounds: { min: { x: 10, y: 20 }, max: { x: 30, y: 40 } },
      transform: { a: 1, b: 0, c: 0, d: 1, tx: 5, ty: -2 },
      data: { type: 'vector_path', path_data: 'M 0 0 L 10 0 L 10 10 Z', closed: true },
    });

    const previews = buildMeshDeformPreviewObjects([object], ['vector']);

    expect(previews).toHaveLength(1);
    expect(previews[0].paths[0]).toMatchObject({ closed: true });
    expect(previews[0].paths[0].points[0]).toEqual({ x: 15, y: 18 });
    expect(previews[0].paths[0].points).toContainEqual({ x: 35, y: 18 });
  });

  it('matches the four-corner perspective mapping used by the final warp', () => {
    const mapped = mapMeshDeformPoint(
      { x: 10, y: 0 },
      { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } },
      [
        { x: 0, y: 0 },
        { x: 15, y: 3 },
        { x: 0, y: 10 },
        { x: 10, y: 10 },
      ],
      2,
      true,
    );

    expect(mapped.x).toBeCloseTo(15);
    expect(mapped.y).toBeCloseTo(3);
  });

  it('keeps curved contours smooth in the adaptive proxy', () => {
    const object = makeProjectObject({
      id: 'circle',
      bounds: { min: { x: 0, y: 0 }, max: { x: 100, y: 100 } },
      data: {
        type: 'vector_path',
        path_data: 'M 50 0 C 77.6 0 100 22.4 100 50 C 100 77.6 77.6 100 50 100 C 22.4 100 0 77.6 0 50 C 0 22.4 22.4 0 50 0 Z',
        closed: true,
      },
    });

    const previews = buildMeshDeformPreviewObjects([object], ['circle']);

    expect(previews[0].paths[0].points.length).toBeGreaterThanOrEqual(24);
  });

  it('uses a bounded sample for extremely dense paths', () => {
    const segments = Array.from({ length: 30_000 }, (_, index) => `L ${index + 1} ${index % 100}`).join(' ');
    const object = makeProjectObject({
      id: 'dense',
      bounds: { min: { x: 0, y: 0 }, max: { x: 300, y: 100 } },
      data: { type: 'vector_path', path_data: `M 0 0 ${segments}`, closed: false },
    });

    const previews = buildMeshDeformPreviewObjects([object], ['dense']);
    const pointCount = previews[0].paths.reduce((sum, path) => sum + path.points.length, 0);

    expect(pointCount).toBeLessThanOrEqual(12_002);
    expect(pointCount).toBeGreaterThan(5_000);
  });
});
