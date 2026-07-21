import type { Bounds, Point2D, Transform2D, ProjectObject } from '../types/project';
import {
  buildPolygonPoints,
  buildStarPath,
  parsePathData,
  type PathCommand,
  computePathBBox,
  mapPathCoordToBounds,
  getVectorPathRenderInfoForObject,
  quadraticExtremumT,
  quadraticPoint,
  quadAt,
  cubicAt,
} from './drawObjects';

// --- Types ---

export interface SnapLine {
  axis: 'x' | 'y';
  value: number; // world-mm coordinate
}

export interface ObjectSnapResult {
  dx: number; // adjustment to drag delta X
  dy: number; // adjustment to drag delta Y
  guides: SnapLine[];
}

export interface PointSnapResult {
  dx: number;
  dy: number;
  guides: SnapLine[];
  snappedTo?: Point2D;
}

export type GeometrySnapClass = 'point' | 'line' | 'bounds';

export interface GeometrySnapResult extends PointSnapResult {
  snappedTo: Point2D;
  targetKey: string;
  targetClass: GeometrySnapClass;
  distance: number;
}

export type AlignAxis = 'left' | 'right' | 'top' | 'bottom' | 'center-h' | 'center-v';
export type DistributeAxis = 'horizontal' | 'vertical';

type VectorVisualBoundsCacheEntry = {
  boundsMinX: number;
  boundsMinY: number;
  boundsMaxX: number;
  boundsMaxY: number;
  transformA: number;
  transformB: number;
  transformC: number;
  transformD: number;
  transformTx: number;
  transformTy: number;
  result: Bounds;
};

type WorldSnapGeometryRevision = {
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
  dataRef: ProjectObject['data'];
};

type WorldSnapPointsCacheEntry = {
  revision: WorldSnapGeometryRevision;
  points: Point2D[];
};

type WorldSnapSegmentsCacheEntry = {
  revision: WorldSnapGeometryRevision;
  segments: SnapSegment[];
};

let vectorVisualBoundsCache = new WeakMap<ReadonlyArray<unknown>, VectorVisualBoundsCacheEntry>();
let vectorLocalSnapPointsCache = new WeakMap<ReadonlyArray<unknown>, Point2D[]>();
let objectWorldSnapPointsCache = new WeakMap<ProjectObject, WorldSnapPointsCacheEntry>();
let objectWorldSnapSegmentsCache = new WeakMap<ProjectObject, WorldSnapSegmentsCacheEntry>();

const HEAVY_VECTOR_SNAP_COMMAND_THRESHOLD = 500;
const HEAVY_VECTOR_SAMPLES_PER_SUBPATH = 16;
const HEAVY_VECTOR_SAMPLE_LIMIT = 64;
const DETAILED_VECTOR_SEGMENT_INTERSECTION_LIMIT = 2000;
const POINT_EPSILON = 1e-6;

// --- Helpers ---

function centerX(b: Bounds): number {
  return (b.min.x + b.max.x) / 2;
}

function centerY(b: Bounds): number {
  return (b.min.y + b.max.y) / 2;
}

function getWorldSnapGeometryRevision(obj: ProjectObject): WorldSnapGeometryRevision {
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
    dataRef: obj.data,
  };
}

function matchesWorldSnapGeometryRevision(
  a: WorldSnapGeometryRevision,
  b: WorldSnapGeometryRevision,
): boolean {
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
    a.ty === b.ty &&
    a.dataRef === b.dataRef
  );
}

export function resetAlignmentCachesForTests(): void {
  vectorVisualBoundsCache = new WeakMap<ReadonlyArray<unknown>, VectorVisualBoundsCacheEntry>();
  vectorLocalSnapPointsCache = new WeakMap<ReadonlyArray<unknown>, Point2D[]>();
  objectWorldSnapPointsCache = new WeakMap<ProjectObject, WorldSnapPointsCacheEntry>();
  objectWorldSnapSegmentsCache = new WeakMap<ProjectObject, WorldSnapSegmentsCacheEntry>();
}

// --- Public functions ---

/**
 * Compute object snap adjustments for a dragged group's bounds against other objects.
 * Uses visual bounds (transform-aware) for accurate snapping on rotated/sheared objects.
 * Returns delta adjustments and guide lines to render.
 */
export function computeObjectSnap(
  draggedBounds: Bounds,
  otherObjects: ProjectObject[],
  thresholdMm: number,
  allObjects?: ProjectObject[],
): ObjectSnapResult {
  // Reference values from dragged bounds
  const dragRefX = [draggedBounds.min.x, draggedBounds.max.x, centerX(draggedBounds)];
  const dragRefY = [draggedBounds.min.y, draggedBounds.max.y, centerY(draggedBounds)];

  let bestDx = Infinity;
  let bestSnapX = NaN;
  let bestDy = Infinity;
  let bestSnapY = NaN;

  for (const other of otherObjects) {
    const vb = computeVisualBoundsWorld(other, allObjects);
    const otherRefX = [vb.min.x, vb.max.x, centerX(vb)];
    const otherRefY = [vb.min.y, vb.max.y, centerY(vb)];

    // Check X-axis snaps
    for (const dr of dragRefX) {
      for (const or of otherRefX) {
        const dist = Math.abs(dr - or);
        if (dist < thresholdMm && dist < Math.abs(bestDx)) {
          bestDx = or - dr;
          bestSnapX = or;
        }
      }
    }

    // Check Y-axis snaps
    for (const dr of dragRefY) {
      for (const or of otherRefY) {
        const dist = Math.abs(dr - or);
        if (dist < thresholdMm && dist < Math.abs(bestDy)) {
          bestDy = or - dr;
          bestSnapY = or;
        }
      }
    }
  }

  const guides: SnapLine[] = [];
  const dx = Number.isFinite(bestDx) ? bestDx : 0;
  const dy = Number.isFinite(bestDy) ? bestDy : 0;

  // Only emit a guide when an actual snap correction occurred. Emitting when
  // dx === 0 (already aligned) caused phantom snap-guide flashes during drags.
  if (dx !== 0 && Number.isFinite(bestSnapX)) {
    guides.push({ axis: 'x', value: bestSnapX });
  }
  if (dy !== 0 && Number.isFinite(bestSnapY)) {
    guides.push({ axis: 'y', value: bestSnapY });
  }

  return { dx, dy, guides };
}

/**
 * Compute new bounds for aligning objects along the given axis.
 * Returns a map of object ID to new bounds (translate only — width/height preserved).
 */
