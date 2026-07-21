import { describe, it, expect, vi } from 'vitest';
import { hitTestPoint, hitTestRect, hitTestHandle, hitTestSelectionEdge, hitTestSnapPoint } from './hitTest';
import type { ProjectObject, Transform2D, Bounds, TransformLocks } from '../types/project';
import type { ViewportParams } from './ViewportTransform';
import { makeProjectObject } from '../test-utils/projectFixtures';
import * as alignment from './alignment';

const identity: Transform2D = { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 };

const defaultVp: ViewportParams = {
  offset: { x: 200, y: 200 },
  zoom: 100,
  canvasWidth: 800,
  canvasHeight: 600,
};

function makeObj(
  id: string,
  bounds: Bounds,
  z_index: number = 0,
  opts?: { visible?: boolean; locked?: boolean },
): ProjectObject {
  return makeProjectObject({
    id,
    name: id,
    visible: opts?.visible ?? true,
    locked: opts?.locked ?? false,
    transform: identity,
    bounds,
    layer_id: 'layer1',
    z_index,
    data: { type: 'shape', kind: 'rectangle', width: bounds.max.x - bounds.min.x, height: bounds.max.y - bounds.min.y, corner_radius: 0 },
  });
}

describe('hitTestPoint', () => {
  it('returns the topmost object at a screen point', () => {
    const objects = [
      makeObj('a', { min: { x: 100, y: 100 }, max: { x: 200, y: 200 } }, 1),
      makeObj('b', { min: { x: 150, y: 150 }, max: { x: 250, y: 250 } }, 2),
    ];

    // Screen point that hits both objects — should return 'b' (higher z_index)
    // Object 'b' bounds min at (150,150) world:
    //   screenX = (150 - 200) * 2 + 400 = -100 + 400 = 300
    //   screenY = (150 - 200) * 2 + 300 = -100 + 300 = 200
    // Object 'b' bounds max at (250,250) world:
    //   screenX = (250 - 200) * 2 + 400 = 100 + 400 = 500
    //   screenY = (250 - 200) * 2 + 300 = 100 + 300 = 400
    const hit = hitTestPoint({ x: 350, y: 250 }, objects, defaultVp);
    expect(hit?.id).toBe('b');
  });

  it('returns null when no object is hit', () => {
    const objects = [
      makeObj('a', { min: { x: 100, y: 100 }, max: { x: 200, y: 200 } }, 1),
    ];
    const hit = hitTestPoint({ x: 0, y: 0 }, objects, defaultVp);
    expect(hit).toBeNull();
  });

  it('skips invisible objects', () => {
    const objects = [
      makeObj('a', { min: { x: 100, y: 100 }, max: { x: 300, y: 300 } }, 1, { visible: false }),
    ];
    // The object covers a large area but is invisible
    const hit = hitTestPoint({ x: 400, y: 300 }, objects, defaultVp);
    expect(hit).toBeNull();
  });

  it('skips locked objects', () => {
    const objects = [
      makeObj('a', { min: { x: 100, y: 100 }, max: { x: 300, y: 300 } }, 1, { locked: true }),
    ];
    const hit = hitTestPoint({ x: 400, y: 300 }, objects, defaultVp);
    expect(hit).toBeNull();
  });

  it('includes locked objects when requested', () => {
    const objects = [
      makeObj('a', { min: { x: 100, y: 100 }, max: { x: 300, y: 300 } }, 1, { locked: true }),
    ];
    const hit = hitTestPoint({ x: 400, y: 300 }, objects, defaultVp, true);
    expect(hit?.id).toBe('a');
  });

  it('can hit-test a zero-width ruler guide by line proximity', () => {
    const guide = makeProjectObject({
      id: 'guide',
      name: 'Guide',
      visible: true,
      locked: false,
      transform: identity,
      bounds: { min: { x: 150, y: 100 }, max: { x: 150, y: 200 } },
      layer_id: 'layer1',
      z_index: 1,
      data: {
        type: 'vector_path',
        path_data: 'M 0 0 L 0 100',
        closed: false,
        ruler_guide_axis: 'vertical',
      },
    });

    const hit = hitTestPoint({ x: 302, y: 100 }, [guide], defaultVp);
    expect(hit?.id).toBe('guide');
  });

  it('gives ruler guides a wider hit target than ordinary bounds tests', () => {
    const guide = makeProjectObject({
      id: 'guide',
      name: 'Guide',
      visible: true,
      locked: false,
      transform: identity,
      bounds: { min: { x: 150, y: 100 }, max: { x: 150, y: 200 } },
      layer_id: 'layer1',
      z_index: 1,
      data: {
        type: 'vector_path',
        path_data: 'M 0 0 L 0 100',
        closed: false,
        ruler_guide_axis: 'vertical',
      },
    });

    const hit = hitTestPoint({ x: 309, y: 100 }, [guide], defaultVp);
    expect(hit?.id).toBe('guide');
  });

  it('can hit-test a zero-width open vector segment by stroke proximity', () => {
    const segment = makeProjectObject({
      id: 'segment',
      name: 'Vertical Segment',
      visible: true,
      locked: false,
      transform: identity,
      bounds: { min: { x: 150, y: 100 }, max: { x: 150, y: 200 } },
      layer_id: 'layer1',
      z_index: 1,
      data: {
        type: 'vector_path',
        path_data: 'M 0 0 L 0 100',
        closed: false,
        ruler_guide_axis: null,
      },
    });

    const hit = hitTestPoint({ x: 303, y: 100 }, [segment], defaultVp);
    expect(hit?.id).toBe('segment');
  });

  it('can hit-test a zero-height open vector segment by stroke proximity', () => {
    const segment = makeProjectObject({
      id: 'segment',
      name: 'Horizontal Segment',
      visible: true,
      locked: false,
      transform: identity,
      bounds: { min: { x: 150, y: 100 }, max: { x: 250, y: 100 } },
      layer_id: 'layer1',
      z_index: 1,
      data: {
        type: 'vector_path',
        path_data: 'M 0 0 L 100 0',
        closed: false,
        ruler_guide_axis: null,
      },
    });

    const hit = hitTestPoint({ x: 400, y: 103 }, [segment], defaultVp);
    expect(hit?.id).toBe('segment');
  });
});

