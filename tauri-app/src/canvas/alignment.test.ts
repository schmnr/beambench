import { beforeEach, describe, it, expect } from 'vitest';
import {
  computeObjectSnap,
  alignObjects,
  distributeObjects,
  getCombinedBounds,
  applyAroundCenter,
  computeTransformedBoundsWorld,
  mapLocalToBounds,
  computeVisualBoundsWorld,
  computeSelectionPivot,
  getObjectSnapPoints,
  getObjectSnapSegments,
  computePointSnap,
  computeSelectionSnap,
  getRulerGuideAxis,
  isRulerGuideObject,
  resetAlignmentCachesForTests,
  resolveGeometrySnap,
} from './alignment';
import type { Bounds, Point2D, Transform2D, ProjectObject } from '../types/project';
import { makeProjectObject } from '../test-utils/projectFixtures';

// Helper to make bounds from (x, y, w, h)
function mkBounds(x: number, y: number, w: number, h: number): Bounds {
  return { min: { x, y }, max: { x: x + w, y: y + h } };
}

// Helper to make an object-like with id and bounds
function mkObj(id: string, x: number, y: number, w: number, h: number) {
  return { id, bounds: mkBounds(x, y, w, h) };
}

beforeEach(() => {
  resetAlignmentCachesForTests();
});

// Helper to make a minimal ProjectObject for snap tests (identity transform, rectangle)
function mkSnapObj(x: number, y: number, w: number, h: number): ProjectObject {
  return makeProjectObject({
    id: `snap-${x}-${y}`,
    name: 'Snap',
    transform: { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 },
    bounds: mkBounds(x, y, w, h),
    layer_id: 'layer-1',
    data: { type: 'shape', kind: 'rectangle', width: w, height: h, corner_radius: 0 },
  });
}

function mkVectorPathObj(
  id: string,
  path_data: string,
  bounds: Bounds,
  overrides?: Partial<ProjectObject>,
): ProjectObject {
  return makeProjectObject({
    id,
    name: id,
    transform: { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 },
    bounds,
    layer_id: 'layer-1',
    data: { type: 'vector_path', path_data, closed: false },
    ...overrides,
  });
}

// --- getCombinedBounds ---

describe('getCombinedBounds', () => {
  it('returns zero bounds for empty array', () => {
    const result = getCombinedBounds([]);
    expect(result).toEqual({ min: { x: 0, y: 0 }, max: { x: 0, y: 0 } });
  });

  it('returns the same bounds for a single input', () => {
    const b = mkBounds(10, 20, 30, 40);
    expect(getCombinedBounds([b])).toEqual(b);
  });

  it('combines two non-overlapping bounds', () => {
    const result = getCombinedBounds([mkBounds(0, 0, 10, 10), mkBounds(20, 30, 5, 5)]);
    expect(result).toEqual({ min: { x: 0, y: 0 }, max: { x: 25, y: 35 } });
  });

  it('combines overlapping bounds', () => {
    const result = getCombinedBounds([mkBounds(0, 0, 20, 20), mkBounds(10, 10, 20, 20)]);
    expect(result).toEqual({ min: { x: 0, y: 0 }, max: { x: 30, y: 30 } });
  });
});

// --- alignObjects ---