export function alignObjects(
  objects: { id: string; bounds: Bounds }[],
  axis: AlignAxis,
): Map<string, Bounds> {
  const result = new Map<string, Bounds>();
  if (objects.length < 2) return result;

  const combined = getCombinedBounds(objects.map((o) => o.bounds));

  for (const obj of objects) {
    const w = obj.bounds.max.x - obj.bounds.min.x;
    const h = obj.bounds.max.y - obj.bounds.min.y;
    let newMinX = obj.bounds.min.x;
    let newMinY = obj.bounds.min.y;

    switch (axis) {
      case 'left':
        newMinX = combined.min.x;
        break;
      case 'right':
        newMinX = combined.max.x - w;
        break;
      case 'top':
        newMinY = combined.min.y;
        break;
      case 'bottom':
        newMinY = combined.max.y - h;
        break;
      case 'center-h':
        newMinX = centerX(combined) - w / 2;
        break;
      case 'center-v':
        newMinY = centerY(combined) - h / 2;
        break;
    }

    result.set(obj.id, {
      min: { x: newMinX, y: newMinY },
      max: { x: newMinX + w, y: newMinY + h },
    });
  }

  return result;
}

/**
 * Compute new bounds for distributing objects evenly along an axis.
 * First and last (by center coordinate) stay fixed; intermediate objects are spaced evenly.
 * Requires 3+ objects; returns empty map for < 3.
 */
export function distributeObjects(
  objects: { id: string; bounds: Bounds }[],
  axis: DistributeAxis,
): Map<string, Bounds> {
  const result = new Map<string, Bounds>();
  if (objects.length < 3) return result;

  // Sort by center coordinate on the distribution axis
  const sorted = [...objects].sort((a, b) => {
    if (axis === 'horizontal') {
      return centerX(a.bounds) - centerX(b.bounds);
    }
    return centerY(a.bounds) - centerY(b.bounds);
  });

  const first = sorted[0];
  const last = sorted[sorted.length - 1];

  const startCenter = axis === 'horizontal' ? centerX(first.bounds) : centerY(first.bounds);
  const endCenter = axis === 'horizontal' ? centerX(last.bounds) : centerY(last.bounds);
  const step = (endCenter - startCenter) / (sorted.length - 1);

  for (let i = 0; i < sorted.length; i++) {
    const obj = sorted[i];
    const w = obj.bounds.max.x - obj.bounds.min.x;
    const h = obj.bounds.max.y - obj.bounds.min.y;
    const targetCenter = startCenter + step * i;

    let newMinX = obj.bounds.min.x;
    let newMinY = obj.bounds.min.y;

    if (axis === 'horizontal') {
      newMinX = targetCenter - w / 2;
    } else {
      newMinY = targetCenter - h / 2;
    }

    result.set(obj.id, {
      min: { x: newMinX, y: newMinY },
      max: { x: newMinX + w, y: newMinY + h },
    });
  }

  return result;
}

/**
 * Returns the bounding box encompassing all input bounds.
 */
export function getCombinedBounds(bounds: Bounds[]): Bounds {
  if (bounds.length === 0) {
    return { min: { x: 0, y: 0 }, max: { x: 0, y: 0 } };
  }

  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;

  for (const b of bounds) {
    if (b.min.x < minX) minX = b.min.x;
    if (b.min.y < minY) minY = b.min.y;
    if (b.max.x > maxX) maxX = b.max.x;
    if (b.max.y > maxY) maxY = b.max.y;
  }

  return { min: { x: minX, y: minY }, max: { x: maxX, y: maxY } };
}

// --- Transform helpers ---

/** Apply an affine transform to a point relative to a center. */
export function applyAroundCenter(t: Transform2D, pt: Point2D, center: Point2D): Point2D {
  const rx = pt.x - center.x, ry = pt.y - center.y;
  return {
    x: t.a * rx + t.c * ry + t.tx + center.x,
    y: t.b * rx + t.d * ry + t.ty + center.y,
  };
}

/** Map local shape-space vertices to bounds-space coordinates. */
export function mapLocalToBounds(localPts: Point2D[], bounds: Bounds): Point2D[] {
  let lMinX = Infinity, lMinY = Infinity, lMaxX = -Infinity, lMaxY = -Infinity;
  for (const p of localPts) {
    lMinX = Math.min(lMinX, p.x); lMinY = Math.min(lMinY, p.y);
    lMaxX = Math.max(lMaxX, p.x); lMaxY = Math.max(lMaxY, p.y);
  }
  const pathW = lMaxX - lMinX, pathH = lMaxY - lMinY;
  const bW = bounds.max.x - bounds.min.x, bH = bounds.max.y - bounds.min.y;
  const sx = pathW > 0 ? bW / pathW : 1;
  const sy = pathH > 0 ? bH / pathH : 1;
  const tx = bounds.min.x - lMinX * sx;
  const ty = bounds.min.y - lMinY * sy;
  return localPts.map(p => ({ x: p.x * sx + tx, y: p.y * sy + ty }));
}

// --- Visual bounds ---

function boundsCorners(b: Bounds): Point2D[] {
  return [
    { x: b.min.x, y: b.min.y },
    { x: b.max.x, y: b.min.y },
    { x: b.max.x, y: b.max.y },
    { x: b.min.x, y: b.max.y },
  ];
}

function boundsMidpoints(b: Bounds): Point2D[] {
  const cx = (b.min.x + b.max.x) / 2;
  const cy = (b.min.y + b.max.y) / 2;
  return [
    { x: cx, y: b.min.y },
    { x: b.max.x, y: cy },
    { x: cx, y: b.max.y },
    { x: b.min.x, y: cy },
  ];
}

function boundsCenter(b: Bounds): Point2D {
  return { x: (b.min.x + b.max.x) / 2, y: (b.min.y + b.max.y) / 2 };
}

export function computeTransformedBoundsWorld(obj: ProjectObject): Bounds {
  if (
    obj.transform.a === 1 &&
    obj.transform.b === 0 &&
    obj.transform.c === 0 &&
    obj.transform.d === 1 &&
    obj.transform.tx === 0 &&
    obj.transform.ty === 0
  ) {
    // Identity-transform fast path returns the object's live bounds reference.
    // Callers that retain/copy this result must clone it before caching.
    return obj.bounds;
  }

  const center = boundsCenter(obj.bounds);
  const transformedCorners = boundsCorners(obj.bounds).map((pt) =>
    applyAroundCenter(obj.transform, pt, center),
  );

  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;
  for (const corner of transformedCorners) {
    minX = Math.min(minX, corner.x);
    minY = Math.min(minY, corner.y);
    maxX = Math.max(maxX, corner.x);
    maxY = Math.max(maxY, corner.y);
  }
  return { min: { x: minX, y: minY }, max: { x: maxX, y: maxY } };
}

/**
 * Resolve a virtual_clone to a synthetic object with source geometry + clone bounds/transform.
 * Returns the original object if not a clone or if the chain can't be resolved.
 */
export function resolveCloneForGeometry(obj: ProjectObject, allObjects: ProjectObject[]): ProjectObject {
  if (obj.data.type !== 'virtual_clone') return obj;
  let current: ProjectObject = obj;
  let depth = 0;
  while (current.data.type === 'virtual_clone' && depth < 10) {
    const sourceId = current.data.source_id;
    const src = allObjects.find(o => o.id === sourceId);
    if (!src) return obj;
    current = src;
    depth++;
  }
  if (current.data.type === 'virtual_clone') return obj;
  return { ...obj, data: current.data };
}

