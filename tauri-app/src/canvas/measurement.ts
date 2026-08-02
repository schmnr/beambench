import type { Bounds, Layer, Point2D, ProjectObject, Transform2D } from '../types/project';
import type { ViewportParams } from './ViewportTransform';
import { worldToScreen } from './ViewportTransform';
import {
  computeVisualBoundsWorld,
  getObjectSnapSegments,
  type SnapSegment,
} from './alignment';
import {
  buildStarPath,
  parsePathData,
} from './drawObjects';

export const MEASURE_SEGMENT_HIT_TOLERANCE_PX = 8;
export type MeasurementMode = 'linear' | 'angle' | 'radius' | 'gap';

export interface MeasurementSegmentMetrics {
  start: Point2D;
  end: Point2D;
  dxMm: number;
  dyMm: number;
  lengthMm: number;
  angleDeg: number;
  segmentIndex: number;
  t: number;
}

export interface MeasurementObjectMetrics {
  objectId: string;
  objectName: string;
  nodes: number | null;
  lines: number | null;
  curves: number | null;
  perimeterMm: number | null;
  widthMm: number;
  heightMm: number;
  center: Point2D;
  closed: boolean | null;
  areaMm2: number | null;
}

export interface MeasurementHoverResult {
  objectId: string;
  objectMetrics: MeasurementObjectMetrics;
  segment: MeasurementSegmentMetrics | null;
}

export interface MeasurementDragMetrics {
  start: Point2D;
  end: Point2D;
  dxMm: number;
  dyMm: number;
  lengthMm: number;
  angleDeg: number;
}

export interface MeasurementLinearResult extends MeasurementDragMetrics {
  kind: 'linear';
}

export interface MeasurementAngleResult {
  kind: 'angle';
  first: MeasurementSegmentMetrics;
  second: MeasurementSegmentMetrics;
  angleDeg: number;
  labelPoint: Point2D;
}

export interface MeasurementRadiusResult {
  kind: 'radius';
  objectId: string;
  objectName: string;
  center: Point2D;
  edgePoint: Point2D;
  radiusXmm: number;
  radiusYmm: number;
  diameterXmm: number;
  diameterYmm: number;
  circular: boolean;
}

export interface MeasurementGapResult extends MeasurementDragMetrics {
  kind: 'gap';
  firstObjectId: string;
  firstObjectName: string;
  secondObjectId: string;
  secondObjectName: string;
  horizontalGapMm: number;
  verticalGapMm: number;
}

export type MeasurementResult =
  | MeasurementLinearResult
  | MeasurementAngleResult
  | MeasurementRadiusResult
  | MeasurementGapResult;

export type MeasurementPending =
  | { kind: 'linear'; start: Point2D }
  | { kind: 'angle'; objectId: string; segment: MeasurementSegmentMetrics }
  | { kind: 'gap'; objectId: string; objectName: string };

function boundsWidth(bounds: Bounds): number {
  return bounds.max.x - bounds.min.x;
}

function boundsHeight(bounds: Bounds): number {
  return bounds.max.y - bounds.min.y;
}

function transformAreaScale(transform: Transform2D): number {
  return Math.abs(transform.a * transform.d - transform.b * transform.c);
}

export function distance(a: Point2D, b: Point2D): number {
  return Math.hypot(b.x - a.x, b.y - a.y);
}

export function angleDeg(start: Point2D, end: Point2D): number {
  return Math.atan2(end.y - start.y, end.x - start.x) * (180 / Math.PI);
}

export function buildDragMeasurement(start: Point2D, end: Point2D): MeasurementDragMetrics {
  const dxMm = end.x - start.x;
  const dyMm = end.y - start.y;
  return {
    start,
    end,
    dxMm,
    dyMm,
    lengthMm: Math.hypot(dxMm, dyMm),
    angleDeg: angleDeg(start, end),
  };
}

export function buildLinearMeasurement(start: Point2D, end: Point2D): MeasurementLinearResult {
  return { kind: 'linear', ...buildDragMeasurement(start, end) };
}

export function buildAngleMeasurement(
  first: MeasurementSegmentMetrics,
  second: MeasurementSegmentMetrics,
): MeasurementAngleResult {
  const firstAngle = Math.atan2(first.dyMm, first.dxMm);
  const secondAngle = Math.atan2(second.dyMm, second.dxMm);
  let difference = Math.abs((secondAngle - firstAngle) * (180 / Math.PI)) % 360;
  if (difference > 180) difference = 360 - difference;
  return {
    kind: 'angle',
    first,
    second,
    angleDeg: difference,
    labelPoint: {
      x: (first.start.x + first.end.x + second.start.x + second.end.x) / 4,
      y: (first.start.y + first.end.y + second.start.y + second.end.y) / 4,
    },
  };
}