describe('alignObjects', () => {
  it('returns empty map for single object', () => {
    const result = alignObjects([mkObj('a', 10, 20, 30, 40)], 'left');
    expect(result.size).toBe(0);
  });

  it('aligns left — all min.x to smallest min.x', () => {
    const objs = [mkObj('a', 10, 0, 20, 10), mkObj('b', 50, 0, 30, 10)];
    const result = alignObjects(objs, 'left');
    expect(result.get('a')!.min.x).toBe(10);
    expect(result.get('b')!.min.x).toBe(10);
    // Width preserved
    expect(result.get('b')!.max.x - result.get('b')!.min.x).toBe(30);
  });

  it('aligns right — all max.x to largest max.x', () => {
    const objs = [mkObj('a', 10, 0, 20, 10), mkObj('b', 50, 0, 30, 10)];
    const result = alignObjects(objs, 'right');
    // Combined max.x = 80
    expect(result.get('a')!.max.x).toBe(80);
    expect(result.get('b')!.max.x).toBe(80);
    // Width preserved
    expect(result.get('a')!.max.x - result.get('a')!.min.x).toBe(20);
  });

  it('aligns top — all min.y to smallest min.y', () => {
    const objs = [mkObj('a', 0, 10, 10, 20), mkObj('b', 0, 50, 10, 30)];
    const result = alignObjects(objs, 'top');
    expect(result.get('a')!.min.y).toBe(10);
    expect(result.get('b')!.min.y).toBe(10);
  });

  it('aligns bottom — all max.y to largest max.y', () => {
    const objs = [mkObj('a', 0, 10, 10, 20), mkObj('b', 0, 50, 10, 30)];
    const result = alignObjects(objs, 'bottom');
    // Combined max.y = 80
    expect(result.get('a')!.max.y).toBe(80);
    expect(result.get('b')!.max.y).toBe(80);
  });

  it('aligns center-h — all center X to combined center X', () => {
    const objs = [mkObj('a', 0, 0, 20, 10), mkObj('b', 60, 0, 40, 10)];
    // Combined: min.x=0, max.x=100, center=50
    const result = alignObjects(objs, 'center-h');
    // a: center should be 50, width=20, so min.x=40
    expect(result.get('a')!.min.x).toBe(40);
    expect(result.get('a')!.max.x).toBe(60);
    // b: center should be 50, width=40, so min.x=30
    expect(result.get('b')!.min.x).toBe(30);
    expect(result.get('b')!.max.x).toBe(70);
  });

  it('aligns center-v — all center Y to combined center Y', () => {
    const objs = [mkObj('a', 0, 0, 10, 20), mkObj('b', 0, 60, 10, 40)];
    // Combined: min.y=0, max.y=100, center=50
    const result = alignObjects(objs, 'center-v');
    // a: center should be 50, height=20, so min.y=40
    expect(result.get('a')!.min.y).toBe(40);
    expect(result.get('a')!.max.y).toBe(60);
    // b: center should be 50, height=40, so min.y=30
    expect(result.get('b')!.min.y).toBe(30);
    expect(result.get('b')!.max.y).toBe(70);
  });

  it('works with 3+ objects', () => {
    const objs = [mkObj('a', 0, 0, 10, 10), mkObj('b', 20, 0, 10, 10), mkObj('c', 50, 0, 10, 10)];
    const result = alignObjects(objs, 'left');
    expect(result.get('a')!.min.x).toBe(0);
    expect(result.get('b')!.min.x).toBe(0);
    expect(result.get('c')!.min.x).toBe(0);
  });

  it('preserves object dimensions', () => {
    const objs = [mkObj('a', 0, 0, 15, 25), mkObj('b', 30, 40, 35, 45)];
    const result = alignObjects(objs, 'center-h');
    const aB = result.get('a')!;
    const bB = result.get('b')!;
    expect(aB.max.x - aB.min.x).toBeCloseTo(15);
    expect(aB.max.y - aB.min.y).toBeCloseTo(25);
    expect(bB.max.x - bB.min.x).toBeCloseTo(35);
    expect(bB.max.y - bB.min.y).toBeCloseTo(45);
  });
});

// --- distributeObjects ---

