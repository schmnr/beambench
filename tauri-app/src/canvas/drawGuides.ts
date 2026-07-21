import type { SnapLine } from './alignment';
import type { ViewportParams } from './ViewportTransform';
import { worldToScreen } from './ViewportTransform';
import { SNAP_GUIDE_COLOR, SNAP_GUIDE_ALPHA } from './constants';

/**
 * Draw snap guide lines on the canvas overlay.
 * Each guide is a full-canvas-spanning dashed line at the guide's world coordinate.
 */
export function drawSnapGuides(
  ctx: CanvasRenderingContext2D,
  guides: SnapLine[],
  vp: ViewportParams,
): void {
  if (guides.length === 0) return;

  ctx.save();
  ctx.strokeStyle = SNAP_GUIDE_COLOR;
  ctx.globalAlpha = SNAP_GUIDE_ALPHA;
  ctx.lineWidth = 1;
  ctx.setLineDash([4, 4]);

  for (const guide of guides) {
    ctx.beginPath();
    if (guide.axis === 'x') {
      // Vertical line at world X coordinate
      const screen = worldToScreen({ x: guide.value, y: 0 }, vp);
      ctx.moveTo(screen.x, 0);
      ctx.lineTo(screen.x, vp.canvasHeight);
    } else {
      // Horizontal line at world Y coordinate
      const screen = worldToScreen({ x: 0, y: guide.value }, vp);
      ctx.moveTo(0, screen.y);
      ctx.lineTo(vp.canvasWidth, screen.y);
    }
    ctx.stroke();
  }

  ctx.restore();
}