export function buildRadiusMeasurement(object: ProjectObject): MeasurementRadiusResult | null {
  if (object.data.type !== 'shape' || object.data.kind !== 'ellipse') return null;
  const center = {
    x: (object.bounds.min.x + object.bounds.max.x) / 2,
    y: (object.bounds.min.y + object.bounds.max.y) / 2,
  };
  const localRadiusX = boundsWidth(object.bounds) / 2;
  const localRadiusY = boundsHeight(object.bounds) / 2;
  const radiusXmm = localRadiusX * Math.hypot(object.transform.a, object.transform.b);
  const radiusYmm = localRadiusY * Math.hypot(object.transform.c, object.transform.d);
  const worldCenter = {
    x: center.x + object.transform.tx,
    y: center.y + object.transform.ty,
  };
  const edgePoint = {
    x: worldCenter.x + object.transform.a * localRadiusX,
    y: worldCenter.y + object.transform.b * localRadiusX,
  };
  const scale = Math.max(radiusXmm, radiusYmm, 1);
  return {
    kind: 'radius',
    objectId: object.id,
    objectName: object.name,
    center: worldCenter,
    edgePoint,
    radiusXmm,
    radiusYmm,
    diameterXmm: radiusXmm * 2,
    diameterYmm: radiusYmm * 2,
    circular: Math.abs(radiusXmm - radiusYmm) <= scale * 1e-4,
  };
}

function segmentMetrics(segment: SnapSegment, segmentIndex: number, t: number): MeasurementSegmentMetrics {
  const dxMm = segment.end.x - segment.start.x;
  const dyMm = segment.end.y - segment.start.y;
  return {
    start: segment.start,
    end: segment.end,
    dxMm,
    dyMm,
    lengthMm: Math.hypot(dxMm, dyMm),
    angleDeg: angleDeg(segment.start, segment.end),
    segmentIndex,
    t,
  };
}

function projectPointToScreenSegment(
  screenPoint: Point2D,
  start: Point2D,
  end: Point2D,
): { distancePx: number; t: number } {
  const dx = end.x - start.x;
  const dy = end.y - start.y;
  const lenSq = dx * dx + dy * dy;
  if (lenSq <= 1e-12) {
    return { distancePx: distance(screenPoint, start), t: 0 };
  }
  const rawT = ((screenPoint.x - start.x) * dx + (screenPoint.y - start.y) * dy) / lenSq;
  const t = Math.max(0, Math.min(1, rawT));
  const projected = { x: start.x + dx * t, y: start.y + dy * t };
  return { distancePx: distance(screenPoint, projected), t };
}

export function nearestSegmentToScreenPoint(
  object: ProjectObject,
  screenPoint: Point2D,
  vp: ViewportParams,
  allObjects?: ProjectObject[],
  tolerancePx = MEASURE_SEGMENT_HIT_TOLERANCE_PX,
): MeasurementSegmentMetrics | null {
  const segments = getObjectSnapSegments(object, allObjects);
  let best: MeasurementSegmentMetrics | null = null;
  let bestDistance = Infinity;

  segments.forEach((segment, index) => {
    const startScreen = worldToScreen(segment.start, vp);
    const endScreen = worldToScreen(segment.end, vp);
    const projected = projectPointToScreenSegment(screenPoint, startScreen, endScreen);
    if (projected.distancePx <= tolerancePx && projected.distancePx < bestDistance) {
      bestDistance = projected.distancePx;
      best = segmentMetrics(segment, index, projected.t);
    }
  });

  return best;
}

function perimeterFromSegments(segments: SnapSegment[]): number | null {
  if (segments.length === 0) return null;
  return segments.reduce((sum, segment) => sum + distance(segment.start, segment.end), 0);
}

function areaFromSegments(segments: SnapSegment[]): number | null {
  if (segments.length < 3) return null;
  const twiceArea = segments.reduce(
    (sum, segment) => sum + segment.start.x * segment.end.y - segment.end.x * segment.start.y,
    0,
  );
  const area = Math.abs(twiceArea) / 2;
  return area > 1e-9 ? area : null;
}

function vectorCounts(object: ProjectObject): Pick<MeasurementObjectMetrics, 'nodes' | 'lines' | 'curves'> {
  if (object.data.type !== 'vector_path') return { nodes: null, lines: null, curves: null };
  const commands = parsePathData(object.data.path_data);
  let nodes = 0;
  let lines = 0;
  let curves = 0;
  for (const command of commands) {
    if (command.type === 'M') nodes++;
    if (command.type === 'L') {
      nodes++;
      lines++;
    }
    if (command.type === 'Q' || command.type === 'C') {
      nodes++;
      curves++;
    }
    if (command.type === 'Z') {
      lines++;
    }
  }
  return { nodes, lines, curves };
}

