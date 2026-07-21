import RBush from 'rbush';
import type { Bounds, ProjectObject } from '../types/project';
import type { ViewportParams } from './ViewportTransform';
import { screenToWorld, screenToWorldDist } from './ViewportTransform';
import { computeTransformedBoundsWorld } from './alignment';

type BoundsRevision = {
  minX: number;
  minY: number;
  maxX: number;
  maxY: number;
  a: number;
  b: number;
  c: number;
  d: number;
  tx: number;
  ty: number;
};

type CachedTransformedBounds = {
  revision: BoundsRevision;
  bounds: Bounds;
};

export type SceneIndexItem = {
  minX: number;
  minY: number;
  maxX: number;
  maxY: number;
  objectId: string;
  object: ProjectObject;
  zIndex: number;
};

type SceneIndexSnapshot = {
  tree: RBush<SceneIndexItem>;
  itemsById: Map<string, SceneIndexItem>;
};

let transformedBoundsCache = new WeakMap<ProjectObject, CachedTransformedBounds>();
let sceneIndexCache = new WeakMap<ReadonlyArray<ProjectObject>, SceneIndexSnapshot>();

function getBoundsRevision(obj: ProjectObject): BoundsRevision {
  return {
    minX: obj.bounds.min.x,
    minY: obj.bounds.min.y,
    maxX: obj.bounds.max.x,
    maxY: obj.bounds.max.y,
    a: obj.transform.a,
    b: obj.transform.b,
    c: obj.transform.c,
    d: obj.transform.d,
    tx: obj.transform.tx,
    ty: obj.transform.ty,
  };
}

function matchesRevision(a: BoundsRevision, b: BoundsRevision): boolean {
  return (
    a.minX === b.minX &&
    a.minY === b.minY &&
    a.maxX === b.maxX &&
    a.maxY === b.maxY &&
    a.a === b.a &&
    a.b === b.b &&
    a.c === b.c &&
    a.d === b.d &&
    a.tx === b.tx &&
    a.ty === b.ty
  );
}

export function getCachedTransformedBoundsWorld(obj: ProjectObject): Bounds {
  const revision = getBoundsRevision(obj);
  const cached = transformedBoundsCache.get(obj);
  if (cached && matchesRevision(cached.revision, revision)) {
    return cached.bounds;
  }

  // `computeTransformedBoundsWorld` returns `obj.bounds` by reference for the
  // identity-transform fast-exit. Deep-copy before storing: otherwise any
  // later in-place mutation of `obj.bounds` (e.g. during a drag) silently
  // mutates the cached entry, and a subsequent call with a different obj
  // sharing the same id but matching the *stored* revision snapshot would
  // incorrectly return the mutated-but-still-matching bounds.
  const bounds = computeTransformedBoundsWorld(obj);
  transformedBoundsCache.set(obj, {
    revision,
    bounds: {
      min: { x: bounds.min.x, y: bounds.min.y },
      max: { x: bounds.max.x, y: bounds.max.y },
    },
  });
  return bounds;
}

function buildSceneIndex(objects: ReadonlyArray<ProjectObject>): SceneIndexSnapshot {
  const tree = new RBush<SceneIndexItem>();
  const itemsById = new Map<string, SceneIndexItem>();
  const items: SceneIndexItem[] = [];

  for (const obj of objects) {
    if (!obj.visible) continue;
    const bounds = getCachedTransformedBoundsWorld(obj);
    const item: SceneIndexItem = {
      minX: bounds.min.x,
      minY: bounds.min.y,
      maxX: bounds.max.x,
      maxY: bounds.max.y,
      objectId: obj.id,
      object: obj,
      zIndex: obj.z_index,
    };
    items.push(item);
    itemsById.set(obj.id, item);
  }

  tree.load(items);
  return { tree, itemsById };
}

function getSceneIndex(objects: ReadonlyArray<ProjectObject>): SceneIndexSnapshot {
  const cached = sceneIndexCache.get(objects);
  if (cached) {
    // The scene index is intentionally keyed by the objects array identity, so snapshots
    // are treated as settled-state caches. Tools that mutate bounds in place during an
    // active drag should not rely on this snapshot until the drag has settled and the
    // authoritative object list is replaced or the cache is rebuilt.
    return cached;
  }
  const snapshot = buildSceneIndex(objects);
  sceneIndexCache.set(objects, snapshot);
  return snapshot;
}

function normalizeBounds(min: { x: number; y: number }, max: { x: number; y: number }): Bounds {
  return {
    min: { x: Math.min(min.x, max.x), y: Math.min(min.y, max.y) },
    max: { x: Math.max(min.x, max.x), y: Math.max(min.y, max.y) },
  };
}

function sortTopmostFirst(items: SceneIndexItem[]): ProjectObject[] {
  return items
    .sort((a, b) => b.zIndex - a.zIndex)
    .map((item) => item.object);
}

export function queryWorldBoundsCandidates(
  worldBounds: Bounds,
  objects: ReadonlyArray<ProjectObject>,
): ProjectObject[] {
  const snapshot = getSceneIndex(objects);
  return sortTopmostFirst(
    snapshot.tree.search({
      minX: worldBounds.min.x,
      minY: worldBounds.min.y,
      maxX: worldBounds.max.x,
      maxY: worldBounds.max.y,
    }),
  );
}

export function queryPointCandidates(
  screenPt: { x: number; y: number },
  objects: ReadonlyArray<ProjectObject>,
  vp: ViewportParams,
  tolerancePx = 6,
): ProjectObject[] {
  const world = screenToWorld(screenPt, vp);
  const toleranceMm = screenToWorldDist(tolerancePx, vp.zoom);
  return queryWorldBoundsCandidates(
    {
      min: { x: world.x - toleranceMm, y: world.y - toleranceMm },
      max: { x: world.x + toleranceMm, y: world.y + toleranceMm },
    },
    objects,
  );
}

export function queryRectCandidates(
  screenRect: { min: { x: number; y: number }; max: { x: number; y: number } },
  objects: ReadonlyArray<ProjectObject>,
  vp: ViewportParams,
): ProjectObject[] {
  const worldMin = screenToWorld(screenRect.min, vp);
  const worldMax = screenToWorld(screenRect.max, vp);
  const worldBounds = normalizeBounds(worldMin, worldMax);
  return queryWorldBoundsCandidates(worldBounds, objects);
}

export function resetSceneIndexCachesForTests(): void {
  transformedBoundsCache = new WeakMap<ProjectObject, CachedTransformedBounds>();
  sceneIndexCache = new WeakMap<ReadonlyArray<ProjectObject>, SceneIndexSnapshot>();
}
