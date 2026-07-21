import type { Point2D } from '../types/project';

/**
 * Transform a local-space raster point to world space using scan angle rotation.
 *
 * For 0 angles (fast path), returns { x: xVal, y: yMm } directly.
 * For non-zero angles, rotates (xVal, yMm) around scanOrigin.
 */
export function stripPointToWorld(
  yMm: number,
  xVal: number,
  scanAngleDeg: number,
  scanOrigin: Point2D,
): Point2D {
  if (Math.abs(scanAngleDeg) < 0.5 || Math.abs(scanAngleDeg - 360) < 0.5) {
    return { x: xVal, y: yMm }; // fast path
  }
  const rad = (scanAngleDeg * Math.PI) / 180;
  const cos = Math.cos(rad);
  const sin = Math.sin(rad);
  return {
    x: scanOrigin.x + xVal * cos - yMm * sin,
    y: scanOrigin.y + xVal * sin + yMm * cos,
  };
}

/** Check whether a scan angle is cardinal (0/90/180/270 ±0.5°). */
export function isCardinalAngle(scanAngleDeg: number | undefined): boolean {
  if (scanAngleDeg == null) return true;
  const norm = ((scanAngleDeg % 360) + 360) % 360;
  return norm < 0.5
    || Math.abs(norm - 90) < 0.5
    || Math.abs(norm - 180) < 0.5
    || Math.abs(norm - 270) < 0.5
    || Math.abs(norm - 360) < 0.5;
}

/**
 * Convert planner-native strip coordinates to world space.
 *
 * Orthogonal scanlines are already in world coordinates, except for the
 * vertical planner transpose (`y_mm = X`, `start/end = Y`).
 * Non-cardinal scanlines remain in local space and must be rotated around
 * `scanOrigin`.
 */
export function plannerStripPointToWorld(
  yMm: number,
  xVal: number,
  options: {
    scanAxis?: 'horizontal' | 'vertical';
    scanAngleDeg?: number;
    scanOrigin?: Point2D;
  } = {},
): Point2D {
  const { scanAxis, scanAngleDeg = 0, scanOrigin } = options;

  if (!isCardinalAngle(scanAngleDeg) && scanOrigin) {
    return stripPointToWorld(yMm, xVal, scanAngleDeg, scanOrigin);
  }

  if (scanAxis === 'vertical') {
    return { x: yMm, y: xVal };
  }

  return { x: xVal, y: yMm };
}

/**
 * Returns the unit vector in the scan direction for the given angle.
 */
export function scanVector(angleDeg: number): Point2D {
  const rad = (angleDeg * Math.PI) / 180;
  return { x: Math.cos(rad), y: Math.sin(rad) };
}