/**
 * Returns points in bounds space whose AABB (after transform) defines the visual extent.
 */
function getVisualExtentPoints(obj: ProjectObject): Point2D[] {
  const corners = boundsCorners(obj.bounds);

  switch (obj.data.type) {
    case 'shape': {
      if (obj.data.kind === 'ellipse') {
        // Sample 36 points on the ellipse inscribed in bounds
        const cx = (obj.bounds.min.x + obj.bounds.max.x) / 2;
        const cy = (obj.bounds.min.y + obj.bounds.max.y) / 2;
        const rx = (obj.bounds.max.x - obj.bounds.min.x) / 2;
        const ry = (obj.bounds.max.y - obj.bounds.min.y) / 2;
        const pts: Point2D[] = [];
        for (let i = 0; i < 36; i++) {
          const theta = (i / 36) * Math.PI * 2;
          pts.push({ x: cx + rx * Math.cos(theta), y: cy + ry * Math.sin(theta) });
        }
        return pts;
      }
      // rectangle: bounds corners are exact
      return corners;
    }

    case 'text':
    case 'raster_image':
      return corners;

    case 'polygon': {
      const localPts = buildPolygonPoints(obj.data.sides, obj.data.radius);
      return mapLocalToBounds(localPts, obj.bounds);
    }

    case 'star': {
      const cmds = buildStarPath(
        obj.data.points,
        obj.data.bulge,
        obj.data.ratio,
        obj.data.dual_radius,
        obj.data.ratio2 ?? undefined,
        obj.data.corner_radius ?? 0,
        obj.data.corner_radii,
      );
      const localPts: Point2D[] = [];
      let current: Point2D | null = null;
      for (const cmd of cmds) {
        const end = { x: cmd.x, y: cmd.y };
        localPts.push(end);
        if (cmd.type === 'quad' && current) {
          const ctrl = { x: cmd.qx, y: cmd.qy };
          const etx = quadraticExtremumT(current.x, ctrl.x, end.x);
          if (etx !== null) {
            const mt = 1 - etx;
            localPts.push({
              x: mt * mt * current.x + 2 * mt * etx * ctrl.x + etx * etx * end.x,
              y: mt * mt * current.y + 2 * mt * etx * ctrl.y + etx * etx * end.y,
            });
          }
          const ety = quadraticExtremumT(current.y, ctrl.y, end.y);
          if (ety !== null) {
            const mt = 1 - ety;
            localPts.push({
              x: mt * mt * current.x + 2 * mt * ety * ctrl.x + ety * ety * end.x,
              y: mt * mt * current.y + 2 * mt * ety * ctrl.y + ety * ety * end.y,
            });
          }
        }
        current = end;
      }
      return mapLocalToBounds(localPts, obj.bounds);
    }

    case 'vector_path': {
      if (!obj.data.path_data) return corners;
      const parsed = parsePathData(obj.data.path_data);
      const bbox = computePathBBox(parsed);
      const bW = obj.bounds.max.x - obj.bounds.min.x;
      const bH = obj.bounds.max.y - obj.bounds.min.y;
      const bMinX = obj.bounds.min.x, bMinY = obj.bounds.min.y;
      const map = (x: number, y: number) => mapPathCoordToBounds(x, y, bbox, bMinX, bMinY, bW, bH);
      const pts: Point2D[] = [];
      let currX = 0, currY = 0;
      for (const cmd of parsed) {
        switch (cmd.type) {
          case 'M':
            pts.push(map(cmd.x, cmd.y));
            currX = cmd.x; currY = cmd.y;
            break;
          case 'L':
            pts.push(map(cmd.x, cmd.y));
            currX = cmd.x; currY = cmd.y;
            break;
          case 'Q': {
            // Sample the quadratic curve to get tight bounds (32 steps matches computePathBBox)
            const qSteps = 32;
            for (let i = 1; i <= qSteps; i++) {
              const t = i / qSteps;
              pts.push(map(
                quadAt(currX, cmd.x1!, cmd.x, t),
                quadAt(currY, cmd.y1!, cmd.y, t),
              ));
            }
            currX = cmd.x; currY = cmd.y;
            break;
          }
          case 'C': {
            // Sample the cubic curve to get tight bounds (48 steps matches computePathBBox)
            const cSteps = 48;
            for (let i = 1; i <= cSteps; i++) {
              const t = i / cSteps;
              pts.push(map(
                cubicAt(currX, cmd.x1!, cmd.x2!, cmd.x, t),
                cubicAt(currY, cmd.y1!, cmd.y2!, cmd.y, t),
              ));
            }
            currX = cmd.x; currY = cmd.y;
            break;
          }
          case 'Z':
            break;
        }
      }
      return pts.length > 0 ? pts : corners;
    }

    default:
      return corners;
  }
}

/**
 * Compute the tight world-space AABB of the actual rendered geometry,
 * taking the transform into account.
 */