describe('hitTestRect', () => {
  it('returns objects whose screen bounds intersect the given rect', () => {
    const objects = [
      makeObj('a', { min: { x: 100, y: 100 }, max: { x: 200, y: 200 } }, 1),
      makeObj('b', { min: { x: 300, y: 300 }, max: { x: 400, y: 400 } }, 2),
    ];

    // Object 'a' screen bounds: (200, 100) to (400, 300)
    // A rect that covers part of 'a' but not 'b'
    const hits = hitTestRect(
      { min: { x: 250, y: 150 }, max: { x: 350, y: 250 } },
      objects,
      defaultVp,
    );
    expect(hits.map((o) => o.id)).toEqual(['a']);
  });

  it('returns empty array when nothing intersects', () => {
    const objects = [
      makeObj('a', { min: { x: 100, y: 100 }, max: { x: 200, y: 200 } }, 1),
    ];
    const hits = hitTestRect(
      { min: { x: 0, y: 0 }, max: { x: 50, y: 50 } },
      objects,
      defaultVp,
    );
    expect(hits).toEqual([]);
  });

  it('can include locked objects for marquee selection', () => {
    const objects = [
      makeObj('a', { min: { x: 100, y: 100 }, max: { x: 200, y: 200 } }, 1, { locked: true }),
    ];
    expect(hitTestRect(
      { min: { x: 250, y: 150 }, max: { x: 350, y: 250 } },
      objects,
      defaultVp,
    )).toEqual([]);
    expect(hitTestRect(
      { min: { x: 250, y: 150 }, max: { x: 350, y: 250 } },
      objects,
      defaultVp,
      true,
    ).map((object) => object.id)).toEqual(['a']);
  });
});