describe('distributeObjects', () => {
  it('returns empty map for fewer than 3 objects', () => {
    expect(distributeObjects([], 'horizontal').size).toBe(0);
    expect(distributeObjects([mkObj('a', 0, 0, 10, 10)], 'horizontal').size).toBe(0);
    expect(distributeObjects([mkObj('a', 0, 0, 10, 10), mkObj('b', 20, 0, 10, 10)], 'horizontal').size).toBe(0);
  });

  it('distributes 3 objects horizontally', () => {
    // a at x=0 (center=5), b at x=10 (center=15), c at x=40 (center=45)
    const objs = [mkObj('a', 0, 0, 10, 10), mkObj('b', 10, 0, 10, 10), mkObj('c', 40, 0, 10, 10)];
    const result = distributeObjects(objs, 'horizontal');
    // First center=5, last center=45, step=20
    // Middle center should be 25, width=10, so min.x=20
    expect(result.get('b')!.min.x).toBe(20);
    // First and last should stay at their original centers
    expect(result.get('a')!.min.x).toBe(0);
    expect(result.get('c')!.min.x).toBe(40);
  });

  it('distributes 3 objects vertically', () => {
    const objs = [mkObj('a', 0, 0, 10, 10), mkObj('b', 0, 10, 10, 10), mkObj('c', 0, 40, 10, 10)];
    const result = distributeObjects(objs, 'vertical');
    // First center=5, last center=45, step=20
    expect(result.get('b')!.min.y).toBe(20);
  });

  it('distributes 5 objects horizontally with even spacing', () => {
    const objs = [
      mkObj('a', 0, 0, 10, 10),   // center=5
      mkObj('b', 8, 0, 10, 10),   // center=13
      mkObj('c', 30, 0, 10, 10),  // center=35
      mkObj('d', 50, 0, 10, 10),  // center=55
      mkObj('e', 80, 0, 10, 10),  // center=85
    ];
    const result = distributeObjects(objs, 'horizontal');
    // Sorted by center: a(5), b(13), c(35), d(55), e(85)
    // Step = (85-5)/4 = 20
    // Centers: 5, 25, 45, 65, 85
    expect(result.get('a')!.min.x).toBe(0);     // center=5
    expect(result.get('b')!.min.x).toBe(20);    // center=25
    expect(result.get('c')!.min.x).toBe(40);    // center=45
    expect(result.get('d')!.min.x).toBe(60);    // center=65
    expect(result.get('e')!.min.x).toBe(80);    // center=85
  });

  it('preserves object dimensions during distribution', () => {
    const objs = [mkObj('a', 0, 0, 15, 25), mkObj('b', 20, 0, 35, 45), mkObj('c', 80, 0, 10, 10)];
    const result = distributeObjects(objs, 'horizontal');
    const bB = result.get('b')!;
    expect(bB.max.x - bB.min.x).toBeCloseTo(35);
    expect(bB.max.y - bB.min.y).toBeCloseTo(45);
  });
});

// --- computeObjectSnap ---

describe('computeObjectSnap', () => {
  it('snaps to left edge of another object', () => {
    // Dragged object at x=48..58, other object at x=50..70
    const dragged = mkBounds(48, 0, 10, 10);
    const others = [mkSnapObj(50, 20, 20, 10)];
    const result = computeObjectSnap(dragged, others, 5);
    // Left edge 48 should snap to 50: dx=+2
    expect(result.dx).toBeCloseTo(2);
    expect(result.guides.length).toBeGreaterThan(0);
    expect(result.guides.some((g) => g.axis === 'x' && g.value === 50)).toBe(true);
  });

  it('snaps to center of another object', () => {
    // Dragged center = 14, other center = 15 (threshold=5)
    const dragged = mkBounds(9, 0, 10, 10); // center=14
    const others = [mkSnapObj(10, 20, 10, 10)]; // center=15
    const result = computeObjectSnap(dragged, others, 5);
    // Center 14 snaps to 15: dx=+1
    expect(result.dx).toBeCloseTo(1);
  });

  it('returns zero delta when outside threshold', () => {
    const dragged = mkBounds(0, 0, 10, 10);
    const others = [mkSnapObj(100, 100, 10, 10)];
    const result = computeObjectSnap(dragged, others, 5);
    expect(result.dx).toBe(0);
    expect(result.dy).toBe(0);
    expect(result.guides.length).toBe(0);
  });

  it('snaps on both axes independently', () => {
    const dragged = mkBounds(48, 48, 10, 10);
    const others = [mkSnapObj(50, 50, 20, 20)];
    const result = computeObjectSnap(dragged, others, 5);
    expect(result.dx).toBeCloseTo(2);
    expect(result.dy).toBeCloseTo(2);
    expect(result.guides.length).toBe(2);
  });

  it('picks closest snap when multiple objects are close', () => {
    const dragged = mkBounds(0, 0, 10, 10);
    const others = [
      mkSnapObj(11, 0, 10, 10), // left=11, distance from right(10)=1
      mkSnapObj(13, 0, 10, 10), // left=13, distance from right(10)=3
    ];
    const result = computeObjectSnap(dragged, others, 5);
    // Right edge (10) to first object's left (11) = 1px, vs right to second left (13) = 3px
    expect(result.dx).toBeCloseTo(1); // snap to 11
  });

  it('handles empty other objects', () => {
    const dragged = mkBounds(0, 0, 10, 10);
    const result = computeObjectSnap(dragged, [], 5);
    expect(result.dx).toBe(0);
    expect(result.dy).toBe(0);
    expect(result.guides.length).toBe(0);
  });

  it('emits no guides when already exactly aligned (no correction)', () => {
    // Dragged left edge exactly at other's left edge (x=50): dx === 0, so no
    // snap correction occurs and no phantom guide should flash.
    const dragged = mkBounds(50, 0, 10, 10);
    const others = [mkSnapObj(50, 20, 20, 10)];
    const result = computeObjectSnap(dragged, others, 5);
    expect(result.dx).toBe(0);
    expect(result.dy).toBe(0);
    expect(result.guides.length).toBe(0);
  });

  it('emits a guide only for the axis that actually snapped', () => {
    // X already aligned (dx === 0), Y within threshold (dy !== 0): only the
    // y-axis correction should produce a guide.
    const dragged = mkBounds(50, 18, 10, 10);
    const others = [mkSnapObj(50, 20, 20, 10)];
    const result = computeObjectSnap(dragged, others, 5);
    expect(result.dx).toBe(0);
    expect(result.dy).toBeCloseTo(2);
    expect(result.guides.some((g) => g.axis === 'x')).toBe(false);
    expect(result.guides.some((g) => g.axis === 'y' && g.value === 20)).toBe(true);
  });
});