export function computeVisualBoundsWorld(obj: ProjectObject, allObjects?: ProjectObject[]): Bounds {
  const resolved = allObjects ? resolveCloneForGeometry(obj, allObjects) : obj;
  // Fast path: identity transform means visual bounds == obj.bounds exactly
  // (mapPathCoordToBounds stretches the path to obj.bounds, and an identity
  // applyAroundCenter leaves the sampled points unchanged). This avoids
  // iterating thousands of curve-sampled points on every move/resize.
  const t = obj.transform;
  if (t.a === 1 && t.b === 0 && t.c === 0 && t.d === 1 && t.tx === 0 && t.ty === 0) {
    return obj.bounds;
  }
  if (resolved.data.type === 'vector_path' && resolved.data.path_data) {
    const info = getVectorPathRenderInfoForObject(resolved);
    if (!info || info.commands.length === 0) {
      return obj.bounds;
    }
    const parsed = info.commands;
    const cached = vectorVisualBoundsCache.get(parsed);
    if (
      cached &&
      cached.boundsMinX === obj.bounds.min.x &&
      cached.boundsMinY === obj.bounds.min.y &&
      cached.boundsMaxX === obj.bounds.max.x &&
      cached.boundsMaxY === obj.bounds.max.y &&
      cached.transformA === obj.transform.a &&
      cached.transformB === obj.transform.b &&
      cached.transformC === obj.transform.c &&
      cached.transformD === obj.transform.d &&
      cached.transformTx === obj.transform.tx &&
      cached.transformTy === obj.transform.ty
    ) {
      return cached.result;
    }

    const bbox = info.bbox;
    const bW = obj.bounds.max.x - obj.bounds.min.x;
    const bH = obj.bounds.max.y - obj.bounds.min.y;
    const bMinX = obj.bounds.min.x;
    const bMinY = obj.bounds.min.y;
    const map = (x: number, y: number) => mapPathCoordToBounds(x, y, bbox, bMinX, bMinY, bW, bH);
    const bc = boundsCenter(obj.bounds);
    let minX = Infinity;
    let minY = Infinity;
    let maxX = -Infinity;
    let maxY = -Infinity;
    let currX = 0;
    let currY = 0;
    const include = (x: number, y: number) => {
      const transformed = applyAroundCenter(obj.transform, map(x, y), bc);
      minX = Math.min(minX, transformed.x);
      minY = Math.min(minY, transformed.y);
      maxX = Math.max(maxX, transformed.x);
      maxY = Math.max(maxY, transformed.y);
    };

    for (const cmd of parsed) {
      switch (cmd.type) {
        case 'M':
          include(cmd.x, cmd.y);
          currX = cmd.x;
          currY = cmd.y;
          break;
        case 'L':
          include(cmd.x, cmd.y);
          currX = cmd.x;
          currY = cmd.y;
          break;
        case 'Q': {
          const qSteps = 32;
          for (let i = 1; i <= qSteps; i++) {
            const t = i / qSteps;
            include(
              quadAt(currX, cmd.x1!, cmd.x, t),
              quadAt(currY, cmd.y1!, cmd.y, t),
            );
          }
          currX = cmd.x;
          currY = cmd.y;
          break;
        }
        case 'C': {
          const cSteps = 48;
          for (let i = 1; i <= cSteps; i++) {
            const t = i / cSteps;
            include(
              cubicAt(currX, cmd.x1!, cmd.x2!, cmd.x, t),
              cubicAt(currY, cmd.y1!, cmd.y2!, cmd.y, t),
            );
          }
          currX = cmd.x;
          currY = cmd.y;
          break;
        }
        case 'Z':
          break;
      }
    }

    if (!isFinite(minX)) {
      return obj.bounds;
    }

    const result = { min: { x: minX, y: minY }, max: { x: maxX, y: maxY } };
    vectorVisualBoundsCache.set(parsed, {
      boundsMinX: obj.bounds.min.x,
      boundsMinY: obj.bounds.min.y,
      boundsMaxX: obj.bounds.max.x,
      boundsMaxY: obj.bounds.max.y,
      transformA: obj.transform.a,
      transformB: obj.transform.b,
      transformC: obj.transform.c,
      transformD: obj.transform.d,
      transformTx: obj.transform.tx,
      transformTy: obj.transform.ty,
      result,
    });
    return result;
  }

  const extentPts = getVisualExtentPoints(resolved);
  const bc = boundsCenter(obj.bounds);
  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
  for (const pt of extentPts) {
    const t = applyAroundCenter(obj.transform, pt, bc);
    minX = Math.min(minX, t.x); minY = Math.min(minY, t.y);
    maxX = Math.max(maxX, t.x); maxY = Math.max(maxY, t.y);
  }
  if (!isFinite(minX)) return obj.bounds;
  return { min: { x: minX, y: minY }, max: { x: maxX, y: maxY } };
}

/**
 * Compute the selection pivot from the tight visual-bounds AABB of all selected objects.
 */
export function computeSelectionPivot(objects: ProjectObject[], allObjects?: ProjectObject[]): Point2D {
  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
  for (const obj of objects) {
    const vb = computeVisualBoundsWorld(obj, allObjects);
    minX = Math.min(minX, vb.min.x); minY = Math.min(minY, vb.min.y);
    maxX = Math.max(maxX, vb.max.x); maxY = Math.max(maxY, vb.max.y);
  }
  return { x: (minX + maxX) / 2, y: (minY + maxY) / 2 };
}

// --- Geometry snap points ---

/**
 * Returns snap points for an object in final world-space coordinates.
 * These are the points that other objects can snap to.
 */
function buildObjectSnapPointsWorld(resolved: ProjectObject): Point2D[] {
  // Resolve virtual clones to source geometry (keeps clone's bounds/transform)
  const bc = boundsCenter(resolved.bounds);
  const t = resolved.transform;
  const worldCenter = applyAroundCenter(t, bc, bc);

  // Per-type base points: only include bbox points that lie on the actual geometry.
  // Rect/text/raster_image fill their bounds, so all 9 points are valid.
  // Ellipse: quadrant points (bounds midpoints) + center are on the shape; corners are not.
  // Polygon/star/vector_path: center only — shape-specific branches provide geometry points.
  const dtype = resolved.data.type;
  const isEllipse = dtype === 'shape' && resolved.data.kind === 'ellipse';
  const isRectLike = dtype === 'shape' && resolved.data.kind === 'rectangle';
  let worldBase: Point2D[];
  if (isRectLike || dtype === 'text' || dtype === 'raster_image' || dtype === 'barcode') {
    const basePts = [...boundsCorners(resolved.bounds), ...boundsMidpoints(resolved.bounds), bc];
    worldBase = basePts.map(pt => applyAroundCenter(t, pt, bc));
  } else if (isEllipse) {
    // Quadrant points coincide with bounds midpoints; corners are off the ellipse
    const basePts = [...boundsMidpoints(resolved.bounds), bc];
    worldBase = basePts.map(pt => applyAroundCenter(t, pt, bc));
  } else {
    // polygon, star, vector_path, group — only center; geometry points added below
    worldBase = [worldCenter];
  }

  switch (resolved.data.type) {
    case 'polygon': {
      const localPts = buildPolygonPoints(resolved.data.sides, resolved.data.radius);
      const boundsPts = mapLocalToBounds(localPts, resolved.bounds);
      // Add edge midpoints between consecutive vertices
      const mids: Point2D[] = [];
      for (let i = 0; i < boundsPts.length; i++) {
        const next = boundsPts[(i + 1) % boundsPts.length];
        mids.push({ x: (boundsPts[i].x + next.x) / 2, y: (boundsPts[i].y + next.y) / 2 });
      }
      const allPts = [...boundsPts, ...mids];
      return [...worldBase, ...allPts.map(pt => applyAroundCenter(t, pt, bc))];
    }

    case 'star': {
      const cmds = buildStarPath(
        resolved.data.points,
        resolved.data.bulge,
        resolved.data.ratio,
        resolved.data.dual_radius,
        resolved.data.ratio2 ?? undefined,
        resolved.data.corner_radius ?? 0,
        resolved.data.corner_radii,
      );
      // Collect endpoints + quadratic extrema (matching visual bounds model).
      // Track endpoint indices so midpoints use the same mapping as boundsPts.
      const localPts: Point2D[] = [];
      const endpointIndices: number[] = [];
      let starCurrent: Point2D | null = null;
      for (const cmd of cmds) {
        const end = { x: cmd.x, y: cmd.y };
        endpointIndices.push(localPts.length);
        localPts.push(end);
        if (cmd.type === 'quad' && starCurrent) {
          const ctrl = { x: cmd.qx, y: cmd.qy };
          const etx = quadraticExtremumT(starCurrent.x, ctrl.x, end.x);
          if (etx !== null) localPts.push(quadraticPoint(starCurrent, ctrl, end, etx));
          const ety = quadraticExtremumT(starCurrent.y, ctrl.y, end.y);
          if (ety !== null) localPts.push(quadraticPoint(starCurrent, ctrl, end, ety));
        }
        starCurrent = end;
      }
      const boundsPts = mapLocalToBounds(localPts, resolved.bounds);
      // Extract endpoint positions from the same mapping (same scale/offset as extrema)
      const endpointBounds = endpointIndices.map(i => boundsPts[i]);
      const mids: Point2D[] = [];
      for (let i = 0; i < endpointBounds.length; i++) {
        const next = endpointBounds[(i + 1) % endpointBounds.length];
        mids.push({ x: (endpointBounds[i].x + next.x) / 2, y: (endpointBounds[i].y + next.y) / 2 });
      }
      const allPts = [...boundsPts, ...mids];
      return [...worldBase, ...allPts.map(pt => applyAroundCenter(t, pt, bc))];
    }

    case 'vector_path': {
      if (!resolved.data.path_data) return worldBase;
      const info = getVectorPathRenderInfoForObject(resolved);
      if (!info || info.commands.length === 0) return worldBase;
      const localPts = getVectorLocalSnapPoints(info.commands);
      const pathBbox = info.bbox;
      const bW = resolved.bounds.max.x - resolved.bounds.min.x;
      const bH = resolved.bounds.max.y - resolved.bounds.min.y;
      const bMinX = resolved.bounds.min.x, bMinY = resolved.bounds.min.y;
      return [
        ...worldBase,
        ...localPts.map((pt) =>
          applyAroundCenter(
            t,
            mapPathCoordToBounds(pt.x, pt.y, pathBbox, bMinX, bMinY, bW, bH),
            bc,
          ),
        ),
      ];
    }

    default:
      return worldBase;
  }
}

