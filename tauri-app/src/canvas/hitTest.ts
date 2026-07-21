import type { ProjectObject, Point2D, TransformLocks } from '../types/project';
import type { ViewportParams } from './ViewportTransform';
import type { HandleId } from '../types/canvas';
import { worldToScreen, screenToWorld, pxPerMm } from './ViewportTransform';
import { getSelectionHandles } from './drawSelection';
import { HANDLE_HIT_SIZE, ROTATION_ARC_RADIUS } from './constants';
import { parsePathData, mapPathCoordToBounds, buildPolygonPoints, buildStarPath, quadraticExtremumT, quadraticPoint, getVectorPathRenderInfoForObject } from './drawObjects';
import { applyInverseTransformToPoint, isIdentityTransform } from './textMeasure';
import { applyAroundCenter, computeTransformedBoundsWorld, computeVisualBoundsWorld, getObjectSnapPoints, getRulerGuideAxis, mapLocalToBounds, resolveCloneForGeometry } from './alignment';
import { queryPointCandidates, queryRectCandidates } from './sceneIndex';
import { measureCanvasPerf } from './canvasPerf';
import { useAppStore } from '../stores/appStore';

function clickTolerancePx(): number {
  return useAppStore.getState().settings?.click_tolerance_px ?? 5;
}

const RULER_GUIDE_HIT_TOLERANCE_PX = 10;

function rulerGuideHitTolerancePx(): number {
  return Math.max(clickTolerancePx(), RULER_GUIDE_HIT_TOLERANCE_PX);
}

/** Hit-test a screen-space point against objects (reverse z_index order, topmost first) */
export function hitTestPoint(
  screenPt: Point2D,
  objects: ProjectObject[],
  vp: ViewportParams,
  includeLocked = false,
): ProjectObject | null {
  return measureCanvasPerf('hit-test', () => {
    const sorted = queryPointCandidates(screenPt, objects, vp, rulerGuideHitTolerancePx());

    for (const obj of sorted) {
      if (!includeLocked && obj.locked) continue;
      if (isPointInObjectBounds(screenPt, obj, vp, objects)) {
        return obj;
      }
    }
    return null;
  });
}

/** Hit-test a screen-space point against all overlapping objects (topmost first) */
export function hitTestPointAll(
  screenPt: Point2D,
  objects: ProjectObject[],
  vp: ViewportParams,
  includeLocked = false,
): ProjectObject[] {
  const sorted = queryPointCandidates(screenPt, objects, vp, rulerGuideHitTolerancePx());
  const hits: ProjectObject[] = [];
  for (const obj of sorted) {
    if (!includeLocked && obj.locked) continue;
    if (isPointInObjectBounds(screenPt, obj, vp, objects)) {
      hits.push(obj);
    }
  }
  return hits;
}

/** Hit-test a screen-space rectangle against objects (for rubber-band selection) */
export function hitTestRect(
  screenRect: { min: Point2D; max: Point2D },
  objects: ProjectObject[],
  vp: ViewportParams,
  includeLocked = false,
): ProjectObject[] {
  const result: ProjectObject[] = [];

  for (const obj of queryRectCandidates(screenRect, objects, vp)) {
    if (!includeLocked && obj.locked) continue;

    const vb = computeVisualBoundsWorld(obj, objects);
    const objTL = worldToScreen(vb.min, vp);
    const objBR = worldToScreen(vb.max, vp);

    // Object is selected if its screen bounds intersect the rubber-band rect
    if (
      objTL.x < screenRect.max.x &&
      objBR.x > screenRect.min.x &&
      objTL.y < screenRect.max.y &&
      objBR.y > screenRect.min.y
    ) {
      result.push(obj);
    }
  }

  return result;
}

