import type { ProjectObject, Bounds } from '../types/project';

export interface SnapCandidate {
  /** World coordinate of the snap line */
  x: number;
  y: number;
  type: 'edge' | 'center';
  axis: 'x' | 'y';
  sourceObjectId: string;
}

export interface SnapGuide {
  axis: 'x' | 'y';
  /** World coordinate of the guide line */
  value: number;
}

export interface SnapResult {
  snappedX: number;
  snappedY: number;
  guides: SnapGuide[];
}

/**
 * Generate snap candidates from visible objects' bounds, excluding specified IDs.
 *
 * For each object, generates candidates for:
 * - X axis: left edge (min.x), right edge (max.x), center ((min.x + max.x) / 2)
 * - Y axis: top edge (min.y), bottom edge (max.y), center ((min.y + max.y) / 2)
 */
export function getSnapCandidates(
  objects: ProjectObject[],
  excludeIds: string[],
): SnapCandidate[] {
  const candidates: SnapCandidate[] = [];
  const excludeSet = new Set(excludeIds);

  for (const obj of objects) {
    if (excludeSet.has(obj.id)) continue;
    if (!obj.visible) continue;

    const { min, max } = obj.bounds;
    const centerX = (min.x + max.x) / 2;
    const centerY = (min.y + max.y) / 2;

    // X-axis candidates (vertical guide lines)
    candidates.push({ x: min.x, y: 0, type: 'edge', axis: 'x', sourceObjectId: obj.id });
    candidates.push({ x: max.x, y: 0, type: 'edge', axis: 'x', sourceObjectId: obj.id });
    candidates.push({ x: centerX, y: 0, type: 'center', axis: 'x', sourceObjectId: obj.id });

    // Y-axis candidates (horizontal guide lines)
    candidates.push({ x: 0, y: min.y, type: 'edge', axis: 'y', sourceObjectId: obj.id });
    candidates.push({ x: 0, y: max.y, type: 'edge', axis: 'y', sourceObjectId: obj.id });
    candidates.push({ x: 0, y: centerY, type: 'center', axis: 'y', sourceObjectId: obj.id });
  }

  return candidates;
}

/**
 * Find the best snap for a dragged object's bounds edges and center.
 *
 * Tests edges and center of the moving bounds against all candidates.
 * X and Y axes are snapped independently so the object can snap on
 * one axis without affecting the other.
 *
 * @param movingBounds - Current bounds of the object(s) being moved
 * @param candidates - Snap candidates from other objects
 * @param thresholdWorld - Snap threshold in world units (mm)
 * @returns Snap result with adjusted position and guide lines to draw
 */
export function findBoundsSnap(
  movingBounds: Bounds,
  candidates: SnapCandidate[],
  thresholdWorld: number,
): SnapResult {
  const centerX = (movingBounds.min.x + movingBounds.max.x) / 2;
  const centerY = (movingBounds.min.y + movingBounds.max.y) / 2;

  // Test points for X axis: left edge, right edge, center
  const testXValues = [movingBounds.min.x, movingBounds.max.x, centerX];
  // Test points for Y axis: top edge, bottom edge, center
  const testYValues = [movingBounds.min.y, movingBounds.max.y, centerY];

  let bestDx: number | null = null;
  let bestDistX = Infinity;
  let bestGuideX: number | null = null;

  let bestDy: number | null = null;
  let bestDistY = Infinity;
  let bestGuideY: number | null = null;

  for (const candidate of candidates) {
    if (candidate.axis === 'x') {
      for (const testX of testXValues) {
        const dist = Math.abs(testX - candidate.x);
        if (dist < thresholdWorld && dist < bestDistX) {
          bestDistX = dist;
          bestDx = candidate.x - testX;
          bestGuideX = candidate.x;
        }
      }
    } else {
      for (const testY of testYValues) {
        const dist = Math.abs(testY - candidate.y);
        if (dist < thresholdWorld && dist < bestDistY) {
          bestDistY = dist;
          bestDy = candidate.y - testY;
          bestGuideY = candidate.y;
        }
      }
    }
  }

  const guides: SnapGuide[] = [];
  const snappedX = bestDx !== null ? centerX + bestDx : centerX;
  const snappedY = bestDy !== null ? centerY + bestDy : centerY;

  if (bestGuideX !== null) {
    guides.push({ axis: 'x', value: bestGuideX });
  }
  if (bestGuideY !== null) {
    guides.push({ axis: 'y', value: bestGuideY });
  }

  return {
    snappedX,
    snappedY,
    guides,
  };
}

/**
 * Compute the snap threshold in world units from a pixel threshold.
 * @param thresholdPx - Threshold in screen pixels (typically 5)
 * @param zoom - Current zoom percentage (100 = 100%)
 */
export function snapThresholdWorld(thresholdPx: number, zoom: number): number {
  return thresholdPx / ((zoom / 100) * 2.0); // 2.0 = BASE_PX_PER_MM
}
