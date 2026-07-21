import type { Point2D, Bounds } from '../types/project';
import { BASE_PX_PER_MM } from './constants';

export interface ViewportParams {
  /** World-space point at center of canvas (mm) */
  offset: Point2D;
  /** Zoom percentage (100 = 100%) */
  zoom: number;
  /** Canvas element width in CSS pixels */
  canvasWidth: number;
  /** Canvas element height in CSS pixels */
  canvasHeight: number;
  /** World Y-axis direction in screen space. Defaults to canvas-style Y-down. */
  yAxis?: 'down' | 'up';
}

/** Pixels-per-mm for a given zoom level */
export function pxPerMm(zoom: number): number {
  return (zoom / 100) * BASE_PX_PER_MM;
}

/** Convert a world-space point (mm) to screen-space pixels */
export function worldToScreen(pt: Point2D, vp: ViewportParams): Point2D {
  const scale = pxPerMm(vp.zoom);
  const y = vp.yAxis === 'up'
    ? (vp.offset.y - pt.y) * scale + vp.canvasHeight / 2
    : (pt.y - vp.offset.y) * scale + vp.canvasHeight / 2;
  return {
    x: (pt.x - vp.offset.x) * scale + vp.canvasWidth / 2,
    y,
  };
}

/** Convert a screen-space pixel position to world-space (mm) */
export function screenToWorld(pt: Point2D, vp: ViewportParams): Point2D {
  const scale = pxPerMm(vp.zoom);
  const y = vp.yAxis === 'up'
    ? vp.offset.y - (pt.y - vp.canvasHeight / 2) / scale
    : (pt.y - vp.canvasHeight / 2) / scale + vp.offset.y;
  return {
    x: (pt.x - vp.canvasWidth / 2) / scale + vp.offset.x,
    y,
  };
}

/** Convert a world-space distance (mm) to screen pixels */
export function worldToScreenDist(mm: number, zoom: number): number {
  return mm * pxPerMm(zoom);
}

/** Convert a screen-pixel distance to world-space (mm) */
export function screenToWorldDist(px: number, zoom: number): number {
  return px / pxPerMm(zoom);
}

/** Get the visible world-space bounds for the current viewport */
export function visibleBounds(vp: ViewportParams): Bounds {
  const topLeft = screenToWorld({ x: 0, y: 0 }, vp);
  const bottomRight = screenToWorld({ x: vp.canvasWidth, y: vp.canvasHeight }, vp);
  return {
    min: {
      x: Math.min(topLeft.x, bottomRight.x),
      y: Math.min(topLeft.y, bottomRight.y),
    },
    max: {
      x: Math.max(topLeft.x, bottomRight.x),
      y: Math.max(topLeft.y, bottomRight.y),
    },
  };
}

/** Convert world-space bounds to a normalized screen rectangle. */
export function worldBoundsToScreenRect(bounds: Bounds, vp: ViewportParams): {
  x: number;
  y: number;
  w: number;
  h: number;
} {
  const a = worldToScreen(bounds.min, vp);
  const b = worldToScreen(bounds.max, vp);
  return {
    x: Math.min(a.x, b.x),
    y: Math.min(a.y, b.y),
    w: Math.abs(b.x - a.x),
    h: Math.abs(b.y - a.y),
  };
}

/** Compute viewport offset to center a given world-space bounds in the canvas */
export function zoomToFitBounds(
  bounds: Bounds,
  canvasWidth: number,
  canvasHeight: number,
  padding: number = 40,
): { offset: Point2D; zoom: number } {
  const worldW = bounds.max.x - bounds.min.x;
  const worldH = bounds.max.y - bounds.min.y;

  if (worldW <= 0 || worldH <= 0) {
    return {
      offset: { x: (bounds.min.x + bounds.max.x) / 2, y: (bounds.min.y + bounds.max.y) / 2 },
      zoom: 100,
    };
  }

  const availW = canvasWidth - padding * 2;
  const availH = canvasHeight - padding * 2;

  const scaleX = availW / worldW;
  const scaleY = availH / worldH;
  const scale = Math.min(scaleX, scaleY);

  const zoom = (scale / BASE_PX_PER_MM) * 100;

  return {
    offset: {
      x: (bounds.min.x + bounds.max.x) / 2,
      y: (bounds.min.y + bounds.max.y) / 2,
    },
    zoom: Math.max(10, Math.min(800, zoom)),
  };
}

/** Snap a world-space value to the nearest grid line */
export function snapToGrid(value: number, gridSpacing: number): number {
  return Math.round(value / gridSpacing) * gridSpacing || 0;
}

/** Snap a world-space point to the nearest grid intersection */
export function snapPointToGrid(pt: Point2D, gridSpacing: number): Point2D {
  return {
    x: snapToGrid(pt.x, gridSpacing),
    y: snapToGrid(pt.y, gridSpacing),
  };
}
