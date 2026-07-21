import type { Point2D, Workspace } from '../types/project';

export function canvasToMachinePoint(point: Point2D, workspace: Workspace): Point2D {
  if (workspace.origin !== 'bottom_left') return point;
  return {
    x: point.x,
    y: workspace.bed_height_mm - point.y,
  };
}

export function machineToCanvasPoint(point: Point2D, workspace: Workspace): Point2D {
  return canvasToMachinePoint(point, workspace);
}
