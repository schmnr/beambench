import { beforeEach, describe, expect, it } from 'vitest';
import type { Bounds, ProjectObject, Transform2D } from '../types/project';
import type { ViewportParams } from './ViewportTransform';
import { makeProjectObject } from '../test-utils/projectFixtures';
import {
  queryPointCandidates,
  queryRectCandidates,
  queryWorldBoundsCandidates,
  resetSceneIndexCachesForTests,
} from './sceneIndex';

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
  zIndex = 0,
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
    z_index: zIndex,
    data: {
      type: 'shape',
      kind: 'rectangle',
      width: bounds.max.x - bounds.min.x,
      height: bounds.max.y - bounds.min.y,
      corner_radius: 0,
    },
  });
}

describe('sceneIndex', () => {
  beforeEach(() => {
    resetSceneIndexCachesForTests();
  });

  it('returns point candidates sorted topmost-first and excludes distant/invisible objects', () => {
    const objects = [
      makeObj('far', { min: { x: 400, y: 400 }, max: { x: 500, y: 500 } }, 3),
      makeObj('low', { min: { x: 100, y: 100 }, max: { x: 200, y: 200 } }, 1),
      makeObj('high', { min: { x: 120, y: 120 }, max: { x: 220, y: 220 } }, 2),
      makeObj('hidden', { min: { x: 110, y: 110 }, max: { x: 210, y: 210 } }, 4, { visible: false }),
    ];

    const candidates = queryPointCandidates({ x: 350, y: 250 }, objects, defaultVp);
    expect(candidates.map((obj) => obj.id)).toEqual(['high', 'low']);
  });

  it('keeps locked objects in candidate queries for selection-time filtering', () => {
    const unlocked = makeObj('unlocked', { min: { x: 100, y: 100 }, max: { x: 180, y: 180 } }, 1);
    const locked = makeObj(
      'locked',
      { min: { x: 110, y: 110 }, max: { x: 190, y: 190 } },
      2,
      { locked: true },
    );
    const objects = [unlocked, locked];

    expect(queryPointCandidates({ x: 350, y: 250 }, objects, defaultVp).map((obj) => obj.id)).toEqual([
      'locked',
      'unlocked',
    ]);
    expect(
      queryRectCandidates(
        { min: { x: 290, y: 190 }, max: { x: 410, y: 310 } },
        objects,
        defaultVp,
      ).map((obj) => obj.id),
    ).toEqual(['locked', 'unlocked']);
  });

  it('returns only intersecting rectangle candidates', () => {
    const objects = [
      makeObj('a', { min: { x: 100, y: 100 }, max: { x: 200, y: 200 } }, 1),
      makeObj('b', { min: { x: 300, y: 300 }, max: { x: 400, y: 400 } }, 2),
      makeObj('c', { min: { x: 150, y: 150 }, max: { x: 260, y: 260 } }, 3),
    ];

    const candidates = queryRectCandidates(
      { min: { x: 250, y: 150 }, max: { x: 420, y: 320 } },
      objects,
      defaultVp,
    );

    expect(candidates.map((obj) => obj.id)).toEqual(['c', 'a']);
  });

  it('returns only intersecting world-bounds candidates', () => {
    const objects = [
      makeObj('near', { min: { x: 100, y: 100 }, max: { x: 150, y: 150 } }, 1),
      makeObj('far', { min: { x: 300, y: 300 }, max: { x: 350, y: 350 } }, 2),
    ];

    const candidates = queryWorldBoundsCandidates(
      { min: { x: 90, y: 90 }, max: { x: 160, y: 160 } },
      objects,
    );

    expect(candidates.map((obj) => obj.id)).toEqual(['near']);
  });
});
