import type { SimilarityTransform } from '../types/camera';
import type { Point2D } from '../types/project';
import { pxPerMm, worldToScreen, type ViewportParams } from './ViewportTransform';

export const CAMERA_OVERLAY_HANDLE_SIZE_PX = 8;
export const CAMERA_OVERLAY_HANDLE_HIT_TOLERANCE_PX = 8;
export const CAMERA_OVERLAY_ROTATE_HANDLE_OFFSET_PX = 20;
export const CAMERA_OVERLAY_MIN_SCALE = 0.01;
export const CAMERA_OVERLAY_MAX_SCALE = 20;

export type CameraOverlayScaleHandle =
  | 'top_left'
  | 'top_right'
  | 'bottom_right'
  | 'bottom_left';

export type CameraOverlayHitTarget =
  | { type: 'move' }
  | { type: 'scale'; handle: CameraOverlayScaleHandle }
  | { type: 'rotate' };

export interface CameraOverlayHandleGeometry {
  corners: Array<{ id: CameraOverlayScaleHandle; screen: Point2D }>;
  rotation: Point2D;
  center: Point2D;
}

export interface CameraOverlayRenderParams {
  frameHandleId: string;
  widthPx: number;
  heightPx: number;
  transform: SimilarityTransform;
  opacity: number;
}

export interface CameraOverlayCanvasTransform {
  translateX: number;
  translateY: number;
  rotationRad: number;
  scale: number;
}

export function mapCameraPixelToWorkspace(
  point: Point2D,
  transform: SimilarityTransform,
): Point2D {
  const rotationRad = (transform.rotation_deg * Math.PI) / 180;
  const cos = Math.cos(rotationRad);
  const sin = Math.sin(rotationRad);
  return {
    x: transform.scale * (cos * point.x - sin * point.y) + transform.translation_x,
    y: transform.scale * (sin * point.x + cos * point.y) + transform.translation_y,
  };
}

export function cameraOverlayWorkspaceCorners(
  widthPx: number,
  heightPx: number,
  transform: SimilarityTransform,
): Point2D[] {
  return [
    mapCameraPixelToWorkspace({ x: 0, y: 0 }, transform),
    mapCameraPixelToWorkspace({ x: widthPx, y: 0 }, transform),
    mapCameraPixelToWorkspace({ x: widthPx, y: heightPx }, transform),
    mapCameraPixelToWorkspace({ x: 0, y: heightPx }, transform),
  ];
}

export function cameraOverlayWorkspaceCenter(
  widthPx: number,
  heightPx: number,
  transform: SimilarityTransform,
): Point2D {
  return mapCameraPixelToWorkspace({ x: widthPx / 2, y: heightPx / 2 }, transform);
}

export function cameraOverlayScreenCorners(
  widthPx: number,
  heightPx: number,
  transform: SimilarityTransform,
  vp: ViewportParams,
): Point2D[] {
  return cameraOverlayWorkspaceCorners(widthPx, heightPx, transform).map((corner) =>
    worldToScreen(corner, vp),
  );
}

export function cameraOverlayCanvasTransform(
  transform: SimilarityTransform,
  vp: ViewportParams,
): CameraOverlayCanvasTransform {
  const origin = worldToScreen(
    { x: transform.translation_x, y: transform.translation_y },
    vp,
  );
  return {
    translateX: origin.x,
    translateY: origin.y,
    rotationRad: (transform.rotation_deg * Math.PI) / 180,
    scale: transform.scale * pxPerMm(vp.zoom),
  };
}

export function fitCameraOverlayToWorkspace(
  widthPx: number,
  heightPx: number,
  workspace: { bed_width_mm: number; bed_height_mm: number },
): SimilarityTransform {
  if (
    widthPx <= 0 ||
    heightPx <= 0 ||
    workspace.bed_width_mm <= 0 ||
    workspace.bed_height_mm <= 0
  ) {
    return {
      scale: 1,
      rotation_deg: 0,
      translation_x: 0,
      translation_y: 0,
    };
  }

  const scale = Math.min(
    workspace.bed_width_mm / widthPx,
    workspace.bed_height_mm / heightPx,
  );
  return {
    scale,
    rotation_deg: 0,
    translation_x: (workspace.bed_width_mm - widthPx * scale) / 2,
    translation_y: (workspace.bed_height_mm - heightPx * scale) / 2,
  };
}

export function cameraOverlayScreenHandleGeometry(
  widthPx: number,
  heightPx: number,
  transform: SimilarityTransform,
  vp: ViewportParams,
): CameraOverlayHandleGeometry {
  const screenCorners = cameraOverlayScreenCorners(widthPx, heightPx, transform, vp);
  const center = worldToScreen(cameraOverlayWorkspaceCenter(widthPx, heightPx, transform), vp);
  const topMid = midpoint(screenCorners[0], screenCorners[1]);
  const outward = normalize({
    x: topMid.x - center.x,
    y: topMid.y - center.y,
  }) ?? { x: 0, y: -1 };
  return {
    corners: [
      { id: 'top_left', screen: screenCorners[0] },
      { id: 'top_right', screen: screenCorners[1] },
      { id: 'bottom_right', screen: screenCorners[2] },
      { id: 'bottom_left', screen: screenCorners[3] },
    ],
    rotation: {
      x: topMid.x + outward.x * CAMERA_OVERLAY_ROTATE_HANDLE_OFFSET_PX,
      y: topMid.y + outward.y * CAMERA_OVERLAY_ROTATE_HANDLE_OFFSET_PX,
    },
    center,
  };
}

