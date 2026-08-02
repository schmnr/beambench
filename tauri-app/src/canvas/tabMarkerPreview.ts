import type { Bounds, Transform2D } from '../types/project';
import { applyAroundCenter } from './alignment';

export interface PersistentTabMarkerSnapshot {
  worldX: number;
  worldY: number;
  objectId: string;
  resolvedBounds: Bounds;
  resolvedTransform: Transform2D;
}

const EPSILON = 1e-9;

function centerOf(bounds: Bounds) {
  return {
    x: (bounds.min.x + bounds.max.x) / 2,
    y: (bounds.min.y + bounds.max.y) / 2,
  };
}

function inverseAroundCenter(
  transform: Transform2D,
  point: { x: number; y: number },
  center: { x: number; y: number },
) {
  const determinant = transform.a * transform.d - transform.b * transform.c;
  if (Math.abs(determinant) < EPSILON) return null;
  const x = point.x - center.x - transform.tx;
  const y = point.y - center.y - transform.ty;
  return {
    x: center.x + (transform.d * x - transform.c * y) / determinant,
    y: center.y + (-transform.b * x + transform.a * y) / determinant,
  };
}

/**
 * Keeps a native-resolved tab marker attached to the browser's transient
 * bounds/transform preview. Native marker resolution catches up after commit;
 * this mapping covers every frame in between without a backend round-trip.
 */
export function remapPersistentTabMarker(
  marker: PersistentTabMarkerSnapshot,
  currentBounds: Bounds,
  currentTransform: Transform2D,
) {
  const sourceBounds = marker.resolvedBounds;
  const sourceCenter = centerOf(sourceBounds);
  const sourcePoint = inverseAroundCenter(
    marker.resolvedTransform,
    { x: marker.worldX, y: marker.worldY },
    sourceCenter,
  );

  if (!sourcePoint) {
    return {
      worldX: marker.worldX + currentBounds.min.x - sourceBounds.min.x,
      worldY: marker.worldY + currentBounds.min.y - sourceBounds.min.y,
    };
  }

  const sourceWidth = sourceBounds.max.x - sourceBounds.min.x;
  const sourceHeight = sourceBounds.max.y - sourceBounds.min.y;
  const fx = Math.abs(sourceWidth) > EPSILON
    ? (sourcePoint.x - sourceBounds.min.x) / sourceWidth
    : 0.5;
  const fy = Math.abs(sourceHeight) > EPSILON
    ? (sourcePoint.y - sourceBounds.min.y) / sourceHeight
    : 0.5;
  const mappedPoint = {
    x: currentBounds.min.x + fx * (currentBounds.max.x - currentBounds.min.x),
    y: currentBounds.min.y + fy * (currentBounds.max.y - currentBounds.min.y),
  };
  const currentPoint = applyAroundCenter(currentTransform, mappedPoint, centerOf(currentBounds));
  return { worldX: currentPoint.x, worldY: currentPoint.y };
}
