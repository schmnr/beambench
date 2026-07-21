import { describe, expect, it } from 'vitest';
import type { ViewportParams } from './ViewportTransform';
import { worldToScreen } from './ViewportTransform';
import {
  buildObjectMeasurementMetrics,
  nearestSegmentToScreenPoint,
  visibleMeasurementObjects,
} from './measurement';
import { makeLayer, makeProjectObject } from '../test-utils/projectFixtures';

const vp: ViewportParams = {
  offset: { x: 0, y: 0 },
  zoom: 100,
  canvasWidth: 100,
  canvasHeight: 100,
};

describe('measurement helpers', () => {
  it('computes closed rectangle metrics', () => {
    const object = makeProjectObject({
      bounds: { min: { x: 0, y: 0 }, max: { x: 20, y: 10 } },
    });

    const metrics = buildObjectMeasurementMetrics(object, [object]);

    expect(metrics.widthMm).toBeCloseTo(20);
    expect(metrics.heightMm).toBeCloseTo(10);
    expect(metrics.center).toEqual({ x: 10, y: 5 });
    expect(metrics.perimeterMm).toBeCloseTo(60);
    expect(metrics.areaMm2).toBeCloseTo(200);
    expect(metrics.closed).toBe(true);
  });

  it('leaves area blank for open vector paths', () => {
    const object = makeProjectObject({
      data: { type: 'vector_path', path_data: 'M 0 0 L 10 0 L 10 10', closed: false },
      bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } },
    });

    const metrics = buildObjectMeasurementMetrics(object, [object]);

    expect(metrics.closed).toBe(false);
    expect(metrics.areaMm2).toBeNull();
    expect(metrics.lines).toBe(2);
  });

  it('computes approximate area for closed vector paths', () => {
    const object = makeProjectObject({
      data: { type: 'vector_path', path_data: 'M 0 0 L 10 0 L 10 10 Z', closed: true },
      bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } },
    });

    const metrics = buildObjectMeasurementMetrics(object, [object]);

    expect(metrics.closed).toBe(true);
    expect(metrics.areaMm2).toBeCloseTo(50);
  });

  it('finds the nearest screen-space segment', () => {
    const object = makeProjectObject({
      bounds: { min: { x: 0, y: 0 }, max: { x: 20, y: 10 } },
    });

    const segment = nearestSegmentToScreenPoint(
      object,
      worldToScreen({ x: 10, y: 0 }, vp),
      vp,
      [object],
    );

    expect(segment?.lengthMm).toBeCloseTo(20);
    expect(segment?.start.y).toBeCloseTo(0);
    expect(segment?.end.y).toBeCloseTo(0);
  });

  it('filters hidden objects and hidden layers', () => {
    const visible = makeProjectObject({ id: 'visible' });
    const hiddenObject = makeProjectObject({ id: 'hidden-object', visible: false });
    const hiddenLayerObject = makeProjectObject({ id: 'hidden-layer-object', layer_id: 'hidden-layer' });

    const result = visibleMeasurementObjects(
      [visible, hiddenObject, hiddenLayerObject],
      [
        makeLayer({ id: 'layer-1', visible: true }),
        makeLayer({ id: 'hidden-layer', visible: false }),
      ],
    );

    expect(result.map((object) => object.id)).toEqual(['visible']);
  });
});
