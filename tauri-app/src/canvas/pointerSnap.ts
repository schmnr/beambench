import { resolveGeometrySnap } from './alignment';
import { screenToWorldDist, snapPointToGrid } from './ViewportTransform';
import { SNAP_THRESHOLD_PX } from './constants';
import type { Project } from '../types/project';
import { queryWorldBoundsCandidates } from './sceneIndex';
import { measureCanvasPerf } from './canvasPerf';

type Point = { x: number; y: number };

export type ResolveCanvasPointerSnapArgs = {
  world: Point;
  ctrlKey: boolean;
  altKey: boolean;
  project: Project | null;
  zoom: number;
  snapEnabled: boolean;
  gridVisible: boolean;
  effectiveSnapSpacing: number;
  snapToObjects: boolean;
  snapThresholdPx?: number | null;
  preferredTargetKey?: string | null;
  excludedPoints?: Point[];
  snapToWorkspace?: boolean;
};

export type ResolveCanvasPointerSnapResult = {
  snapped: Point;
  nextPreferredTargetKey: string | null;
};

export function resolveCanvasPointerSnap({
  world,
  ctrlKey,
  altKey,
  project,
  zoom,
  snapEnabled,
  gridVisible,
  effectiveSnapSpacing,
  snapToObjects,
  snapThresholdPx,
  preferredTargetKey,
  excludedPoints,
  snapToWorkspace = false,
}: ResolveCanvasPointerSnapArgs): ResolveCanvasPointerSnapResult {
  return measureCanvasPerf('snap-resolution', () => {
    if (ctrlKey) {
      return {
        snapped: world,
        nextPreferredTargetKey: null,
      };
    }

    const gridSnapped =
      snapEnabled && gridVisible ? snapPointToGrid(world, effectiveSnapSpacing) : world;
    if (!project) {
      return {
        snapped: gridSnapped,
        nextPreferredTargetKey: null,
      };
    }

    const geometrySnapEnabled = snapToObjects || altKey;
    const activeLayerIds = new Set(
      project.layers.filter((layer) => layer.enabled && layer.visible).map((layer) => layer.id),
    );
    const snapPx = snapThresholdPx ?? SNAP_THRESHOLD_PX;
    const thresholdMm = screenToWorldDist(altKey ? snapPx * 1.5 : snapPx, zoom);
    const snapObjects = geometrySnapEnabled
      ? queryWorldBoundsCandidates(
          {
            min: { x: world.x - thresholdMm, y: world.y - thresholdMm },
            max: { x: world.x + thresholdMm, y: world.y + thresholdMm },
          },
          project.objects,
        ).filter((obj) => obj.visible && activeLayerIds.has(obj.layer_id))
      : [];
    const geometrySnap = geometrySnapEnabled
      ? resolveGeometrySnap(world, snapObjects, thresholdMm, project.objects, {
          preferredTargetKey: altKey ? preferredTargetKey ?? null : null,
          preferredReleaseMultiplier: altKey ? 2.1 : 1.8,
          excludedPoints,
        })
      : null;

    if (geometrySnap) {
      return {
        snapped: geometrySnap.snappedTo,
        nextPreferredTargetKey: geometrySnap.targetKey,
      };
    }

    if (snapToWorkspace) {
      const width = project.workspace.bed_width_mm;
      const height = project.workspace.bed_height_mm;
      const candidates: Array<{ point: Point; key: string }> = [
        { point: { x: 0, y: 0 }, key: 'workspace:top-left' },
        { point: { x: width, y: 0 }, key: 'workspace:top-right' },
        { point: { x: width, y: height }, key: 'workspace:bottom-right' },
        { point: { x: 0, y: height }, key: 'workspace:bottom-left' },
        { point: { x: width / 2, y: height / 2 }, key: 'workspace:center' },
        { point: { x: Math.max(0, Math.min(width, world.x)), y: 0 }, key: 'workspace:top' },
        { point: { x: Math.max(0, Math.min(width, world.x)), y: height }, key: 'workspace:bottom' },
        { point: { x: 0, y: Math.max(0, Math.min(height, world.y)) }, key: 'workspace:left' },
        { point: { x: width, y: Math.max(0, Math.min(height, world.y)) }, key: 'workspace:right' },
      ];
      let best: { point: Point; key: string; distance: number } | null = null;
      for (const candidate of candidates) {
        const candidateDistance = Math.hypot(candidate.point.x - world.x, candidate.point.y - world.y);
        if (candidateDistance <= thresholdMm && (!best || candidateDistance < best.distance)) {
          best = { ...candidate, distance: candidateDistance };
        }
      }
      if (best) {
        return {
          snapped: best.point,
          nextPreferredTargetKey: best.key,
        };
      }
    }

    return {
      snapped: gridSnapped,
      nextPreferredTargetKey: null,
    };
  });
}