/** Hit-test a screen-space rectangle — only objects fully contained inside the rect */
export function hitTestRectContained(
  screenRect: { min: Point2D; max: Point2D },
  objects: ProjectObject[],
  vp: ViewportParams,
  includeLocked = false,
): ProjectObject[] {
  const result: ProjectObject[] = [];

  for (const obj of queryRectCandidates(screenRect, objects, vp)) {
    if (!includeLocked && obj.locked) continue;

    const vb = computeVisualBoundsWorld(obj, objects);
    const objTL = worldToScreen(vb.min, vp);
    const objBR = worldToScreen(vb.max, vp);

    if (
      objTL.x >= screenRect.min.x &&
      objBR.x <= screenRect.max.x &&
      objTL.y >= screenRect.min.y &&
      objBR.y <= screenRect.max.y
    ) {
      result.push(obj);
    }
  }

  return result;
}

/** Hit-test screen-space point against selection handles */
export function hitTestHandle(
  screenPt: Point2D,
  selectedObjects: ProjectObject[],
  vp: ViewportParams,
  transformLocks?: TransformLocks,
  allObjects?: ProjectObject[],
): HandleId | null {
  if (selectedObjects.length === 0) return null;

  const handles = getSelectionHandles(selectedObjects, vp, transformLocks, allObjects);
  const half = HANDLE_HIT_SIZE / 2;

  for (const handle of handles) {
    if (handle.id.startsWith('rotate_') && isRotationHandleHit(screenPt, handle)) {
      return handle.id;
    }
    if (
      screenPt.x >= handle.screenX - half &&
      screenPt.x <= handle.screenX + half &&
      screenPt.y >= handle.screenY - half &&
      screenPt.y <= handle.screenY + half
    ) {
      return handle.id;
    }
  }

  return null;
}

function isRotationHandleHit(screenPt: Point2D, handle: { id: HandleId; screenX: number; screenY: number }): boolean {
  const dx = screenPt.x - handle.screenX;
  const dy = screenPt.y - handle.screenY;

  // Preserve the previous center hit target for users who click inside the arc.
  const centerHalf = HANDLE_HIT_SIZE / 2;
  if (Math.abs(dx) <= centerHalf && Math.abs(dy) <= centerHalf) {
    return true;
  }

  const radius = Math.hypot(dx, dy);
  const radialTolerance = Math.max(centerHalf, 8);
  if (
    radius < ROTATION_ARC_RADIUS - radialTolerance ||
    radius > ROTATION_ARC_RADIUS + radialTolerance
  ) {
    return false;
  }

  const midAngle = rotationHandleMidAngle(handle.id);
  if (midAngle == null) return false;

  const halfSweep = (110 / 2) * Math.PI / 180;
  const angleTolerance = 0.35;
  const angle = Math.atan2(dy, dx);
  return angleDistance(angle, midAngle) <= halfSweep + angleTolerance;
}

function rotationHandleMidAngle(handleId: HandleId): number | null {
  switch (handleId) {
    case 'rotate_nw': return 5 * Math.PI / 4;
    case 'rotate_ne': return -Math.PI / 4;
    case 'rotate_se': return Math.PI / 4;
    case 'rotate_sw': return 3 * Math.PI / 4;
    default: return null;
  }
}

function angleDistance(a: number, b: number): number {
  const twoPi = Math.PI * 2;
  return Math.abs((((a - b) + Math.PI) % twoPi + twoPi) % twoPi - Math.PI);
}

/** Offscreen context for glyph-level hit testing (created once, reused). */
let hitCtx: CanvasRenderingContext2D | null = null;
function getHitCtx(): CanvasRenderingContext2D {
  if (!hitCtx) {
    const canvas = document.createElement('canvas');
    canvas.width = 1; canvas.height = 1;
    hitCtx = canvas.getContext('2d')!;
  }
  return hitCtx;
}

/**
 * Glyph-level refinement for text objects with resolved_path_data.
 * Builds a Path2D in untransformed screen space, then tests the screen point
 * (inverse-transformed if needed) against fill + thick stroke.
 * Returns true (fall back to bbox) when resolved_path_data is absent.
 */