describe('resolveGeometrySnap', () => {
  it('projects to the nearest point on a line segment', () => {
    const line = mkVectorPathObj(
      'line',
      'M 0 0 L 10 0',
      mkBounds(0, 0, 10, 0),
    );

    const result = resolveGeometrySnap({ x: 2, y: 0.5 }, [line], 1);
    expect(result).not.toBeNull();
    expect(result?.targetClass).toBe('line');
    expect(result?.snappedTo.x).toBeCloseTo(2);
    expect(result?.snappedTo.y).toBeCloseTo(0);
    expect(result?.dx).toBeCloseTo(0);
    expect(result?.dy).toBeCloseTo(-0.5);
  });

  it('prefers explicit point targets over line projections', () => {
    const line = mkVectorPathObj(
      'line',
      'M 0 0 L 10 0',
      mkBounds(0, 0, 10, 0),
    );

    const result = resolveGeometrySnap({ x: 0.15, y: 0.2 }, [line], 1);
    expect(result).not.toBeNull();
    expect(result?.targetClass).toBe('point');
    expect(result?.snappedTo.x).toBeCloseTo(0);
    expect(result?.snappedTo.y).toBeCloseTo(0);
  });

  it('recognizes explicit ruler-guide metadata', () => {
    const guide = mkVectorPathObj(
      'guide',
      'M 0 0 L 0 100',
      { min: { x: 20, y: 0 }, max: { x: 20, y: 100 } },
      { data: { type: 'vector_path', path_data: 'M 0 0 L 0 100', closed: false, ruler_guide_axis: 'vertical' } },
    );

    expect(getRulerGuideAxis(guide)).toBe('vertical');
    expect(isRulerGuideObject(guide)).toBe(true);
  });

  it('keeps a preferred snap target within the wider release band', () => {
    const line = mkVectorPathObj(
      'line',
      'M 0 0 L 10 0',
      mkBounds(0, 0, 10, 0),
    );

    const snapped = resolveGeometrySnap({ x: 2, y: 0.5 }, [line], 1);
    expect(snapped?.targetKey).toBe('line:seg:0');

    const sticky = resolveGeometrySnap(
      { x: 2, y: 1.9 },
      [line],
      1,
      undefined,
      { preferredTargetKey: 'line:seg:0', preferredReleaseMultiplier: 2.1 },
    );
    expect(sticky?.targetKey).toBe('line:seg:0');
    expect(sticky?.targetClass).toBe('line');
    expect(sticky?.dy).toBeCloseTo(-1.9);
  });
});

describe('computeSelectionSnap', () => {
  it('chooses the anchor with the smallest movement adjustment', () => {
    const line = mkVectorPathObj(
      'line',
      'M 10 0 L 10 20',
      { min: { x: 10, y: 0 }, max: { x: 10, y: 20 } },
    );

    const result = computeSelectionSnap(
      mkBounds(0, 0, 10, 10),
      [{ x: 9.6, y: 2 }, { x: 1, y: 1 }],
      [line],
      2,
    );

    expect(result).not.toBeNull();
    expect(result?.targetClass).toBe('line');
    expect(result?.dx).toBeCloseTo(0.4);
    expect(result?.dy).toBeCloseTo(0);
  });
});

