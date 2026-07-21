import { describe, it, expect } from 'vitest';
import { getSnapCandidates, findBoundsSnap, snapThresholdWorld } from './snapping';
import type { ProjectObject, Bounds } from '../types/project';
import { makeProjectObject } from '../test-utils/projectFixtures';

function makeObject(id: string, bounds: Bounds, visible = true): ProjectObject {
  return makeProjectObject({
    id,
    name: id,
    visible,
    transform: { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 },
    bounds,
    layer_id: 'layer-1',
    data: { type: 'shape', kind: 'rectangle', width: bounds.max.x - bounds.min.x, height: bounds.max.y - bounds.min.y, corner_radius: 0 },
  });
}

describe('getSnapCandidates', () => {
  it('generates 6 candidates per visible object (3 X + 3 Y)', () => {
    const obj = makeObject('a', { min: { x: 10, y: 20 }, max: { x: 30, y: 40 } });
    const candidates = getSnapCandidates([obj], []);
    expect(candidates).toHaveLength(6);
  });

  it('produces correct X-axis candidates (left, right, center)', () => {
    const obj = makeObject('a', { min: { x: 10, y: 20 }, max: { x: 30, y: 40 } });
    const candidates = getSnapCandidates([obj], []);
    const xCandidates = candidates.filter((c) => c.axis === 'x');
    expect(xCandidates).toHaveLength(3);
    const xValues = xCandidates.map((c) => c.x).sort((a, b) => a - b);
    expect(xValues).toEqual([10, 20, 30]); // min, center, max
  });

  it('produces correct Y-axis candidates (top, bottom, center)', () => {
    const obj = makeObject('a', { min: { x: 10, y: 20 }, max: { x: 30, y: 40 } });
    const candidates = getSnapCandidates([obj], []);
    const yCandidates = candidates.filter((c) => c.axis === 'y');
    expect(yCandidates).toHaveLength(3);
    const yValues = yCandidates.map((c) => c.y).sort((a, b) => a - b);
    expect(yValues).toEqual([20, 30, 40]); // min, center, max
  });

  it('excludes objects by ID', () => {
    const objA = makeObject('a', { min: { x: 10, y: 20 }, max: { x: 30, y: 40 } });
    const objB = makeObject('b', { min: { x: 50, y: 60 }, max: { x: 70, y: 80 } });
    const candidates = getSnapCandidates([objA, objB], ['a']);
    expect(candidates).toHaveLength(6); // only objB
    expect(candidates.every((c) => c.sourceObjectId === 'b')).toBe(true);
  });

  it('excludes hidden objects', () => {
    const obj = makeObject('a', { min: { x: 10, y: 20 }, max: { x: 30, y: 40 } }, false);
    const candidates = getSnapCandidates([obj], []);
    expect(candidates).toHaveLength(0);
  });

  it('returns empty array when all objects excluded', () => {
    const obj = makeObject('a', { min: { x: 10, y: 20 }, max: { x: 30, y: 40 } });
    const candidates = getSnapCandidates([obj], ['a']);
    expect(candidates).toHaveLength(0);
  });

  it('returns empty array for empty objects list', () => {
    const candidates = getSnapCandidates([], []);
    expect(candidates).toHaveLength(0);
  });

  it('handles multiple objects correctly', () => {
    const objs = [
      makeObject('a', { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } }),
      makeObject('b', { min: { x: 20, y: 20 }, max: { x: 30, y: 30 } }),
      makeObject('c', { min: { x: 40, y: 40 }, max: { x: 50, y: 50 } }),
    ];
    const candidates = getSnapCandidates(objs, []);
    expect(candidates).toHaveLength(18); // 3 objects x 6 each
  });

  it('marks edge and center types correctly', () => {
    const obj = makeObject('a', { min: { x: 10, y: 20 }, max: { x: 30, y: 40 } });
    const candidates = getSnapCandidates([obj], []);
    const xCandidates = candidates.filter((c) => c.axis === 'x');
    expect(xCandidates.filter((c) => c.type === 'edge')).toHaveLength(2);
    expect(xCandidates.filter((c) => c.type === 'center')).toHaveLength(1);
    const yCandidates = candidates.filter((c) => c.axis === 'y');
    expect(yCandidates.filter((c) => c.type === 'edge')).toHaveLength(2);
    expect(yCandidates.filter((c) => c.type === 'center')).toHaveLength(1);
  });
});

