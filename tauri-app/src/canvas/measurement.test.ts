import { describe, expect, it } from 'vitest';
import type { ViewportParams } from './ViewportTransform';
import { worldToScreen } from './ViewportTransform';
import {
  buildAngleMeasurement,
  buildGapMeasurement,
  buildObjectMeasurementMetrics,
  buildRadiusMeasurement,
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

  it('measures the included angle between segments', () => {
    const result = buildAngleMeasurement(
      { start: { x: 0, y: 0 }, end: { x: 10, y: 0 }, dxMm: 10, dyMm: 0, lengthMm: 10, angleDeg: 0, segmentIndex: 0, t: 0.5 },
      { start: { x: 0, y: 0 }, end: { x: 0, y: 10 }, dxMm: 0, dyMm: 10, lengthMm: 10, angleDeg: 90, segmentIndex: 1, t: 0.5 },
    );

    expect(result.angleDeg).toBeCloseTo(90);
    expect(result.labelPoint).toEqual({ x: 2.5, y: 2.5 });
  });

  it('measures transformed ellipse radii and diameter', () => {
    const ellipse = makeProjectObject({
      id: 'ellipse',
      name: 'Ellipse',
      data: { type: 'shape', kind: 'ellipse', width: 20, height: 10, corner_radius: 0 },
      bounds: { min: { x: 0, y: 0 }, max: { x: 20, y: 10 } },
      transform: { a: 2, b: 0, c: 0, d: 3, tx: 4, ty: 5 },
    });

    const result = buildRadiusMeasurement(ellipse);

    expect(result?.center).toEqual({ x: 14, y: 10 });
    expect(result?.radiusXmm).toBeCloseTo(20);
    expect(result?.radiusYmm).toBeCloseTo(15);
    expect(result?.diameterXmm).toBeCloseTo(40);
    expect(result?.circular).toBe(false);
  });

  it('returns the closest edge gap and axis clearances between objects', () => {
    const first = makeProjectObject({
      id: 'first',
      name: 'First',
      bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } },
    });
    const second = makeProjectObject({
      id: 'second',
      name: 'Second',
      bounds: { min: { x: 20, y: 5 }, max: { x: 30, y: 15 } },
    });

    const result = buildGapMeasurement(first, second, [first, second]);

    expect(result?.lengthMm).toBeCloseTo(10);
    expect(result?.horizontalGapMm).toBeCloseTo(10);
    expect(result?.verticalGapMm).toBeCloseTo(0);
  });
});