export function isPointInTextGlyphs(
  screenPt: Point2D,
  obj: ProjectObject,
  vp: ViewportParams,
): boolean {
  if (obj.data.type !== 'text') return true;
  const pathData = obj.data.resolved_path_data;
  if (!pathData) return true; // no path data → fall back to bbox

  const commands = parsePathData(pathData);
  if (commands.length === 0) return true;

  // Build Path2D in untransformed screen coords (bounds-relative → screen)
  const localToScreen = (px: number, py: number) =>
    worldToScreen({ x: obj.bounds.min.x + px, y: obj.bounds.min.y + py }, vp);

  const path = new Path2D();
  for (const cmd of commands) {
    switch (cmd.type) {
      case 'M': { const pt = localToScreen(cmd.x, cmd.y); path.moveTo(pt.x, pt.y); break; }
      case 'L': { const pt = localToScreen(cmd.x, cmd.y); path.lineTo(pt.x, pt.y); break; }
      case 'Q': { const cp = localToScreen(cmd.x1!, cmd.y1!); const pt = localToScreen(cmd.x, cmd.y); path.quadraticCurveTo(cp.x, cp.y, pt.x, pt.y); break; }
      case 'C': { const cp1 = localToScreen(cmd.x1!, cmd.y1!); const cp2 = localToScreen(cmd.x2!, cmd.y2!); const pt = localToScreen(cmd.x, cmd.y); path.bezierCurveTo(cp1.x, cp1.y, cp2.x, cp2.y, pt.x, pt.y); break; }
      case 'Z': path.closePath(); break;
    }
  }

  // Determine the test point: for transformed text, inverse-transform screen→world→inverse→screen
  let testPt = screenPt;
  if (!isIdentityTransform(obj.transform)) {
    const worldPt = screenToWorld(screenPt, vp);
    const invWorldPt = applyInverseTransformToPoint(worldPt, obj.transform, obj.bounds);
    if (!invWorldPt) return true; // singular matrix → fall back to bbox
    testPt = worldToScreen(invWorldPt, vp);
  }

  const ctx = getHitCtx();
  ctx.lineWidth = Math.max(1, clickTolerancePx());
  return ctx.isPointInPath(path, testPt.x, testPt.y) ||
         ctx.isPointInStroke(path, testPt.x, testPt.y);
}