describe('hitTestHandle', () => {
  it('returns null when no objects are selected', () => {
    const result = hitTestHandle({ x: 0, y: 0 }, [], defaultVp);
    expect(result).toBeNull();
  });

  it('returns the handle id when a handle is hit', () => {
    const obj = makeObj('a', { min: { x: 150, y: 150 }, max: { x: 250, y: 250 } }, 1);
    // 'a' screen top-left: (150-200)*2+400=300, (150-200)*2+300=200
    // NW handle should be near (300, 200)
    const result = hitTestHandle({ x: 300, y: 200 }, [obj], defaultVp);
    expect(result).toBe('nw');
  });

  it('returns a rotation handle when clicking the visible arrow arc', () => {
    const obj = makeObj('a', { min: { x: 150, y: 150 }, max: { x: 250, y: 250 } }, 1);
    // NW rotation handle center is offset to (290, 190); visible arc midpoint
    // sits around radius 12 on the upper-left diagonal, outside the old center box.
    const result = hitTestHandle({ x: 282, y: 182 }, [obj], defaultVp);
    expect(result).toBe('rotate_nw');
  });

  it('keeps corner resize precedence over nearby rotation handles', () => {
    const obj = makeObj('a', { min: { x: 150, y: 150 }, max: { x: 250, y: 250 } }, 1);
    const result = hitTestHandle({ x: 300, y: 200 }, [obj], defaultVp);
    expect(result).toBe('nw');
  });

  it('returns the center move handle at the midpoint of a selected flat line', () => {
    const segment = makeProjectObject({
      id: 'segment',
      name: 'Horizontal Segment',
      visible: true,
      locked: false,
      transform: identity,
      bounds: { min: { x: 150, y: 100 }, max: { x: 250, y: 100 } },
      layer_id: 'layer1',
      z_index: 1,
      data: {
        type: 'vector_path',
        path_data: 'M 0 0 L 100 0',
        closed: false,
        ruler_guide_axis: null,
      },
    });

    const result = hitTestHandle({ x: 400, y: 100 }, [segment], defaultVp);
    expect(result).toBe('center');
  });

  it('returns null when point is not near any handle', () => {
    const obj = makeObj('a', { min: { x: 150, y: 150 }, max: { x: 250, y: 250 } }, 1);
    const result = hitTestHandle({ x: 0, y: 0 }, [obj], defaultVp);
    expect(result).toBeNull();
  });
});

describe('hitTestSelectionEdge', () => {
  it('returns null when move is locked', () => {
    const obj = makeObj('a', { min: { x: 150, y: 150 }, max: { x: 250, y: 250 } }, 1);
    // Screen top-left of obj: ((150-200)*2+400, (150-200)*2+300) = (300, 200)
    // Clicking right on the left edge should normally return a world point
    const locks: TransformLocks = { move_enabled: false, size_enabled: true, rotate_enabled: true, shear_enabled: true };
    const result = hitTestSelectionEdge({ x: 300, y: 250 }, [obj], defaultVp, locks);
    expect(result).toBeNull();
  });
});

describe('hitTestSnapPoint', () => {
  it('returns null when move is locked', () => {
    const obj = makeObj('a', { min: { x: 150, y: 150 }, max: { x: 250, y: 250 } }, 1);
    // Screen top-left corner: (300, 200) — this is a snap point
    const locks: TransformLocks = { move_enabled: false, size_enabled: true, rotate_enabled: true, shear_enabled: true };
    const result = hitTestSnapPoint({ x: 300, y: 200 }, [obj], defaultVp, locks);
    expect(result).toBeNull();
  });

  it('uses cheap vector snap-point hit targets instead of full snap-point expansion for heavy vectors', () => {
    const expensiveSnapSpy = vi.spyOn(alignment, 'getObjectSnapPoints');
    const heavyPath = `M 0 0 ${Array.from({ length: 4000 }, (_, index) => `L ${index + 1} ${index % 17}`).join(' ')}`;
    const obj = makeProjectObject({
      id: 'heavy-vector',
      name: 'Heavy Vector',
      transform: identity,
      bounds: { min: { x: 150, y: 150 }, max: { x: 250, y: 250 } },
      layer_id: 'layer1',
      z_index: 1,
      data: {
        type: 'vector_path',
        path_data: heavyPath,
        closed: false,
      },
    });

    const result = hitTestSnapPoint({ x: 300, y: 200 }, [obj], defaultVp);
    expect(result).not.toBeNull();
    expect(expensiveSnapSpy).not.toHaveBeenCalled();
  });
});