// --- Helpers for transform/visual-bounds tests ---

function makeIdentityTransform(): Transform2D {
  return { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 };
}

function makeRectObject(overrides?: Partial<ProjectObject>): ProjectObject {
  return makeProjectObject({
    id: 'obj-1',
    name: 'Rect',
    transform: makeIdentityTransform(),
    bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } },
    layer_id: 'layer-1',
    data: { type: 'shape', kind: 'rectangle', width: 10, height: 10, corner_radius: 0 },
    ...overrides,
  });
}

// --- applyAroundCenter ---

describe('applyAroundCenter', () => {
  it('identity preserves point', () => {
    const t = makeIdentityTransform();
    const pt: Point2D = { x: 5, y: 7 };
    const center: Point2D = { x: 0, y: 0 };
    const result = applyAroundCenter(t, pt, center);
    expect(result.x).toBeCloseTo(5);
    expect(result.y).toBeCloseTo(7);
  });

  it('90-degree rotation around origin maps (1,0) to (0,1)', () => {
    // 90-degree CCW rotation: a=cos(90)=0, b=sin(90)=1, c=-sin(90)=-1, d=cos(90)=0
    const t: Transform2D = { a: 0, b: 1, c: -1, d: 0, tx: 0, ty: 0 };
    const pt: Point2D = { x: 1, y: 0 };
    const center: Point2D = { x: 0, y: 0 };
    const result = applyAroundCenter(t, pt, center);
    expect(result.x).toBeCloseTo(0);
    expect(result.y).toBeCloseTo(1);
  });

  it('rotation around non-zero center', () => {
    // 90-degree CCW rotation around center (5,5)
    const t: Transform2D = { a: 0, b: 1, c: -1, d: 0, tx: 0, ty: 0 };
    const pt: Point2D = { x: 10, y: 5 }; // 5 units right of center
    const center: Point2D = { x: 5, y: 5 };
    const result = applyAroundCenter(t, pt, center);
    // rx=5, ry=0 => new x = 0*5 + (-1)*0 + 0 + 5 = 5
    //               new y = 1*5 + 0*0 + 0 + 5 = 10
    expect(result.x).toBeCloseTo(5);
    expect(result.y).toBeCloseTo(10);
  });
});

// --- mapLocalToBounds ---

describe('mapLocalToBounds', () => {
  it('maps vertices into bounds', () => {
    // Local points forming a unit square 0..1
    const localPts: Point2D[] = [
      { x: 0, y: 0 },
      { x: 1, y: 0 },
      { x: 1, y: 1 },
      { x: 0, y: 1 },
    ];
    const bounds: Bounds = { min: { x: 10, y: 20 }, max: { x: 30, y: 60 } };
    const mapped = mapLocalToBounds(localPts, bounds);

    // 0 maps to min, 1 maps to max
    expect(mapped[0].x).toBeCloseTo(10);
    expect(mapped[0].y).toBeCloseTo(20);
    expect(mapped[1].x).toBeCloseTo(30);
    expect(mapped[1].y).toBeCloseTo(20);
    expect(mapped[2].x).toBeCloseTo(30);
    expect(mapped[2].y).toBeCloseTo(60);
    expect(mapped[3].x).toBeCloseTo(10);
    expect(mapped[3].y).toBeCloseTo(60);
  });
});

// --- computeVisualBoundsWorld ---

describe('computeVisualBoundsWorld', () => {
  it('preserves the identity-transform fast path for vector paths', () => {
    const obj = mkVectorPathObj('vector-identity', 'M 0 0 L 10 0 L 10 10', mkBounds(10, 20, 30, 40));
    expect(computeVisualBoundsWorld(obj)).toBe(obj.bounds);
  });

  it('with identity transform equals raw bounds', () => {
    const obj = makeRectObject();
    const vb = computeVisualBoundsWorld(obj);
    expect(vb.min.x).toBeCloseTo(0);
    expect(vb.min.y).toBeCloseTo(0);
    expect(vb.max.x).toBeCloseTo(10);
    expect(vb.max.y).toBeCloseTo(10);
  });

  it('with 45-degree rotation extends beyond raw bounds', () => {
    const cos45 = Math.cos(Math.PI / 4);
    const sin45 = Math.sin(Math.PI / 4);
    const obj = makeRectObject({
      transform: { a: cos45, b: sin45, c: -sin45, d: cos45, tx: 0, ty: 0 },
    });
    const vb = computeVisualBoundsWorld(obj);
    // A 10x10 square rotated 45 degrees around its center should have a
    // visual AABB larger than 10x10 — the diagonal is 10*sqrt(2) ~ 14.14
    const vbW = vb.max.x - vb.min.x;
    const vbH = vb.max.y - vb.min.y;
    expect(vbW).toBeGreaterThan(10);
    expect(vbH).toBeGreaterThan(10);
    expect(vbW).toBeCloseTo(10 * Math.SQRT2, 1);
    expect(vbH).toBeCloseTo(10 * Math.SQRT2, 1);
  });
});