function isPointInObjectBounds(
  screenPt: Point2D,
  obj: ProjectObject,
  vp: ViewportParams,
  allObjects?: ProjectObject[],
): boolean {
  // Resolve virtual clones before choosing tolerance so ruler guides do not
  // get rejected by the coarse zero-width bounds check.
  const resolved = allObjects ? resolveCloneForGeometry(obj, allObjects) : obj;
  const guideAxis = getRulerGuideAxis(resolved);
  const coarseBounds = computeTransformedBoundsWorld(obj);
  const coarseTopLeft = worldToScreen(coarseBounds.min, vp);
  const coarseBottomRight = worldToScreen(coarseBounds.max, vp);
  const coarseTolerancePx = guideAxis ? rulerGuideHitTolerancePx() : clickTolerancePx();
  if (
    screenPt.x < coarseTopLeft.x - coarseTolerancePx ||
    screenPt.x > coarseBottomRight.x + coarseTolerancePx ||
    screenPt.y < coarseTopLeft.y - coarseTolerancePx ||
    screenPt.y > coarseBottomRight.y + coarseTolerancePx
  ) {
    return false;
  }

  if (guideAxis) {
    const vb = computeVisualBoundsWorld(resolved, allObjects);
    const tl = worldToScreen(vb.min, vp);
    const br = worldToScreen(vb.max, vp);
    const tolerancePx = rulerGuideHitTolerancePx();
    if (guideAxis === 'vertical') {
      const lineX = (tl.x + br.x) / 2;
      return (
        Math.abs(screenPt.x - lineX) <= tolerancePx &&
        screenPt.y >= Math.min(tl.y, br.y) - tolerancePx &&
        screenPt.y <= Math.max(tl.y, br.y) + tolerancePx
      );
    }
    const lineY = (tl.y + br.y) / 2;
    return (
      Math.abs(screenPt.y - lineY) <= tolerancePx &&
      screenPt.x >= Math.min(tl.x, br.x) - tolerancePx &&
      screenPt.x <= Math.max(tl.x, br.x) + tolerancePx
    );
  }

  const vb = computeVisualBoundsWorld(resolved, allObjects);
  const tl = worldToScreen(vb.min, vp);
  const br = worldToScreen(vb.max, vp);
  const geometryBoundsTolerancePx = isDegenerateVectorPath(resolved) ? clickTolerancePx() : 0;

  const inBounds = (
    screenPt.x >= tl.x - geometryBoundsTolerancePx &&
    screenPt.x <= br.x + geometryBoundsTolerancePx &&
    screenPt.y >= tl.y - geometryBoundsTolerancePx &&
    screenPt.y <= br.y + geometryBoundsTolerancePx
  );

  if (!inBounds) return false;

  // Glyph-level refinement for text with resolved path data
  if (resolved.data.type === 'text' && resolved.data.resolved_path_data) {
    return isPointInTextGlyphs(screenPt, resolved, vp);
  }

  // Geometry-level refinement for shapes with well-defined outlines
  if (resolved.data.type === 'shape' && resolved.data.kind === 'ellipse') {
    return isPointInEllipseGeometry(screenPt, resolved, vp);
  }
  if (resolved.data.type === 'polygon') {
    return isPointInPolygonGeometry(screenPt, resolved, vp);
  }
  if (resolved.data.type === 'star') {
    return isPointInStarGeometry(screenPt, resolved, vp);
  }
  if (resolved.data.type === 'vector_path' && resolved.data.path_data) {
    return isPointInVectorPathGeometry(screenPt, resolved, vp);
  }

  return true;
}

/** Whether Path2D is available (not in jsdom test environment). */
const hasPath2D = typeof Path2D !== 'undefined';

function isDegenerateVectorPath(obj: ProjectObject): boolean {
  if (obj.data.type !== 'vector_path') return false;
  return (
    Math.abs(obj.bounds.max.x - obj.bounds.min.x) <= 1e-9 ||
    Math.abs(obj.bounds.max.y - obj.bounds.min.y) <= 1e-9
  );
}

/**
 * For transformed objects, inverse-transform the screen point to untransformed space
 * so we can test against the untransformed shape Path2D.
 */
function getTestPoint(screenPt: Point2D, obj: ProjectObject, vp: ViewportParams): Point2D | null {
  if (isIdentityTransform(obj.transform)) return screenPt;
  const worldPt = screenToWorld(screenPt, vp);
  const invPt = applyInverseTransformToPoint(worldPt, obj.transform, obj.bounds);
  if (!invPt) return null; // singular matrix
  return worldToScreen(invPt, vp);
}

function isPointInEllipseGeometry(screenPt: Point2D, obj: ProjectObject, vp: ViewportParams): boolean {
  if (!hasPath2D) return true;
  const testPt = getTestPoint(screenPt, obj, vp);
  if (!testPt) return true; // singular → fall back to bbox

  const tl = worldToScreen(obj.bounds.min, vp);
  const br = worldToScreen(obj.bounds.max, vp);
  const cx = (tl.x + br.x) / 2;
  const cy = (tl.y + br.y) / 2;
  const rx = (br.x - tl.x) / 2;
  const ry = (br.y - tl.y) / 2;

  const path = new Path2D();
  path.ellipse(cx, cy, rx, ry, 0, 0, Math.PI * 2);

  const ctx = getHitCtx();
  ctx.lineWidth = 8;
  return ctx.isPointInPath(path, testPt.x, testPt.y) ||
         ctx.isPointInStroke(path, testPt.x, testPt.y);
}

