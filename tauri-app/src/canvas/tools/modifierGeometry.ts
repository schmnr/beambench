import type { Bounds, Point2D } from '../../types/project';

export function constrainAngle(from: Point2D, to: Point2D): Point2D {
  const dx = to.x - from.x;
  const dy = to.y - from.y;
  if (dx === 0 && dy === 0) return to;

  const angle = Math.atan2(dy, dx);
  const distance = Math.hypot(dx, dy);
  const constrainedAngle = Math.round(angle / (Math.PI / 4)) * (Math.PI / 4);

  return {
    x: from.x + Math.cos(constrainedAngle) * distance,
    y: from.y + Math.sin(constrainedAngle) * distance,
  };
}

export function dragBoundsWithModifiers(
  start: Point2D,
  current: Point2D,
  shiftKey: boolean,
  ctrlOrCmd: boolean,
): Bounds {
  const rawDx = current.x - start.x;
  const rawDy = current.y - start.y;
  let dx = rawDx;
  let dy = rawDy;

  if (shiftKey) {
    const size = Math.max(Math.abs(rawDx), Math.abs(rawDy));
    dx = (rawDx < 0 ? -1 : 1) * size;
    dy = (rawDy < 0 ? -1 : 1) * size;
  }

  if (ctrlOrCmd) {
    return {
      min: { x: start.x - Math.abs(dx), y: start.y - Math.abs(dy) },
      max: { x: start.x + Math.abs(dx), y: start.y + Math.abs(dy) },
    };
  }

  const end = { x: start.x + dx, y: start.y + dy };
  return {
    min: { x: Math.min(start.x, end.x), y: Math.min(start.y, end.y) },
    max: { x: Math.max(start.x, end.x), y: Math.max(start.y, end.y) },
  };
}
