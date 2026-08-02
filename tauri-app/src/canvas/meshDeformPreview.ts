import type { Bounds, Point2D, ProjectObject } from '../types/project';
import {
  cubicAt,
  getVectorPathRenderInfo,
  getVectorPathRenderInfoForObject,
  mapPathCoordToBounds,
  quadAt,
  type PathBBox,
  type PathCommand,
} from './drawObjects';

// Full contour fidelity matters more than maximum frame rate for artwork in the
// normal SVG complexity range. Only truly extreme selections are sampled.
const MAX_PREVIEW_POINTS = 96_000;
export const THROTTLED_MESH_PREVIEW_FRAME_INTERVAL_MS = 1000 / 30;

/** Hysteresis keeps the preview from rapidly switching between 60 and 30 FPS. */
export function adaptiveMeshPreviewFrameInterval(
  currentIntervalMs: number,
  averageRenderDurationMs: number,
): number {
  if (averageRenderDurationMs > 18) return THROTTLED_MESH_PREVIEW_FRAME_INTERVAL_MS;
  if (averageRenderDurationMs < 12) return 0;
  return currentIntervalMs;
}

export interface MeshDeformPreviewPath {
  points: Point2D[];
  closed: boolean;
}

export interface MeshDeformPreviewObject {
  objectId: string;
  layerId: string;
  paths: MeshDeformPreviewPath[];
  /** Raster/barcode fallbacks remain outlines instead of pretending to be exact geometry. */
  outlineOnly?: boolean;
}

function applyObjectTransform(point: Point2D, object: ProjectObject): Point2D {
  const cx = (object.bounds.min.x + object.bounds.max.x) / 2;
  const cy = (object.bounds.min.y + object.bounds.max.y) / 2;
  const dx = point.x - cx;
  const dy = point.y - cy;
  return {
    x: cx + object.transform.a * dx + object.transform.c * dy + object.transform.tx,
    y: cy + object.transform.b * dx + object.transform.d * dy + object.transform.ty,
  };
}

function mapPathPoint(point: Point2D, object: ProjectObject, bbox: PathBBox): Point2D {
  const mapped = mapPathCoordToBounds(
    point.x,
    point.y,
    bbox,
    object.bounds.min.x,
    object.bounds.min.y,
    object.bounds.max.x - object.bounds.min.x,
    object.bounds.max.y - object.bounds.min.y,
  );
  return applyObjectTransform(mapped, object);
}

function flattenCommands(
  commands: PathCommand[],
  bbox: PathBBox,
  object: ProjectObject,
  commandStride: number,
  curveSteps: number,
): MeshDeformPreviewPath[] {
  const paths: MeshDeformPreviewPath[] = [];
  let points: Point2D[] = [];
  let current: Point2D | null = null;
  let start: Point2D | null = null;
  let pendingEndpoint: Point2D | null = null;
  let drawableIndex = 0;

  const map = (point: Point2D) => mapPathPoint(point, object, bbox);
  const push = (point: Point2D) => {
    const mapped = map(point);
    const previous = points[points.length - 1];
    if (!previous || previous.x !== mapped.x || previous.y !== mapped.y) points.push(mapped);
  };
  const flush = (closed: boolean) => {
    if (pendingEndpoint) push(pendingEndpoint);
    pendingEndpoint = null;
    if (points.length >= 2) paths.push({ points, closed });
    points = [];
  };

  for (const command of commands) {
    if (command.type === 'M') {
      flush(false);
      current = { x: command.x, y: command.y };
      start = current;
      push(current);
      continue;
    }
    if (command.type === 'Z') {
      if (start) pendingEndpoint = start;
      flush(true);
      current = start;
      start = null;
      continue;
    }

    const from = current;
    const to = { x: command.x, y: command.y };
    current = to;
    pendingEndpoint = to;
    const include = drawableIndex % commandStride === 0;
    drawableIndex += 1;
    if (!include || !from) continue;

    if (command.type === 'L') {
      push(to);
    } else if (command.type === 'Q') {
      for (let step = 1; step <= curveSteps; step++) {
        const t = step / curveSteps;
        push({
          x: quadAt(from.x, command.x1!, to.x, t),
          y: quadAt(from.y, command.y1!, to.y, t),
        });
      }
    } else {
      for (let step = 1; step <= curveSteps; step++) {
        const t = step / curveSteps;
        push({
          x: cubicAt(from.x, command.x1!, command.x2!, to.x, t),
          y: cubicAt(from.y, command.y1!, command.y2!, to.y, t),
        });
      }
    }
    pendingEndpoint = null;
  }
  flush(false);
  return paths;
}

function boundsOutline(object: ProjectObject): MeshDeformPreviewPath[] {
  const { min, max } = object.bounds;
  return [{
    closed: true,
    points: [
      applyObjectTransform(min, object),
      applyObjectTransform({ x: max.x, y: min.y }, object),
      applyObjectTransform(max, object),
      applyObjectTransform({ x: min.x, y: max.y }, object),
    ],
  }];
}