function isPointInPolygonGeometry(screenPt: Point2D, obj: ProjectObject, vp: ViewportParams): boolean {
  if (!hasPath2D) return true;
  if (obj.data.type !== 'polygon') return true;
  const testPt = getTestPoint(screenPt, obj, vp);
  if (!testPt) return true;

  const localPts = buildPolygonPoints(obj.data.sides, obj.data.radius);
  const boundsPts = mapLocalToBounds(localPts, obj.bounds);

  const path = new Path2D();
  for (let i = 0; i < boundsPts.length; i++) {
    const s = worldToScreen(boundsPts[i], vp);
    if (i === 0) path.moveTo(s.x, s.y);
    else path.lineTo(s.x, s.y);
  }
  path.closePath();

  const ctx = getHitCtx();
  ctx.lineWidth = 8;
  return ctx.isPointInPath(path, testPt.x, testPt.y) ||
         ctx.isPointInStroke(path, testPt.x, testPt.y);
}

function isPointInStarGeometry(screenPt: Point2D, obj: ProjectObject, vp: ViewportParams): boolean {
  if (!hasPath2D) return true;
  if (obj.data.type !== 'star') return true;
  const testPt = getTestPoint(screenPt, obj, vp);
  if (!testPt) return true;

  const cmds = buildStarPath(
    obj.data.points,
    obj.data.bulge,
    obj.data.ratio,
    obj.data.dual_radius,
    obj.data.ratio2 ?? undefined,
    obj.data.corner_radius ?? 0,
    obj.data.corner_radii,
  );

  // Compute local AABB including quadratic extrema (matches drawPathShape)
  let lMinX = Infinity, lMinY = Infinity, lMaxX = -Infinity, lMaxY = -Infinity;
  let current: { x: number; y: number } | null = null;
  for (const cmd of cmds) {
    const end = { x: cmd.x, y: cmd.y };
    lMinX = Math.min(lMinX, end.x); lMinY = Math.min(lMinY, end.y);
    lMaxX = Math.max(lMaxX, end.x); lMaxY = Math.max(lMaxY, end.y);
    if (cmd.type === 'quad' && current) {
      const ctrl = { x: cmd.qx, y: cmd.qy };
      const etx = quadraticExtremumT(current.x, ctrl.x, end.x);
      if (etx !== null) { const p = quadraticPoint(current, ctrl, end, etx); lMinX = Math.min(lMinX, p.x); lMaxX = Math.max(lMaxX, p.x); }
      const ety = quadraticExtremumT(current.y, ctrl.y, end.y);
      if (ety !== null) { const p = quadraticPoint(current, ctrl, end, ety); lMinY = Math.min(lMinY, p.y); lMaxY = Math.max(lMaxY, p.y); }
    }
    current = end;
  }
  const pathW = lMaxX - lMinX, pathH = lMaxY - lMinY;
  const bMinX = obj.bounds.min.x, bMinY = obj.bounds.min.y;
  const bW = obj.bounds.max.x - bMinX, bH = obj.bounds.max.y - bMinY;
  const sx = pathW > 0 ? bW / pathW : 1, sy = pathH > 0 ? bH / pathH : 1;
  const mapTx = bMinX - lMinX * sx, mapTy = bMinY - lMinY * sy;
  const toScr = (lx: number, ly: number) => worldToScreen({ x: lx * sx + mapTx, y: ly * sy + mapTy }, vp);

  const path = new Path2D();
  for (const cmd of cmds) {
    const s = toScr(cmd.x, cmd.y);
    if (cmd.type === 'move') {
      path.moveTo(s.x, s.y);
    } else if (cmd.type === 'line') {
      path.lineTo(s.x, s.y);
    } else {
      const cs = toScr(cmd.qx, cmd.qy);
      path.quadraticCurveTo(cs.x, cs.y, s.x, s.y);
    }
  }
  path.closePath();

  const ctx = getHitCtx();
  ctx.lineWidth = 8;
  return ctx.isPointInPath(path, testPt.x, testPt.y) ||
         ctx.isPointInStroke(path, testPt.x, testPt.y);
}