function starCounts(object: ProjectObject): Pick<MeasurementObjectMetrics, 'nodes' | 'lines' | 'curves'> {
  if (object.data.type !== 'star') return { nodes: null, lines: null, curves: null };
  const commands = buildStarPath(
    object.data.points,
    object.data.bulge,
    object.data.ratio,
    object.data.dual_radius,
    object.data.ratio2 ?? undefined,
    object.data.corner_radius ?? 0,
    object.data.corner_radii,
  );
  const lines = commands.filter((command) => command.type === 'line').length;
  const curves = commands.filter((command) => command.type === 'quad').length;
  const nodes = Math.max(0, commands.length);
  return { nodes, lines: curves > 0 ? lines : Math.max(0, nodes), curves };
}

function closedState(object: ProjectObject): boolean | null {
  switch (object.data.type) {
    case 'shape':
    case 'polygon':
    case 'star':
      return true;
    case 'vector_path':
      return object.data.closed;
    default:
      return null;
  }
}

function primitiveAreaMm2(object: ProjectObject, segments: SnapSegment[]): number | null {
  switch (object.data.type) {
    case 'shape': {
      const w = boundsWidth(object.bounds);
      const h = boundsHeight(object.bounds);
      if (object.data.kind === 'ellipse') {
        return Math.PI * (w / 2) * (h / 2) * transformAreaScale(object.transform);
      }
      return w * h * transformAreaScale(object.transform);
    }
    case 'polygon':
    case 'star':
      return areaFromSegments(segments);
    case 'vector_path':
      return object.data.closed ? areaFromSegments(segments) : null;
    default:
      return null;
  }
}

function countsForObject(object: ProjectObject): Pick<MeasurementObjectMetrics, 'nodes' | 'lines' | 'curves'> {
  switch (object.data.type) {
    case 'shape':
      return object.data.kind === 'ellipse'
        ? { nodes: 4, lines: 0, curves: 4 }
        : { nodes: 4, lines: 4, curves: 0 };
    case 'polygon':
      return { nodes: object.data.sides, lines: object.data.sides, curves: 0 };
    case 'star':
      return starCounts(object);
    case 'vector_path':
      return vectorCounts(object);
    case 'text':
    case 'raster_image':
    case 'barcode':
      return { nodes: 4, lines: 4, curves: 0 };
    default:
      return { nodes: null, lines: null, curves: null };
  }
}

export function buildObjectMeasurementMetrics(
  object: ProjectObject,
  allObjects?: ProjectObject[],
): MeasurementObjectMetrics {
  const visualBounds = computeVisualBoundsWorld(object, allObjects);
  const segments = getObjectSnapSegments(object, allObjects);
  const counts = countsForObject(object);
  return {
    objectId: object.id,
    objectName: object.name,
    ...counts,
    perimeterMm: perimeterFromSegments(segments),
    widthMm: boundsWidth(visualBounds),
    heightMm: boundsHeight(visualBounds),
    center: {
      x: (visualBounds.min.x + visualBounds.max.x) / 2,
      y: (visualBounds.min.y + visualBounds.max.y) / 2,
    },
    closed: closedState(object),
    areaMm2: primitiveAreaMm2(object, segments),
  };
}

function boundsSegments(bounds: Bounds): SnapSegment[] {
  const points = [
    { x: bounds.min.x, y: bounds.min.y },
    { x: bounds.max.x, y: bounds.min.y },
    { x: bounds.max.x, y: bounds.max.y },
    { x: bounds.min.x, y: bounds.max.y },
  ];
  return points.map((start, index) => ({
    start,
    end: points[(index + 1) % points.length],
    key: `bounds:${index}`,
    isGuide: false,
  }));
}

function sampledSegments(segments: SnapSegment[], maxSegments = 700): SnapSegment[] {
  if (segments.length <= maxSegments) return segments;
  const step = segments.length / maxSegments;
  return Array.from({ length: maxSegments }, (_, index) => segments[Math.floor(index * step)]);
}

function closestPointOnWorldSegment(point: Point2D, start: Point2D, end: Point2D): Point2D {
  const dx = end.x - start.x;
  const dy = end.y - start.y;
  const lengthSquared = dx * dx + dy * dy;
  if (lengthSquared <= 1e-12) return start;
  const t = Math.max(0, Math.min(1, ((point.x - start.x) * dx + (point.y - start.y) * dy) / lengthSquared));
  return { x: start.x + dx * t, y: start.y + dy * t };
}