describe('computeTransformedBoundsWorld', () => {
  it('returns the coarse transformed AABB from raw bounds corners', () => {
    const cos45 = Math.cos(Math.PI / 4);
    const sin45 = Math.sin(Math.PI / 4);
    const obj = makeRectObject({
      bounds: mkBounds(0, 0, 100, 50),
      transform: { a: cos45, b: sin45, c: -sin45, d: cos45, tx: 0, ty: 0 },
    });

    const bounds = computeTransformedBoundsWorld(obj);
    expect(bounds.min.x).toBeLessThan(obj.bounds.min.x);
    expect(bounds.min.y).toBeLessThan(obj.bounds.min.y);
    expect(bounds.max.x).toBeGreaterThan(obj.bounds.max.x);
    expect(bounds.max.y).toBeGreaterThan(obj.bounds.max.y);
  });
});

// --- computeSelectionPivot ---

describe('computeSelectionPivot', () => {
  it('returns center of combined visual bounds', () => {
    const obj1 = makeRectObject({
      id: 'a',
      bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } },
      data: { type: 'shape', kind: 'rectangle', width: 10, height: 10, corner_radius: 0 },
    });
    const obj2 = makeRectObject({
      id: 'b',
      bounds: { min: { x: 20, y: 20 }, max: { x: 30, y: 30 } },
      data: { type: 'shape', kind: 'rectangle', width: 10, height: 10, corner_radius: 0 },
    });
    const pivot = computeSelectionPivot([obj1, obj2]);
    // Combined bounds: (0,0)-(30,30), center = (15,15)
    expect(pivot.x).toBeCloseTo(15);
    expect(pivot.y).toBeCloseTo(15);
  });
});

// --- getObjectSnapPoints ---