function isPointInVectorPathGeometry(screenPt: Point2D, obj: ProjectObject, vp: ViewportParams): boolean {
  if (!hasPath2D) return true;
  if (obj.data.type !== 'vector_path' || !obj.data.path_data) return true;

  const info = getVectorPathRenderInfoForObject(obj);
  if (!info || info.commands.length === 0) return true;

  const pathBBox = info.bbox;
  const bMinX = obj.bounds.min.x, bMinY = obj.bounds.min.y;
  const bW = obj.bounds.max.x - bMinX, bH = obj.bounds.max.y - bMinY;

  // Preferred path: test directly against the cached local-coordinates Path2D.
  // This avoids rebuilding a screen-space path from every command on each hit
  // test, which is exactly what makes empty-click deselection stall on large
  // imported potrace SVGs.
  if (info.path2d && pathBBox.width > 0 && pathBBox.height > 0 && bW > 0 && bH > 0) {
    const worldPt = screenToWorld(screenPt, vp);
    const localWorldPt = isIdentityTransform(obj.transform)
      ? worldPt
      : applyInverseTransformToPoint(worldPt, obj.transform, obj.bounds);
    if (!localWorldPt) return true;

    const localTestPt = {
      x: pathBBox.minX + ((localWorldPt.x - bMinX) / bW) * pathBBox.width,
      y: pathBBox.minY + ((localWorldPt.y - bMinY) / bH) * pathBBox.height,
    };

    const scale = pxPerMm(vp.zoom);
    const localStrokeScaleX = Math.abs((bW / pathBBox.width) * scale);
    const localStrokeScaleY = Math.abs((bH / pathBBox.height) * scale);
    const ctx = getHitCtx();
    ctx.lineWidth = clickTolerancePx() / Math.max(Math.min(localStrokeScaleX, localStrokeScaleY), 1e-6);
    return (
      ctx.isPointInPath(info.path2d, localTestPt.x, localTestPt.y) ||
      ctx.isPointInStroke(info.path2d, localTestPt.x, localTestPt.y)
    );
  }

  const testPt = getTestPoint(screenPt, obj, vp);
  if (!testPt) return true;

  const mapPt = (px: number, py: number) => {
    const mapped = mapPathCoordToBounds(px, py, pathBBox, bMinX, bMinY, bW, bH);
    return worldToScreen(mapped, vp);
  };

  const path = new Path2D();
  for (const cmd of info.commands) {
    switch (cmd.type) {
      case 'M': { const pt = mapPt(cmd.x, cmd.y); path.moveTo(pt.x, pt.y); break; }
      case 'L': { const pt = mapPt(cmd.x, cmd.y); path.lineTo(pt.x, pt.y); break; }
      case 'Q': { const cp = mapPt(cmd.x1!, cmd.y1!); const pt = mapPt(cmd.x, cmd.y); path.quadraticCurveTo(cp.x, cp.y, pt.x, pt.y); break; }
      case 'C': { const cp1 = mapPt(cmd.x1!, cmd.y1!); const cp2 = mapPt(cmd.x2!, cmd.y2!); const pt = mapPt(cmd.x, cmd.y); path.bezierCurveTo(cp1.x, cp1.y, cp2.x, cp2.y, pt.x, pt.y); break; }
      case 'Z': path.closePath(); break;
    }
  }

  const ctx = getHitCtx();
  ctx.lineWidth = Math.max(8, clickTolerancePx() * 2);
  return (
    ctx.isPointInPath(path, testPt.x, testPt.y) ||
    ctx.isPointInStroke(path, testPt.x, testPt.y)
  );
}