export function getObjectSnapPoints(obj: ProjectObject, allObjects?: ProjectObject[]): Point2D[] {
  const resolved = allObjects ? resolveCloneForGeometry(obj, allObjects) : obj;
  const revision = getWorldSnapGeometryRevision(resolved);
  const cached = objectWorldSnapPointsCache.get(resolved);
  if (cached && matchesWorldSnapGeometryRevision(cached.revision, revision)) {
    return cached.points;
  }

  const points = buildObjectSnapPointsWorld(resolved);
  objectWorldSnapPointsCache.set(resolved, { revision, points });
  return points;
}

type VectorSubpath = {
  points: Point2D[];
  closed: boolean;
};

function pointsEqual(a: Point2D | null | undefined, b: Point2D | null | undefined): boolean {
  if (!a || !b) return false;
  return Math.abs(a.x - b.x) <= POINT_EPSILON && Math.abs(a.y - b.y) <= POINT_EPSILON;
}

function pushDistinctPoint(points: Point2D[], point: Point2D): void {
  if (pointsEqual(points[points.length - 1], point)) {
    return;
  }
  points.push(point);
}

function pushUniquePoint(points: Point2D[], point: Point2D): void {
  if (points.some((existing) => pointsEqual(existing, point))) {
    return;
  }
  points.push(point);
}

function buildVectorSubpaths(parsed: ReadonlyArray<PathCommand>): VectorSubpath[] {
  const subpaths: VectorSubpath[] = [];
  let currentSubpath: VectorSubpath | null = null;
  let last: Point2D | null = null;
  let rawLast = { x: 0, y: 0 };

  for (const cmd of parsed) {
    if (cmd.type === 'M') {
      const start = { x: cmd.x, y: cmd.y };
      currentSubpath = { points: [start], closed: false };
      subpaths.push(currentSubpath);
      last = start;
      rawLast = start;
      continue;
    }

    if (!currentSubpath || !last) {
      continue;
    }

    if (cmd.type === 'L') {
      const end = { x: cmd.x, y: cmd.y };
      pushDistinctPoint(currentSubpath.points, end);
      last = end;
      rawLast = end;
      continue;
    }

    if (cmd.type === 'Q') {
      const steps = 4;
      for (let i = 1; i <= steps; i++) {
        const t = i / steps;
        const point = {
          x: quadAt(rawLast.x, cmd.x1!, cmd.x, t),
          y: quadAt(rawLast.y, cmd.y1!, cmd.y, t),
        };
        pushDistinctPoint(currentSubpath.points, point);
        last = point;
      }
      rawLast = { x: cmd.x, y: cmd.y };
      continue;
    }

    if (cmd.type === 'C') {
      const steps = 6;
      for (let i = 1; i <= steps; i++) {
        const t = i / steps;
        const point = {
          x: cubicAt(rawLast.x, cmd.x1!, cmd.x2!, cmd.x, t),
          y: cubicAt(rawLast.y, cmd.y1!, cmd.y2!, cmd.y, t),
        };
        pushDistinctPoint(currentSubpath.points, point);
        last = point;
      }
      rawLast = { x: cmd.x, y: cmd.y };
      continue;
    }

    if (cmd.type === 'Z') {
      currentSubpath.closed = true;
      last = currentSubpath.points[0] ?? last;
      rawLast = last ?? rawLast;
    }
  }

  return subpaths;
}

function sampleSubpathVertices(
  points: ReadonlyArray<Point2D>,
  maxSamples: number,
  closed: boolean,
): Point2D[] {
  if (points.length === 0 || maxSamples <= 0) return [];
  if (points.length <= maxSamples) return [...points];

  const indices = new Set<number>();
  if (closed) {
    for (let i = 0; i < maxSamples; i++) {
      indices.add(Math.floor((i * points.length) / maxSamples));
    }
  } else if (maxSamples === 1) {
    indices.add(Math.floor((points.length - 1) / 2));
  } else {
    for (let i = 0; i < maxSamples; i++) {
      indices.add(Math.round((i * (points.length - 1)) / (maxSamples - 1)));
    }
  }

  return [...indices]
    .sort((a, b) => a - b)
    .map((index) => points[index])
    .filter((point): point is Point2D => point !== undefined);
}