function worldSegmentIntersection(
  first: SnapSegment,
  second: SnapSegment,
): Point2D | null {
  const firstDx = first.end.x - first.start.x;
  const firstDy = first.end.y - first.start.y;
  const secondDx = second.end.x - second.start.x;
  const secondDy = second.end.y - second.start.y;
  const denominator = firstDx * secondDy - firstDy * secondDx;
  if (Math.abs(denominator) <= 1e-12) return null;
  const offsetX = second.start.x - first.start.x;
  const offsetY = second.start.y - first.start.y;
  const firstT = (offsetX * secondDy - offsetY * secondDx) / denominator;
  const secondT = (offsetX * firstDy - offsetY * firstDx) / denominator;
  if (firstT < 0 || firstT > 1 || secondT < 0 || secondT > 1) return null;
  return {
    x: first.start.x + firstT * firstDx,
    y: first.start.y + firstT * firstDy,
  };
}

function axisGap(firstMin: number, firstMax: number, secondMin: number, secondMax: number): number {
  if (firstMax < secondMin) return secondMin - firstMax;
  if (secondMax < firstMin) return firstMin - secondMax;
  return 0;
}

export function buildGapMeasurement(
  first: ProjectObject,
  second: ProjectObject,
  allObjects?: ProjectObject[],
): MeasurementGapResult | null {
  if (first.id === second.id) return null;
  const firstBounds = computeVisualBoundsWorld(first, allObjects);
  const secondBounds = computeVisualBoundsWorld(second, allObjects);
  const firstCenter = {
    x: (firstBounds.min.x + firstBounds.max.x) / 2,
    y: (firstBounds.min.y + firstBounds.max.y) / 2,
  };
  const secondCenter = {
    x: (secondBounds.min.x + secondBounds.max.x) / 2,
    y: (secondBounds.min.y + secondBounds.max.y) / 2,
  };
  const firstGeometrySegments = getObjectSnapSegments(first, allObjects);
  const secondGeometrySegments = getObjectSnapSegments(second, allObjects);
  const firstSegments = sampledSegments(
    firstGeometrySegments.length > 0 ? firstGeometrySegments : boundsSegments(firstBounds),
  );
  const secondSegments = sampledSegments(
    secondGeometrySegments.length > 0 ? secondGeometrySegments : boundsSegments(secondBounds),
  );

  let bestStart = firstCenter;
  let bestEnd = secondCenter;
  let bestDistance = distance(bestStart, bestEnd);
  const consider = (start: Point2D, end: Point2D) => {
    const candidateDistance = distance(start, end);
    if (candidateDistance < bestDistance) {
      bestDistance = candidateDistance;
      bestStart = start;
      bestEnd = end;
    }
  };

  for (const firstSegment of firstSegments) {
    for (const secondSegment of secondSegments) {
      const intersection = worldSegmentIntersection(firstSegment, secondSegment);
      if (intersection) {
        bestStart = intersection;
        bestEnd = intersection;
        bestDistance = 0;
        break;
      }
      consider(firstSegment.start, closestPointOnWorldSegment(firstSegment.start, secondSegment.start, secondSegment.end));
      consider(firstSegment.end, closestPointOnWorldSegment(firstSegment.end, secondSegment.start, secondSegment.end));
      consider(closestPointOnWorldSegment(secondSegment.start, firstSegment.start, firstSegment.end), secondSegment.start);
      consider(closestPointOnWorldSegment(secondSegment.end, firstSegment.start, firstSegment.end), secondSegment.end);
    }
    if (bestDistance === 0) break;
  }

  return {
    kind: 'gap',
    firstObjectId: first.id,
    firstObjectName: first.name,
    secondObjectId: second.id,
    secondObjectName: second.name,
    ...buildDragMeasurement(bestStart, bestEnd),
    horizontalGapMm: axisGap(firstBounds.min.x, firstBounds.max.x, secondBounds.min.x, secondBounds.max.x),
    verticalGapMm: axisGap(firstBounds.min.y, firstBounds.max.y, secondBounds.min.y, secondBounds.max.y),
  };
}

export function visibleMeasurementObjects(
  objects: ProjectObject[],
  layers: Array<Pick<Layer, 'id'> & { visible?: boolean }>,
): ProjectObject[] {
  const visibleLayerIds = new Set(
    layers
      .filter((layer) => layer.visible !== false)
      .map((layer) => layer.id),
  );
  return objects.filter((object) => object.visible && visibleLayerIds.has(object.layer_id));
}

export function buildHoverMeasurement(
  object: ProjectObject,
  screenPoint: Point2D,
  vp: ViewportParams,
  allObjects?: ProjectObject[],
): MeasurementHoverResult {
  return {
    objectId: object.id,
    objectMetrics: buildObjectMeasurementMetrics(object, allObjects),
    segment: nearestSegmentToScreenPoint(object, screenPoint, vp, allObjects),
  };
}
