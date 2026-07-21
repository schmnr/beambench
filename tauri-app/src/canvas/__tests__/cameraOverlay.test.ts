import { describe, expect, it } from 'vitest';
import {
  cameraOverlayCanvasTransform,
  cameraOverlayScreenCorners,
  cameraOverlayScreenHandleGeometry,
  cameraOverlayWorkspaceCorners,
  fitCameraOverlayToWorkspace,
  hitTestCameraOverlayControls,
  mapCameraPixelToWorkspace,
  rotateCameraOverlayTransform,
  scaleCameraOverlayTransform,
  translateCameraOverlayTransform,
} from '../cameraOverlay';
import { canvasToMachinePoint } from '../../utils/workspaceCoordinates';

describe('cameraOverlay geometry', () => {
  it('maps camera pixels through the saved similarity transform', () => {
    const transform = {
      scale: 0.5,
      rotation_deg: 90,
      translation_x: 10,
      translation_y: 20,
    };

    const point = mapCameraPixelToWorkspace({ x: 4, y: 8 }, transform);

    expect(point.x).toBeCloseTo(6);
    expect(point.y).toBeCloseTo(22);
  });

  it('maps overlay corners to predictable workspace and screen coordinates', () => {
    const transform = {
      scale: 0.2,
      rotation_deg: 0,
      translation_x: 10,
      translation_y: 15,
    };
    const vp = {
      offset: { x: 10, y: 15 },
      zoom: 100,
      canvasWidth: 200,
      canvasHeight: 100,
    };

    expect(cameraOverlayWorkspaceCorners(100, 50, transform)).toEqual([
      { x: 10, y: 15 },
      { x: 30, y: 15 },
      { x: 30, y: 25 },
      { x: 10, y: 25 },
    ]);
    expect(cameraOverlayScreenCorners(100, 50, transform, vp)).toEqual([
      { x: 100, y: 50 },
      { x: 140, y: 50 },
      { x: 140, y: 70 },
      { x: 100, y: 70 },
    ]);
    expect(cameraOverlayCanvasTransform(transform, vp)).toEqual({
      translateX: 100,
      translateY: 50,
      rotationRad: 0,
      scale: 0.4,
    });
  });

  it('fits a camera frame inside the workspace without stretching aspect ratio', () => {
    const transform = fitCameraOverlayToWorkspace(1280, 720, {
      bed_width_mm: 370,
      bed_height_mm: 360,
    });

    expect(transform.scale).toBeCloseTo(370 / 1280);
    expect(transform.translation_x).toBeCloseTo(0);
    expect(transform.translation_y).toBeCloseTo((360 - 720 * (370 / 1280)) / 2);
    expect(cameraOverlayWorkspaceCorners(1280, 720, transform)[2]).toEqual({
      x: 370,
      y: 284.0625,
    });
  });

  it('keeps fitted overlay corners consistent with bottom-left machine coordinates', () => {
    const workspace = {
      bed_width_mm: 200,
      bed_height_mm: 100,
      origin: 'bottom_left' as const,
    };
    const transform = fitCameraOverlayToWorkspace(100, 50, workspace);
    const [topLeft, , bottomRight, bottomLeft] = cameraOverlayWorkspaceCorners(100, 50, transform);

    expect(topLeft).toEqual({ x: 0, y: 0 });
    expect(bottomRight).toEqual({ x: 200, y: 100 });
    expect(canvasToMachinePoint(topLeft, workspace)).toEqual({ x: 0, y: 100 });
    expect(canvasToMachinePoint(bottomLeft, workspace)).toEqual({ x: 0, y: 0 });
  });

  it('hit-tests overlay body, scale handles, and rotation handle', () => {
    const transform = fitCameraOverlayToWorkspace(100, 50, {
      bed_width_mm: 200,
      bed_height_mm: 100,
    });
    const vp = {
      offset: { x: 100, y: 50 },
      zoom: 100,
      canvasWidth: 400,
      canvasHeight: 300,
    };
    const handles = cameraOverlayScreenHandleGeometry(100, 50, transform, vp);

    expect(hitTestCameraOverlayControls(handles.center, 100, 50, transform, vp)).toEqual({
      type: 'move',
    });
    expect(hitTestCameraOverlayControls(handles.corners[0].screen, 100, 50, transform, vp)).toEqual({
      type: 'scale',
      handle: 'top_left',
    });
    expect(hitTestCameraOverlayControls(handles.rotation, 100, 50, transform, vp)).toEqual({
      type: 'rotate',
    });
    expect(hitTestCameraOverlayControls({ x: 0, y: 0 }, 100, 50, transform, vp)).toBeNull();
  });

  it('keeps handle geometry screen-sized independent of zoom', () => {
    const transform = {
      scale: 0.5,
      rotation_deg: 0,
      translation_x: 10,
      translation_y: 20,
    };
    const lowZoom = cameraOverlayScreenHandleGeometry(100, 50, transform, {
      offset: { x: 0, y: 0 },
      zoom: 50,
      canvasWidth: 400,
      canvasHeight: 300,
    });
    const highZoom = cameraOverlayScreenHandleGeometry(100, 50, transform, {
      offset: { x: 0, y: 0 },
      zoom: 200,
      canvasWidth: 400,
      canvasHeight: 300,
    });

    expect(lowZoom.rotation.y - (lowZoom.corners[0].screen.y + lowZoom.corners[1].screen.y) / 2)
      .toBeCloseTo(-20);
    expect(highZoom.rotation.y - (highZoom.corners[0].screen.y + highZoom.corners[1].screen.y) / 2)
      .toBeCloseTo(-20);
  });

  it('moves, scales, and rotates around predictable points', () => {
    const transform = {
      scale: 1,
      rotation_deg: 0,
      translation_x: 0,
      translation_y: 0,
    };
    const vp = {
      offset: { x: 0, y: 0 },
      zoom: 100,
      canvasWidth: 400,
      canvasHeight: 300,
    };

    expect(translateCameraOverlayTransform(transform, 20, 10, vp)).toEqual({
      scale: 1,
      rotation_deg: 0,
      translation_x: 10,
      translation_y: 5,
    });

    const scaled = scaleCameraOverlayTransform(
      100,
      50,
      transform,
      { x: 300, y: 150 },
      { x: 300, y: 100 },
      vp,
    );
    expect(scaled.scale).toBeCloseTo(2);
    expect(mapCameraPixelToWorkspace({ x: 50, y: 25 }, scaled)).toEqual({ x: 50, y: 25 });

    const rotated = rotateCameraOverlayTransform(
      100,
      50,
      transform,
      { x: 300, y: 150 },
      { x: 350, y: 200 },
      vp,
    );
    expect(rotated.rotation_deg).toBeCloseTo(90);
    expect(mapCameraPixelToWorkspace({ x: 50, y: 25 }, rotated).x).toBeCloseTo(50);
    expect(mapCameraPixelToWorkspace({ x: 50, y: 25 }, rotated).y).toBeCloseTo(25);
  });
});