function getDetailedVectorLocalSnapPoints(parsed: ReadonlyArray<PathCommand>): Point2D[] {
  const pts: Point2D[] = [];
  const segments: [Point2D, Point2D][] = [];
  const wrapPairs = new Set<string>();
  let last: Point2D | null = null;
  let rawLast = { x: 0, y: 0 };
  let firstPt: Point2D | null = null;
  let subpathFirstSegIdx = 0;

  for (const cmd of parsed) {
    if (cmd.type === 'M' || cmd.type === 'L' || cmd.type === 'Q' || cmd.type === 'C') {
      pts.push({ x: cmd.x, y: cmd.y });
    }
    if (cmd.type === 'M') {
      const pt = { x: cmd.x, y: cmd.y };
      last = pt;
      firstPt = pt;
      rawLast = { x: cmd.x, y: cmd.y };
      subpathFirstSegIdx = segments.length;
    } else if (cmd.type === 'L' && last) {
      const end = { x: cmd.x, y: cmd.y };
      segments.push([last, end]);
      last = end;
      rawLast = { x: cmd.x, y: cmd.y };
    } else if (cmd.type === 'Q' && last) {
      const steps = 8;
      let prev: Point2D = last;
      for (let i = 1; i <= steps; i++) {
        const ft = i / steps;
        const pt = {
          x: quadAt(rawLast.x, cmd.x1!, cmd.x, ft),
          y: quadAt(rawLast.y, cmd.y1!, cmd.y, ft),
        };
        segments.push([prev, pt]);
        if (i < steps) pts.push(pt);
        prev = pt;
      }
      last = prev;
      rawLast = { x: cmd.x, y: cmd.y };
    } else if (cmd.type === 'C' && last) {
      const steps = 12;
      let prev: Point2D = last;
      for (let i = 1; i <= steps; i++) {
        const ft = i / steps;
        const pt = {
          x: cubicAt(rawLast.x, cmd.x1!, cmd.x2!, cmd.x, ft),
          y: cubicAt(rawLast.y, cmd.y1!, cmd.y2!, cmd.y, ft),
        };
        segments.push([prev, pt]);
        if (i < steps) pts.push(pt);
        prev = pt;
      }
      last = prev;
      rawLast = { x: cmd.x, y: cmd.y };
    } else if (cmd.type === 'Z' && last && firstPt) {
      if (!pointsEqual(last, firstPt)) {
        segments.push([last, firstPt]);
      }
      const lastSegIdx = segments.length - 1;
      if (lastSegIdx >= subpathFirstSegIdx && lastSegIdx > subpathFirstSegIdx) {
        wrapPairs.add(`${subpathFirstSegIdx},${lastSegIdx}`);
      }
      last = firstPt;
    }
  }

  if (segments.length > DETAILED_VECTOR_SEGMENT_INTERSECTION_LIMIT) {
    return pts;
  }

  for (let i = 0; i < segments.length; i++) {
    for (let j = i + 2; j < segments.length; j++) {
      if (wrapPairs.has(`${i},${j}`)) continue;
      const ix = segmentIntersection(segments[i][0], segments[i][1], segments[j][0], segments[j][1]);
      if (ix) pts.push(ix);
    }
  }

  return pts;
}

function getHeavyVectorLocalSnapPoints(parsed: ReadonlyArray<PathCommand>): Point2D[] {
  const reduced: Point2D[] = [];

  for (const subpath of buildVectorSubpaths(parsed)) {
    if (subpath.points.length === 0) continue;

    if (!subpath.closed) {
      pushUniquePoint(reduced, subpath.points[0]);
      if (reduced.length >= HEAVY_VECTOR_SAMPLE_LIMIT) break;
      pushUniquePoint(reduced, subpath.points[subpath.points.length - 1]);
      if (reduced.length >= HEAVY_VECTOR_SAMPLE_LIMIT) break;
    }

    const remainingBudget = HEAVY_VECTOR_SAMPLE_LIMIT - reduced.length;
    if (remainingBudget <= 0) break;

    for (const point of sampleSubpathVertices(
      subpath.points,
      Math.min(HEAVY_VECTOR_SAMPLES_PER_SUBPATH, remainingBudget),
      subpath.closed,
    )) {
      pushUniquePoint(reduced, point);
      if (reduced.length >= HEAVY_VECTOR_SAMPLE_LIMIT) {
        break;
      }
    }

    if (reduced.length >= HEAVY_VECTOR_SAMPLE_LIMIT) {
      break;
    }
  }

  return reduced;
}

function getVectorLocalSnapPoints(parsed: ReadonlyArray<PathCommand>): Point2D[] {
  const cached = vectorLocalSnapPointsCache.get(parsed);
  if (cached) {
    return cached;
  }

  const pts = parsed.length > HEAVY_VECTOR_SNAP_COMMAND_THRESHOLD
    ? getHeavyVectorLocalSnapPoints(parsed)
    : getDetailedVectorLocalSnapPoints(parsed);
  vectorLocalSnapPointsCache.set(parsed, pts);
  return pts;
}

/** Compute intersection point of two line segments, or null if they don't intersect. */
function segmentIntersection(
  a1: Point2D, a2: Point2D, b1: Point2D, b2: Point2D,
): Point2D | null {
  const dx1 = a2.x - a1.x, dy1 = a2.y - a1.y;
  const dx2 = b2.x - b1.x, dy2 = b2.y - b1.y;
  const denom = dx1 * dy2 - dy1 * dx2;
  if (Math.abs(denom) < 1e-12) return null;
  const t = ((b1.x - a1.x) * dy2 - (b1.y - a1.y) * dx2) / denom;
  const u = ((b1.x - a1.x) * dy1 - (b1.y - a1.y) * dx1) / denom;
  if (t < 0 || t > 1 || u < 0 || u > 1) return null;
  return { x: a1.x + t * dx1, y: a1.y + t * dy1 };
}

// --- Point-to-point snapping ---

/**
 * True 2D point-to-point snapping — snaps to the nearest actual geometry point,
 * not to independent X/Y coordinates from different points.
 */
export function computePointSnap(
  draggedPoint: Point2D,
  otherObjects: ProjectObject[],
  thresholdMm: number,
  allObjects?: ProjectObject[],
): PointSnapResult {
  let bestDist = thresholdMm;
  let bestTarget: Point2D | null = null;

  for (const obj of otherObjects) {
    const snapPts = getObjectSnapPoints(obj, allObjects);
    for (const pt of snapPts) {
      const dx = draggedPoint.x - pt.x;
      const dy = draggedPoint.y - pt.y;
      const dist = Math.sqrt(dx * dx + dy * dy);
      if (dist < bestDist) {
        bestDist = dist;
        bestTarget = pt;
      }
    }
  }

  if (bestTarget) {
    const dx = bestTarget.x - draggedPoint.x;
    const dy = bestTarget.y - draggedPoint.y;
    return {
      dx,
      dy,
      guides: [
        { axis: 'x', value: bestTarget.x },
        { axis: 'y', value: bestTarget.y },
      ],
      snappedTo: bestTarget,
    };
  }

  return { dx: 0, dy: 0, guides: [] };
}

export interface SnapSegment {
  start: Point2D;
  end: Point2D;
  key: string;
  isGuide: boolean;
}

interface ResolveSnapOptions {
  preferredTargetKey?: string | null;
  preferredReleaseMultiplier?: number;
  excludedPoints?: Point2D[];
}

function snapClassRank(targetClass: GeometrySnapClass): number {
  switch (targetClass) {
    case 'point':
      return 0;
    case 'line':
      return 1;
    case 'bounds':
      return 2;
  }
}

function distance(a: Point2D, b: Point2D): number {
  return Math.hypot(a.x - b.x, a.y - b.y);
}

