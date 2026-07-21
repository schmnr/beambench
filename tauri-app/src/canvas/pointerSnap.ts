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
    const geometrySnapEnabled = Boolean(project) && (snapToObjects || altKey);
    if (!geometrySnapEnabled || !project) {
      return {
        snapped: gridSnapped,
        nextPreferredTargetKey: null,
      };
    }

    const activeLayerIds = new Set(
      project.layers.filter((layer) => layer.enabled && layer.visible).map((layer) => layer.id),
    );
    const snapPx = snapThresholdPx ?? SNAP_THRESHOLD_PX;
    const thresholdMm = screenToWorldDist(altKey ? snapPx * 1.5 : snapPx, zoom);
    const snapObjects = queryWorldBoundsCandidates(
      {
        min: { x: world.x - thresholdMm, y: world.y - thresholdMm },
        max: { x: world.x + thresholdMm, y: world.y + thresholdMm },
      },
      project.objects,
    ).filter((obj) => activeLayerIds.has(obj.layer_id));
    const geometrySnap = resolveGeometrySnap(world, snapObjects, thresholdMm, project.objects, {
      preferredTargetKey: altKey ? preferredTargetKey ?? null : null,
      preferredReleaseMultiplier: altKey ? 2.1 : 1.8,
      excludedPoints,
    });

    if (geometrySnap) {
      return {
        snapped: geometrySnap.snappedTo,
        nextPreferredTargetKey: geometrySnap.targetKey,
      };
    }

    return {
      snapped: gridSnapped,
      nextPreferredTargetKey: null,
    };
  });
}