describe('findBoundsSnap', () => {
  it('snaps X axis when moving bounds left edge is close to candidate', () => {
    // Target object at x=50. Moving object left edge at x=49, within threshold of 2.
    const targetObj = makeObject('target', { min: { x: 50, y: 0 }, max: { x: 70, y: 20 } });
    const candidates = getSnapCandidates([targetObj], []);
    const movingBounds: Bounds = { min: { x: 49, y: 100 }, max: { x: 69, y: 120 } };

    const result = findBoundsSnap(movingBounds, candidates, 2);
    // Left edge should snap from 49 to 50, shifting center by +1
    expect(result.snappedX).toBeCloseTo(60); // original center 59 + 1
    expect(result.guides).toContainEqual({ axis: 'x', value: 50 });
  });

  it('snaps Y axis when moving bounds top edge is close to candidate', () => {
    const targetObj = makeObject('target', { min: { x: 0, y: 50 }, max: { x: 20, y: 70 } });
    const candidates = getSnapCandidates([targetObj], []);
    const movingBounds: Bounds = { min: { x: 100, y: 49 }, max: { x: 120, y: 69 } };

    const result = findBoundsSnap(movingBounds, candidates, 2);
    // Top edge should snap from 49 to 50, shifting center by +1
    expect(result.snappedY).toBeCloseTo(60); // original center 59 + 1
    expect(result.guides).toContainEqual({ axis: 'y', value: 50 });
  });

  it('snaps both axes independently', () => {
    const targetObj = makeObject('target', { min: { x: 50, y: 50 }, max: { x: 70, y: 70 } });
    const candidates = getSnapCandidates([targetObj], []);
    // Moving object: left edge near 50, top edge near 50
    const movingBounds: Bounds = { min: { x: 49, y: 51 }, max: { x: 69, y: 71 } };

    const result = findBoundsSnap(movingBounds, candidates, 2);
    expect(result.guides).toHaveLength(2);
  });

  it('returns no guides when nothing is within threshold', () => {
    const targetObj = makeObject('target', { min: { x: 50, y: 50 }, max: { x: 70, y: 70 } });
    const candidates = getSnapCandidates([targetObj], []);
    // Moving object far from any candidate
    const movingBounds: Bounds = { min: { x: 100, y: 100 }, max: { x: 120, y: 120 } };

    const result = findBoundsSnap(movingBounds, candidates, 2);
    expect(result.guides).toHaveLength(0);
    // Center should remain unchanged
    expect(result.snappedX).toBeCloseTo(110);
    expect(result.snappedY).toBeCloseTo(110);
  });

  it('chooses the closest candidate when multiple are within threshold', () => {
    const objs = [
      makeObject('a', { min: { x: 48, y: 0 }, max: { x: 58, y: 10 } }), // right edge = 58
      makeObject('b', { min: { x: 50, y: 0 }, max: { x: 60, y: 10 } }), // left edge = 50
    ];
    const candidates = getSnapCandidates(objs, []);
    // Moving object left edge at 49.5 — closest x candidate is 50 (dist 0.5) vs 48 (dist 1.5)
    const movingBounds: Bounds = { min: { x: 49.5, y: 100 }, max: { x: 59.5, y: 110 } };

    const result = findBoundsSnap(movingBounds, candidates, 3);
    expect(result.guides).toContainEqual({ axis: 'x', value: 50 });
  });

  it('snaps center to center of another object', () => {
    const targetObj = makeObject('target', { min: { x: 50, y: 50 }, max: { x: 70, y: 70 } });
    // target center is (60, 60)
    const candidates = getSnapCandidates([targetObj], []);
    // Moving object center at (60.5, 60.5)
    const movingBounds: Bounds = { min: { x: 50.5, y: 50.5 }, max: { x: 70.5, y: 70.5 } };

    const result = findBoundsSnap(movingBounds, candidates, 2);
    expect(result.snappedX).toBeCloseTo(60);
    expect(result.snappedY).toBeCloseTo(60);
  });

  it('handles empty candidates array', () => {
    const movingBounds: Bounds = { min: { x: 10, y: 20 }, max: { x: 30, y: 40 } };
    const result = findBoundsSnap(movingBounds, [], 2);
    expect(result.snappedX).toBeCloseTo(20); // center
    expect(result.snappedY).toBeCloseTo(30); // center
    expect(result.guides).toHaveLength(0);
  });
});

describe('snapThresholdWorld', () => {
  it('returns correct threshold at 100% zoom', () => {
    // At 100% zoom: pxPerMm = 2.0, so 5px = 2.5mm
    expect(snapThresholdWorld(5, 100)).toBeCloseTo(2.5);
  });

  it('returns larger threshold at lower zoom', () => {
    // At 50% zoom: pxPerMm = 1.0, so 5px = 5mm
    expect(snapThresholdWorld(5, 50)).toBeCloseTo(5);
  });

  it('returns smaller threshold at higher zoom', () => {
    // At 200% zoom: pxPerMm = 4.0, so 5px = 1.25mm
    expect(snapThresholdWorld(5, 200)).toBeCloseTo(1.25);
  });
});