describe('getObjectSnapPoints', () => {
  it('returns world-space points for rect — at least 9 base points', () => {
    const obj = makeRectObject({
      bounds: { min: { x: 10, y: 20 }, max: { x: 30, y: 40 } },
      data: { type: 'shape', kind: 'rectangle', width: 20, height: 20, corner_radius: 0 },
    });
    const pts = getObjectSnapPoints(obj);
    // Base points: 4 corners + 4 midpoints + 1 center = 9
    expect(pts.length).toBeGreaterThanOrEqual(9);

    // Verify some known world-space points exist
    const hasPoint = (x: number, y: number) =>
      pts.some((p) => Math.abs(p.x - x) < 0.01 && Math.abs(p.y - y) < 0.01);
    expect(hasPoint(10, 20)).toBe(true); // top-left corner
    expect(hasPoint(30, 40)).toBe(true); // bottom-right corner
    expect(hasPoint(20, 30)).toBe(true); // center
    expect(hasPoint(20, 20)).toBe(true); // top midpoint
    expect(hasPoint(30, 30)).toBe(true); // right midpoint
  });

  it('returns world-space points for polygon — more than 9 base points', () => {
    const obj = makeRectObject({
      bounds: { min: { x: 0, y: 0 }, max: { x: 20, y: 20 } },
      data: { type: 'polygon', sides: 6, radius: 10 },
    });
    const pts = getObjectSnapPoints(obj);
    // Base 9 + polygon vertices (6) + edge midpoints (6) = 21
    expect(pts.length).toBeGreaterThan(9);
  });

  it('reuses cached world-space points until bounds change', () => {
    const obj = makeRectObject({
      id: 'cache-rect',
      bounds: { min: { x: 10, y: 20 }, max: { x: 30, y: 40 } },
      data: { type: 'shape', kind: 'rectangle', width: 20, height: 20, corner_radius: 0 },
    });

    const first = getObjectSnapPoints(obj);
    const second = getObjectSnapPoints(obj);
    expect(second).toBe(first);

    obj.bounds.min.x += 5;
    obj.bounds.max.x += 5;

    const third = getObjectSnapPoints(obj);
    expect(third).not.toBe(first);
    expect(third.some((p) => Math.abs(p.x - 15) < 0.01 && Math.abs(p.y - 20) < 0.01)).toBe(true);
  });

  it('caps heavy vector snap points and skips quadratic expansion for dense paths', () => {
    const repeatedSegments = Array.from({ length: 520 }, (_, index) => {
      const x = (index % 40) * 2;
      const y = Math.floor(index / 40) * 2;
      return `L ${x} ${y}`;
    }).join(' ');
    const obj = mkVectorPathObj(
      'heavy-vector',
      `M 0 0 ${repeatedSegments}`,
      { min: { x: 0, y: 0 }, max: { x: 100, y: 100 } },
    );

    const pts = getObjectSnapPoints(obj);

    // Base world snap points contribute 9 points. Dense vector-local samples are capped at 64.
    expect(pts.length).toBeLessThanOrEqual(73);
  });

  it('keeps open-path endpoints available on heavy vectors', () => {
    const repeatedSegments = Array.from({ length: 520 }, (_, index) => `L ${index + 1} 0`).join(' ');
    const obj = mkVectorPathObj(
      'heavy-open-vector',
      `M 0 0 ${repeatedSegments}`,
      { min: { x: 10, y: 20 }, max: { x: 110, y: 20 } },
    );

    const pts = getObjectSnapPoints(obj);
    const hasPoint = (x: number, y: number) =>
      pts.some((p) => Math.abs(p.x - x) < 0.01 && Math.abs(p.y - y) < 0.01);

    expect(hasPoint(10, 20)).toBe(true);
    expect(hasPoint(110, 20)).toBe(true);
  });
});

describe('getObjectSnapSegments', () => {
  it('reuses cached world-space segments until bounds change', () => {
    const obj = makeRectObject({
      id: 'cache-segments',
      bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } },
      data: { type: 'shape', kind: 'rectangle', width: 10, height: 10, corner_radius: 0 },
    });

    const first = getObjectSnapSegments(obj);
    const second = getObjectSnapSegments(obj);
    expect(second).toBe(first);

    obj.bounds.min.y += 5;
    obj.bounds.max.y += 5;

    const third = getObjectSnapSegments(obj);
    expect(third).not.toBe(first);
    expect(third[0]?.start.y).toBeCloseTo(5);
  });
});

// --- computePointSnap ---

describe('computePointSnap', () => {
  it('snaps to nearest point within threshold', () => {
    const target = makeRectObject({
      bounds: { min: { x: 10, y: 10 }, max: { x: 20, y: 20 } },
      data: { type: 'shape', kind: 'rectangle', width: 10, height: 10, corner_radius: 0 },
    });
    // Drag point very close to the top-left corner (10, 10)
    const draggedPoint: Point2D = { x: 10.5, y: 10.3 };
    const result = computePointSnap(draggedPoint, [target], 2);
    // Should snap to (10, 10)
    expect(result.dx).toBeCloseTo(10 - 10.5);
    expect(result.dy).toBeCloseTo(10 - 10.3);
    expect(result.snappedTo).toBeDefined();
    expect(result.snappedTo!.x).toBeCloseTo(10);
    expect(result.snappedTo!.y).toBeCloseTo(10);
    expect(result.guides.length).toBeGreaterThan(0);
  });

  it('returns zero delta when no points within threshold', () => {
    const target = makeRectObject({
      bounds: { min: { x: 100, y: 100 }, max: { x: 110, y: 110 } },
      data: { type: 'shape', kind: 'rectangle', width: 10, height: 10, corner_radius: 0 },
    });
    // Drag point far from any snap point
    const draggedPoint: Point2D = { x: 0, y: 0 };
    const result = computePointSnap(draggedPoint, [target], 2);
    expect(result.dx).toBe(0);
    expect(result.dy).toBe(0);
    expect(result.guides.length).toBe(0);
    expect(result.snappedTo).toBeUndefined();
  });
});