/**
 * Guide behavior is explicit metadata, not geometry inference.
 * A manually drawn axis-aligned line on T1 is still just a normal line unless
 * `ruler_guide_axis` is present on its vector-path data.
 */
export function getRulerGuideAxis(obj: ProjectObject): 'horizontal' | 'vertical' | null {
  if (obj.data.type !== 'vector_path') return null;
  return obj.data.ruler_guide_axis ?? null;
}

export function isRulerGuideObject(obj: ProjectObject): boolean {
  return getRulerGuideAxis(obj) !== null;
}

function projectPointToSegment(point: Point2D, start: Point2D, end: Point2D): Point2D {
  const dx = end.x - start.x;
  const dy = end.y - start.y;
  const lenSq = dx * dx + dy * dy;
  if (lenSq <= 1e-12) return start;
  const t = ((point.x - start.x) * dx + (point.y - start.y) * dy) / lenSq;
  const clamped = Math.max(0, Math.min(1, t));
  return {
    x: start.x + dx * clamped,
    y: start.y + dy * clamped,
  };
}

function flattenVectorPathSegments(resolved: ProjectObject): SnapSegment[] {
  if (resolved.data.type !== 'vector_path' || !resolved.data.path_data) return [];
  const info = getVectorPathRenderInfoForObject(resolved);
  if (!info || info.commands.length === 0) return [];
  const parsed = info.commands;
  const pathBbox = info.bbox;
  const bW = resolved.bounds.max.x - resolved.bounds.min.x;
  const bH = resolved.bounds.max.y - resolved.bounds.min.y;
  const bMinX = resolved.bounds.min.x;
  const bMinY = resolved.bounds.min.y;
  const bc = boundsCenter(resolved.bounds);
  const map = (px: number, py: number) =>
    applyAroundCenter(
      resolved.transform,
      mapPathCoordToBounds(px, py, pathBbox, bMinX, bMinY, bW, bH),
      bc,
    );
  const segments: SnapSegment[] = [];
  let last: Point2D | null = null;
  let rawLast = { x: 0, y: 0 };
  let firstPt: Point2D | null = null;
  let segmentIndex = 0;
  for (const cmd of parsed) {
    if (cmd.type === 'M') {
      last = map(cmd.x, cmd.y);
      firstPt = last;
      rawLast = { x: cmd.x, y: cmd.y };
      continue;
    }
    if (cmd.type === 'L' && last) {
      const end = map(cmd.x, cmd.y);
      segments.push({
        start: last,
        end,
        key: `${resolved.id}:seg:${segmentIndex++}`,
        isGuide: isRulerGuideObject(resolved),
      });
      last = end;
      rawLast = { x: cmd.x, y: cmd.y };
      continue;
    }
    if (cmd.type === 'Q' && last) {
      const steps = 8;
      let prev: Point2D = last;
      for (let i = 1; i <= steps; i++) {
        const ft = i / steps;
        const pt = map(
          quadAt(rawLast.x, cmd.x1!, cmd.x, ft),
          quadAt(rawLast.y, cmd.y1!, cmd.y, ft),
        );
        segments.push({
          start: prev,
          end: pt,
          key: `${resolved.id}:seg:${segmentIndex++}`,
          isGuide: isRulerGuideObject(resolved),
        });
        prev = pt;
      }
      last = prev;
      rawLast = { x: cmd.x, y: cmd.y };
      continue;
    }
    if (cmd.type === 'C' && last) {
      const steps = 12;
      let prev: Point2D = last;
      for (let i = 1; i <= steps; i++) {
        const ft = i / steps;
        const pt = map(
          cubicAt(rawLast.x, cmd.x1!, cmd.x2!, cmd.x, ft),
          cubicAt(rawLast.y, cmd.y1!, cmd.y2!, cmd.y, ft),
        );
        segments.push({
          start: prev,
          end: pt,
          key: `${resolved.id}:seg:${segmentIndex++}`,
          isGuide: isRulerGuideObject(resolved),
        });
        prev = pt;
      }
      last = prev;
      rawLast = { x: cmd.x, y: cmd.y };
      continue;
    }
    if (cmd.type === 'Z' && last && firstPt) {
      if (distance(last, firstPt) > 1e-6) {
        segments.push({
          start: last,
          end: firstPt,
          key: `${resolved.id}:seg:${segmentIndex++}`,
          isGuide: isRulerGuideObject(resolved),
        });
      }
      last = firstPt;
    }
  }
  return segments;
}

function buildObjectSnapSegmentsWorld(resolved: ProjectObject): SnapSegment[] {
  const bc = boundsCenter(resolved.bounds);
  const t = resolved.transform;

  switch (resolved.data.type) {
    case 'shape': {
      if (resolved.data.kind === 'ellipse') {
        const cx = (resolved.bounds.min.x + resolved.bounds.max.x) / 2;
        const cy = (resolved.bounds.min.y + resolved.bounds.max.y) / 2;
        const rx = (resolved.bounds.max.x - resolved.bounds.min.x) / 2;
        const ry = (resolved.bounds.max.y - resolved.bounds.min.y) / 2;
        const pts: Point2D[] = [];
        for (let i = 0; i < 24; i++) {
          const theta = (i / 24) * Math.PI * 2;
          pts.push(applyAroundCenter(t, { x: cx + rx * Math.cos(theta), y: cy + ry * Math.sin(theta) }, bc));
        }
        return pts.map((start, i) => ({
          start,
          end: pts[(i + 1) % pts.length],
          key: `${resolved.id}:seg:${i}`,
          isGuide: false,
        }));
      }
      const corners = boundsCorners(resolved.bounds).map((pt) => applyAroundCenter(t, pt, bc));
      return corners.map((start, i) => ({
        start,
        end: corners[(i + 1) % corners.length],
        key: `${resolved.id}:seg:${i}`,
        isGuide: false,
      }));
    }

    case 'text':
    case 'raster_image':
    case 'barcode': {
      const corners = boundsCorners(resolved.bounds).map((pt) => applyAroundCenter(t, pt, bc));
      return corners.map((start, i) => ({
        start,
        end: corners[(i + 1) % corners.length],
        key: `${resolved.id}:seg:${i}`,
        isGuide: false,
      }));
    }

    case 'polygon': {
      const localPts = buildPolygonPoints(resolved.data.sides, resolved.data.radius);
      const pts = mapLocalToBounds(localPts, resolved.bounds).map((pt) => applyAroundCenter(t, pt, bc));
      return pts.map((start, i) => ({
        start,
        end: pts[(i + 1) % pts.length],
        key: `${resolved.id}:seg:${i}`,
        isGuide: false,
      }));
    }

    case 'star': {
      const cmds = buildStarPath(
        resolved.data.points,
        resolved.data.bulge,
        resolved.data.ratio,
        resolved.data.dual_radius,
        resolved.data.ratio2 ?? undefined,
        resolved.data.corner_radius ?? 0,
        resolved.data.corner_radii,
      );
      const pts: Point2D[] = [];
      let current: Point2D | null = null;
      for (const cmd of cmds) {
        const end = { x: cmd.x, y: cmd.y };
        pts.push(end);
        if (cmd.type === 'quad' && current) {
          const ctrl = { x: cmd.qx, y: cmd.qy };
          const steps = 6;
          for (let i = 1; i <= steps; i++) {
            pts.push(quadraticPoint(current, ctrl, end, i / steps));
          }
        }
        current = end;
      }
      const mapped = mapLocalToBounds(pts, resolved.bounds).map((pt) => applyAroundCenter(t, pt, bc));
      return mapped.slice(0, -1).map((start, i) => ({
        start,
        end: mapped[i + 1],
        key: `${resolved.id}:seg:${i}`,
        isGuide: false,
      }));
    }

    case 'vector_path':
      return flattenVectorPathSegments(resolved);

    default:
      return [];
  }
}