function renderInfoForObject(object: ProjectObject) {
  const vectorInfo = getVectorPathRenderInfoForObject(object);
  if (vectorInfo) return vectorInfo;
  if (object.data.type === 'text' && object.data.resolved_path_data) {
    return getVectorPathRenderInfo(object.data.resolved_path_data);
  }
  return null;
}

/**
 * Build a bounded-complexity representation once at drag start. Dense paths are
 * sampled so pointer movement never reparses or redraws the full source SVG.
 */
export function buildMeshDeformPreviewObjects(
  objects: ProjectObject[],
  objectIds: string[],
): MeshDeformPreviewObject[] {
  const selected = objectIds
    .map((id) => objects.find((object) => object.id === id))
    .filter((object): object is ProjectObject => object !== undefined);
  const infos = new Map(selected.map((object) => [object.id, renderInfoForObject(object)]));
  const commands = [...infos.values()].flatMap((info) => info?.commands ?? []);
  const drawableCount = commands.filter((command) => command.type !== 'M' && command.type !== 'Z').length;
  const curveCount = commands.filter((command) => command.type === 'Q' || command.type === 'C').length;
  const estimatedPointBudget = Math.max(1, MAX_PREVIEW_POINTS - selected.length * 2);
  const commandStride = Math.max(1, Math.ceil(drawableCount / estimatedPointBudget));
  const sampledDrawableCount = Math.max(1, Math.ceil(drawableCount / commandStride));
  const sampledCurveCount = Math.ceil(curveCount / commandStride);
  const desiredCurveSteps = drawableCount <= 2_000 ? 6 : drawableCount <= 8_000 ? 3 : 2;
  const extraCurveCapacity = Math.max(0, estimatedPointBudget - sampledDrawableCount);
  const curveSteps = Math.max(
    1,
    Math.min(
      desiredCurveSteps,
      1 + (sampledCurveCount > 0 ? Math.floor(extraCurveCapacity / sampledCurveCount) : 0),
    ),
  );

  return selected.map((object) => {
    const info = infos.get(object.id);
    if (info && info.commands.length > 0) {
      return {
        objectId: object.id,
        layerId: object.layer_id,
        paths: flattenCommands(info.commands, info.bbox, object, commandStride, curveSteps),
      };
    }
    return {
      objectId: object.id,
      layerId: object.layer_id,
      paths: boundsOutline(object),
      outlineOnly: true,
    };
  }).filter((preview) => preview.paths.length > 0);
}

export function mapMeshDeformPoint(
  point: Point2D,
  sourceBounds: Bounds,
  handles: Point2D[],
  gridSize: number,
  perspective: boolean,
): Point2D {
  const width = sourceBounds.max.x - sourceBounds.min.x;
  const height = sourceBounds.max.y - sourceBounds.min.y;
  const u = Math.max(0, Math.min(1, (point.x - sourceBounds.min.x) / width));
  const v = Math.max(0, Math.min(1, (point.y - sourceBounds.min.y) / height));

  if (perspective && gridSize === 2) {
    const [p00, p10, p01, p11] = handles;
    const dx1 = p10.x - p11.x;
    const dx2 = p01.x - p11.x;
    const dx3 = p00.x - p10.x + p11.x - p01.x;
    const dy1 = p10.y - p11.y;
    const dy2 = p01.y - p11.y;
    const dy3 = p00.y - p10.y + p11.y - p01.y;
    const det = dx1 * dy2 - dx2 * dy1;
    if (Math.abs(det) > 1e-12) {
      const g = (dx3 * dy2 - dx2 * dy3) / det;
      const h = (dx1 * dy3 - dx3 * dy1) / det;
      const a = p10.x - p00.x + g * p10.x;
      const b = p01.x - p00.x + h * p01.x;
      const d = p10.y - p00.y + g * p10.y;
      const e = p01.y - p00.y + h * p01.y;
      const denominator = g * u + h * v + 1;
      if (Math.abs(denominator) > 1e-12) {
        return {
          x: (a * u + b * v + p00.x) / denominator,
          y: (d * u + e * v + p00.y) / denominator,
        };
      }
    }
  }

  const lastCell = gridSize - 2;
  const scaledU = u * (gridSize - 1);
  const scaledV = v * (gridSize - 1);
  const col = Math.min(Math.floor(scaledU), lastCell);
  const row = Math.min(Math.floor(scaledV), lastCell);
  const fu = scaledU - col;
  const fv = scaledV - row;
  const p00 = handles[row * gridSize + col];
  const p10 = handles[row * gridSize + col + 1];
  const p01 = handles[(row + 1) * gridSize + col];
  const p11 = handles[(row + 1) * gridSize + col + 1];
  const top = { x: p00.x + (p10.x - p00.x) * fu, y: p00.y + (p10.y - p00.y) * fu };
  const bottom = { x: p01.x + (p11.x - p01.x) * fu, y: p01.y + (p11.y - p01.y) * fu };
  return {
    x: top.x + (bottom.x - top.x) * fv,
    y: top.y + (bottom.y - top.y) * fv,
  };
}
