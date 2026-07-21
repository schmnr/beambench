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