export function getObjectSnapSegments(obj: ProjectObject, allObjects?: ProjectObject[]): SnapSegment[] {
  const resolved = allObjects ? resolveCloneForGeometry(obj, allObjects) : obj;
  const revision = getWorldSnapGeometryRevision(resolved);
  const cached = objectWorldSnapSegmentsCache.get(resolved);
  if (cached && matchesWorldSnapGeometryRevision(cached.revision, revision)) {
    return cached.segments;
  }

  const segments = buildObjectSnapSegmentsWorld(resolved);
  objectWorldSnapSegmentsCache.set(resolved, { revision, segments });
  return segments;
}

function buildGuideLinesForTarget(target: Point2D, targetClass: GeometrySnapClass): SnapLine[] {
  if (targetClass === 'line') {
    return [
      { axis: 'x', value: target.x },
      { axis: 'y', value: target.y },
    ];
  }
  return [
    { axis: 'x', value: target.x },
    { axis: 'y', value: target.y },
  ];
}

function chooseBetterSnap(
  current: GeometrySnapResult | null,
  candidate: GeometrySnapResult,
): GeometrySnapResult {
  if (!current) return candidate;
  const currentRank = snapClassRank(current.targetClass);
  const nextRank = snapClassRank(candidate.targetClass);
  if (nextRank !== currentRank) return nextRank < currentRank ? candidate : current;
  if (Math.abs(candidate.distance - current.distance) > 1e-9) {
    return candidate.distance < current.distance ? candidate : current;
  }
  if (candidate.targetKey !== current.targetKey) {
    return candidate.targetKey < current.targetKey ? candidate : current;
  }
  return current;
}

export function resolveGeometrySnap(
  draggedPoint: Point2D,
  otherObjects: ProjectObject[],
  thresholdMm: number,
  allObjects?: ProjectObject[],
  options: ResolveSnapOptions = {},
): GeometrySnapResult | null {
  let best: GeometrySnapResult | null = null;
  let preferred: GeometrySnapResult | null = null;
  const preferredThreshold = thresholdMm * (options.preferredReleaseMultiplier ?? 1.8);
  const excludedPoints = options.excludedPoints ?? [];

  for (const obj of otherObjects) {
    const snapPts = getObjectSnapPoints(obj, allObjects);
    for (let i = 0; i < snapPts.length; i++) {
      const pt = snapPts[i];
      if (excludedPoints.some((excluded) => distance(excluded, pt) <= 1e-6)) {
        continue;
      }
      const dist = distance(draggedPoint, pt);
      if (dist > thresholdMm) continue;
      const candidate: GeometrySnapResult = {
        dx: pt.x - draggedPoint.x,
        dy: pt.y - draggedPoint.y,
        guides: buildGuideLinesForTarget(pt, 'point'),
        snappedTo: pt,
        targetKey: `${obj.id}:pt:${i}`,
        targetClass: 'point',
        distance: dist,
      };
      best = chooseBetterSnap(best, candidate);
    }

    const segments = getObjectSnapSegments(obj, allObjects);
    for (const segment of segments) {
      const projected = projectPointToSegment(draggedPoint, segment.start, segment.end);
      const dist = distance(draggedPoint, projected);
      if (dist <= thresholdMm) {
        const candidate: GeometrySnapResult = {
          dx: projected.x - draggedPoint.x,
          dy: projected.y - draggedPoint.y,
          guides: buildGuideLinesForTarget(projected, 'line'),
          snappedTo: projected,
          targetKey: segment.key,
          targetClass: 'line',
          distance: dist,
        };
        best = chooseBetterSnap(best, candidate);
      }
      if (options.preferredTargetKey && segment.key === options.preferredTargetKey && dist <= preferredThreshold) {
        preferred = {
          dx: projected.x - draggedPoint.x,
          dy: projected.y - draggedPoint.y,
          guides: buildGuideLinesForTarget(projected, 'line'),
          snappedTo: projected,
          targetKey: segment.key,
          targetClass: 'line',
          distance: dist,
        };
      }
    }

    if (options.preferredTargetKey) {
      for (let i = 0; i < snapPts.length; i++) {
        const pt = snapPts[i];
        const key = `${obj.id}:pt:${i}`;
        if (excludedPoints.some((excluded) => distance(excluded, pt) <= 1e-6)) {
          continue;
        }
        const dist = distance(draggedPoint, pt);
        if (key === options.preferredTargetKey && dist <= preferredThreshold) {
          preferred = {
            dx: pt.x - draggedPoint.x,
            dy: pt.y - draggedPoint.y,
            guides: buildGuideLinesForTarget(pt, 'point'),
            snappedTo: pt,
            targetKey: key,
            targetClass: 'point',
            distance: dist,
          };
        }
      }
    }
  }

  return preferred ?? best;
}

export function computeSelectionSnap(
  draggedBounds: Bounds,
  anchorPoints: Point2D[],
  otherObjects: ProjectObject[],
  thresholdMm: number,
  allObjects?: ProjectObject[],
  options: ResolveSnapOptions = {},
): GeometrySnapResult | null {
  let best: GeometrySnapResult | null = null;

  anchorPoints.forEach((anchor) => {
    const candidate = resolveGeometrySnap(anchor, otherObjects, thresholdMm, allObjects, options);
    if (!candidate) return;
    best = chooseBetterSnap(best, candidate);
  });

  const boundsCandidate = computeObjectSnap(draggedBounds, otherObjects, thresholdMm, allObjects);
  if (boundsCandidate.guides.length > 0) {
    const movement = Math.hypot(boundsCandidate.dx, boundsCandidate.dy);
    const snapPoint = {
      x: centerX(draggedBounds) + boundsCandidate.dx,
      y: centerY(draggedBounds) + boundsCandidate.dy,
    };
    best = chooseBetterSnap(best, {
      dx: boundsCandidate.dx,
      dy: boundsCandidate.dy,
      guides: boundsCandidate.guides,
      snappedTo: snapPoint,
      targetKey: 'bounds',
      targetClass: 'bounds',
      distance: movement,
    });
  }

  return best;
}