/** Hit-test selection box edge for move-via-edge-drag */
export function hitTestSelectionEdge(
  screenPt: Point2D,
  selectedObjects: ProjectObject[],
  vp: ViewportParams,
  transformLocks?: TransformLocks,
  allObjects?: ProjectObject[],
): Point2D | null {
  if (selectedObjects.length === 0) return null;
  if (transformLocks?.move_enabled === false) return null;

  // Compute selection box from visual bounds
  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
  for (const obj of selectedObjects) {
    const vb = computeVisualBoundsWorld(obj, allObjects);
    const tl = worldToScreen(vb.min, vp);
    const br = worldToScreen(vb.max, vp);
    minX = Math.min(minX, tl.x); minY = Math.min(minY, tl.y);
    maxX = Math.max(maxX, br.x); maxY = Math.max(maxY, br.y);
  }

  const edgeThreshold = clickTolerancePx();
  const x = screenPt.x, y = screenPt.y;

  // Check if near any edge of the selection box (but inside or very close)
  const nearLeft = Math.abs(x - minX) <= edgeThreshold && y >= minY - edgeThreshold && y <= maxY + edgeThreshold;
  const nearRight = Math.abs(x - maxX) <= edgeThreshold && y >= minY - edgeThreshold && y <= maxY + edgeThreshold;
  const nearTop = Math.abs(y - minY) <= edgeThreshold && x >= minX - edgeThreshold && x <= maxX + edgeThreshold;
  const nearBottom = Math.abs(y - maxY) <= edgeThreshold && x >= minX - edgeThreshold && x <= maxX + edgeThreshold;

  if (nearLeft || nearRight || nearTop || nearBottom) {
    return screenToWorld(screenPt, vp);
  }
  return null;
}

/** Hit-test snap points on selected objects for snap-point drag */
export function hitTestSnapPoint(
  screenPt: Point2D,
  selectedObjects: ProjectObject[],
  vp: ViewportParams,
  transformLocks?: TransformLocks,
  allObjects?: ProjectObject[],
): { snapPoint: Point2D } | null {
  if (transformLocks?.move_enabled === false) return null;

  const half = HANDLE_HIT_SIZE / 2;
  let bestDist = Infinity;
  let bestPoint: Point2D | null = null;

  for (const obj of selectedObjects) {
    const snapPts = getSnapPointHitTargets(obj, allObjects);
    for (const pt of snapPts) {
      const screen = worldToScreen(pt, vp);
      const dx = screenPt.x - screen.x;
      const dy = screenPt.y - screen.y;
      const dist = Math.sqrt(dx * dx + dy * dy);
      if (dist <= half && dist < bestDist) {
        bestDist = dist;
        bestPoint = pt;
      }
    }
  }

  return bestPoint ? { snapPoint: bestPoint } : null;
}

function getSnapPointHitTargets(
  obj: ProjectObject,
  allObjects?: ProjectObject[],
): Point2D[] {
  const resolved = allObjects ? resolveCloneForGeometry(obj, allObjects) : obj;
  if (resolved.data.type !== 'vector_path' || !resolved.data.path_data) {
    return getObjectSnapPoints(obj, allObjects);
  }

  const info = getVectorPathRenderInfoForObject(resolved);
  if (!info || info.commands.length === 0) {
    return getObjectSnapPoints(obj, allObjects);
  }

  const pathBBox = info.bbox;
  const bMinX = resolved.bounds.min.x;
  const bMinY = resolved.bounds.min.y;
  const bW = resolved.bounds.max.x - bMinX;
  const bH = resolved.bounds.max.y - bMinY;
  const center = {
    x: (resolved.bounds.min.x + resolved.bounds.max.x) / 2,
    y: (resolved.bounds.min.y + resolved.bounds.max.y) / 2,
  };
  const map = (px: number, py: number) =>
    applyAroundCenter(
      resolved.transform,
      mapPathCoordToBounds(px, py, pathBBox, bMinX, bMinY, bW, bH),
      center,
    );

  const points: Point2D[] = [applyAroundCenter(resolved.transform, center, center)];
  for (const cmd of info.commands) {
    if (cmd.type === 'M' || cmd.type === 'L' || cmd.type === 'Q' || cmd.type === 'C') {
      points.push(map(cmd.x, cmd.y));
    }
  }

  return points;
}