export function hitTestCameraOverlayControls(
  point: Point2D,
  widthPx: number,
  heightPx: number,
  transform: SimilarityTransform,
  vp: ViewportParams,
): CameraOverlayHitTarget | null {
  const handles = cameraOverlayScreenHandleGeometry(widthPx, heightPx, transform, vp);
  const handleRadius =
    CAMERA_OVERLAY_HANDLE_SIZE_PX / 2 + CAMERA_OVERLAY_HANDLE_HIT_TOLERANCE_PX;

  if (distance(point, handles.rotation) <= handleRadius) {
    return { type: 'rotate' };
  }

  for (const handle of handles.corners) {
    if (
      Math.abs(point.x - handle.screen.x) <= handleRadius &&
      Math.abs(point.y - handle.screen.y) <= handleRadius
    ) {
      return { type: 'scale', handle: handle.id };
    }
  }

  const corners = handles.corners.map((handle) => handle.screen);
  return pointInPolygon(point, corners) ? { type: 'move' } : null;
}

export function translateCameraOverlayTransform(
  transform: SimilarityTransform,
  screenDx: number,
  screenDy: number,
  vp: ViewportParams,
): SimilarityTransform {
  const worldDx = screenDx / pxPerMm(vp.zoom);
  const worldDy = (vp.yAxis === 'up' ? -screenDy : screenDy) / pxPerMm(vp.zoom);
  return {
    ...transform,
    translation_x: transform.translation_x + worldDx,
    translation_y: transform.translation_y + worldDy,
  };
}

export function scaleCameraOverlayTransform(
  widthPx: number,
  heightPx: number,
  startTransform: SimilarityTransform,
  startScreen: Point2D,
  currentScreen: Point2D,
  vp: ViewportParams,
): SimilarityTransform {
  const centerScreen = worldToScreen(
    cameraOverlayWorkspaceCenter(widthPx, heightPx, startTransform),
    vp,
  );
  const startDistance = Math.max(distance(startScreen, centerScreen), 1);
  const currentDistance = Math.max(distance(currentScreen, centerScreen), 1);
  const nextScale = clamp(
    startTransform.scale * (currentDistance / startDistance),
    CAMERA_OVERLAY_MIN_SCALE,
    CAMERA_OVERLAY_MAX_SCALE,
  );
  return transformKeepingCameraCenter(widthPx, heightPx, startTransform, {
    scale: nextScale,
    rotation_deg: startTransform.rotation_deg,
  });
}

export function rotateCameraOverlayTransform(
  widthPx: number,
  heightPx: number,
  startTransform: SimilarityTransform,
  startScreen: Point2D,
  currentScreen: Point2D,
  vp: ViewportParams,
): SimilarityTransform {
  const centerScreen = worldToScreen(
    cameraOverlayWorkspaceCenter(widthPx, heightPx, startTransform),
    vp,
  );
  const startAngle = Math.atan2(startScreen.y - centerScreen.y, startScreen.x - centerScreen.x);
  const currentAngle = Math.atan2(
    currentScreen.y - centerScreen.y,
    currentScreen.x - centerScreen.x,
  );
  return transformKeepingCameraCenter(widthPx, heightPx, startTransform, {
    scale: startTransform.scale,
    rotation_deg: startTransform.rotation_deg + ((currentAngle - startAngle) * 180) / Math.PI,
  });
}

function transformKeepingCameraCenter(
  widthPx: number,
  heightPx: number,
  startTransform: SimilarityTransform,
  next: Pick<SimilarityTransform, 'scale' | 'rotation_deg'>,
): SimilarityTransform {
  const centerPx = { x: widthPx / 2, y: heightPx / 2 };
  const centerWorkspace = mapCameraPixelToWorkspace(centerPx, startTransform);
  const rotatedCenter = rotateAndScale(centerPx, next.scale, next.rotation_deg);
  return {
    scale: next.scale,
    rotation_deg: next.rotation_deg,
    translation_x: centerWorkspace.x - rotatedCenter.x,
    translation_y: centerWorkspace.y - rotatedCenter.y,
  };
}

function rotateAndScale(point: Point2D, scale: number, rotationDeg: number): Point2D {
  const rotationRad = (rotationDeg * Math.PI) / 180;
  const cos = Math.cos(rotationRad);
  const sin = Math.sin(rotationRad);
  return {
    x: scale * (cos * point.x - sin * point.y),
    y: scale * (sin * point.x + cos * point.y),
  };
}

function midpoint(a: Point2D, b: Point2D): Point2D {
  return { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 };
}

function normalize(vector: Point2D): Point2D | null {
  const len = Math.hypot(vector.x, vector.y);
  if (len <= 0.000001) return null;
  return { x: vector.x / len, y: vector.y / len };
}

function distance(a: Point2D, b: Point2D): number {
  return Math.hypot(a.x - b.x, a.y - b.y);
}

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}

function pointInPolygon(point: Point2D, polygon: Point2D[]): boolean {
  let inside = false;
  for (let i = 0, j = polygon.length - 1; i < polygon.length; j = i++) {
    const pi = polygon[i];
    const pj = polygon[j];
    const intersects =
      (pi.y > point.y) !== (pj.y > point.y) &&
      point.x < ((pj.x - pi.x) * (point.y - pi.y)) / (pj.y - pi.y) + pi.x;
    if (intersects) inside = !inside;
  }
  return inside;
}
