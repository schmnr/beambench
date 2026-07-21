import type { CanvasTool, CanvasMouseEvent, ToolContext } from './types';
import type { ToolOverlay } from '../CanvasRenderer';
import type { Point2D } from '../../types/project';
import { parsePathData, computePathBBox } from '../drawObjects';
import { worldToScreen } from '../ViewportTransform';
import { constrainAngle } from './modifierGeometry';

const CLOSE_THRESHOLD_PX = 8;

interface PenPoint {
  anchor: Point2D;
  handleIn?: Point2D;
  handleOut?: Point2D;
  screenAnchor: Point2D;
  screenHandleIn?: Point2D;
  screenHandleOut?: Point2D;
}

type PenState =
  | { type: 'idle' }
  | {
      type: 'placing';
      points: PenPoint[];
      currentScreen: Point2D;
    }
  | {
      type: 'dragging';
      points: PenPoint[];
      currentScreen: Point2D;
      dragIndex: number;
    };

export class PenTool implements CanvasTool {
  name = 'line';
  private state: PenState = { type: 'idle' };

  onMouseDown(e: CanvasMouseEvent, ctx: ToolContext): void {
    let worldPt: Point2D = { x: e.snappedX, y: e.snappedY };
    const screenPt: Point2D = { x: e.screenX, y: e.screenY };

    if (this.state.type === 'idle') {
      const point: PenPoint = {
        anchor: worldPt,
        screenAnchor: screenPt,
      };
      this.state = {
        type: 'dragging',
        points: [point],
        currentScreen: screenPt,
        dragIndex: 0,
      };
      ctx.requestRender();
      return;
    }

    if (this.state.type === 'placing') {
      const { points } = this.state;

      if (e.shiftKey && points.length > 0) {
        worldPt = constrainAngle(points[points.length - 1].anchor, worldPt);
      }
      const constrainedScreenPt = e.shiftKey && points.length > 0 ? worldToScreen(worldPt, ctx.vp) : screenPt;

      // Close detection: if clicking near first point, close path
      if (points.length >= 2) {
        const first = points[0].screenAnchor;
        const dx = screenPt.x - first.x;
        const dy = screenPt.y - first.y;
        if (Math.sqrt(dx * dx + dy * dy) < CLOSE_THRESHOLD_PX) {
          this.finalize(ctx, true);
          return;
        }
      }

      const point: PenPoint = {
        anchor: worldPt,
        screenAnchor: constrainedScreenPt,
      };
      points.push(point);
      this.state = {
        type: 'dragging',
        points,
        currentScreen: constrainedScreenPt,
        dragIndex: points.length - 1,
      };
      ctx.requestRender();
    }
  }

  onMouseMove(e: CanvasMouseEvent, ctx: ToolContext): void {
    const screenPt: Point2D = { x: e.screenX, y: e.screenY };
    let worldPt: Point2D = { x: e.snappedX, y: e.snappedY };
    let previewScreenPt = screenPt;

    if (this.state.type === 'dragging') {
      const pt = this.state.points[this.state.dragIndex];
      const anchor = pt.anchor;
      const screenAnchor = pt.screenAnchor;

      if (e.shiftKey) {
        worldPt = constrainAngle(anchor, worldPt);
        previewScreenPt = worldToScreen(worldPt, ctx.vp);
      }

      // handleOut = cursor
      pt.handleOut = worldPt;
      pt.screenHandleOut = previewScreenPt;

      // handleIn = mirror of handleOut through anchor
      pt.handleIn = {
        x: anchor.x + (anchor.x - worldPt.x),
        y: anchor.y + (anchor.y - worldPt.y),
      };
      pt.screenHandleIn = {
        x: screenAnchor.x + (screenAnchor.x - previewScreenPt.x),
        y: screenAnchor.y + (screenAnchor.y - previewScreenPt.y),
      };

      this.state.currentScreen = previewScreenPt;
      ctx.requestRender();
      return;
    }

    if (this.state.type === 'placing') {
      const points = this.state.points;
      if (e.shiftKey && points.length > 0) {
        worldPt = constrainAngle(points[points.length - 1].anchor, worldPt);
        previewScreenPt = worldToScreen(worldPt, ctx.vp);
      }
      this.state.currentScreen = previewScreenPt;
      ctx.requestRender();
    }
  }

  onMouseUp(_e: CanvasMouseEvent, ctx: ToolContext): void {
    if (this.state.type === 'dragging') {
      // Check if drag was negligible (no real handle created)
      const pt = this.state.points[this.state.dragIndex];
      if (pt.handleOut) {
        const dx = pt.handleOut.x - pt.anchor.x;
        const dy = pt.handleOut.y - pt.anchor.y;
        if (Math.sqrt(dx * dx + dy * dy) < 0.5) {
          // Negligible drag — remove handles
          pt.handleOut = undefined;
          pt.handleIn = undefined;
          pt.screenHandleOut = undefined;
          pt.screenHandleIn = undefined;
        }
      }

      this.state = {
        type: 'placing',
        points: this.state.points,
        currentScreen: this.state.currentScreen,
      };
      ctx.requestRender();
    }
  }

  onDoubleClick(_e: CanvasMouseEvent, ctx: ToolContext): void {
    if (this.state.type === 'placing' || this.state.type === 'dragging') {
      // Remove the last point which was added by the mouseDown of this double-click
      if (this.state.points.length > 1) {
        this.state.points.pop();
      }
      this.finalize(ctx, false);
    }
  }

  onKeyDown(e: KeyboardEvent, ctx: ToolContext): void {
    if (this.state.type === 'idle') return;

    if (e.key === 'Escape') {
      this.finalize(ctx, false);
    } else if (e.key === 'Backspace') {
      if (this.state.points.length > 1) {
        this.state.points.pop();
        ctx.requestRender();
      } else {
        // Only one point left, cancel
        this.state = { type: 'idle' };
        ctx.requestRender();
      }
    }
  }

  getCursor(_ctx: ToolContext): string {
    return 'crosshair';
  }

  getOverlay(): ToolOverlay {
    if (this.state.type === 'idle') {
      return { type: 'none' };
    }

    const { points, currentScreen } = this.state;
    let closed = false;
    if (this.state.type === 'placing' && points.length >= 2) {
      const first = points[0].screenAnchor;
      const dx = currentScreen.x - first.x;
      const dy = currentScreen.y - first.y;
      closed = Math.sqrt(dx * dx + dy * dy) < CLOSE_THRESHOLD_PX;
    }

    return {
      type: 'pen-preview',
      points: points.map((p) => ({
        anchor: p.anchor,
        handleIn: p.handleIn,
        handleOut: p.handleOut,
      })),
      screenPoints: points.map((p) => ({
        anchor: p.screenAnchor,
        handleIn: p.screenHandleIn,
        handleOut: p.screenHandleOut,
      })),
      currentScreen,
      dragging: this.state.type === 'dragging',
      closed,
    };
  }

  reset(): void {
    this.state = { type: 'idle' };
  }

  private finalize(ctx: ToolContext, closed: boolean): void {
    const points = this.state.type !== 'idle' ? this.state.points : [];

    if (points.length < 2) {
      this.state = { type: 'idle' };
      ctx.requestRender();
      return;
    }

    const pathData = buildPenPathData(points, closed);
    const cmds = parsePathData(pathData);
    const bbox = computePathBBox(cmds);
    const bounds = {
      min: { x: bbox.minX, y: bbox.minY },
      max: { x: bbox.maxX, y: bbox.maxY },
    };

    // projectStore.addObject resolves content-type routing.
    const layerId = ctx.selectedLayerId ?? '__auto__';

    ctx.addObject('Path', layerId, { type: 'vector_path', path_data: pathData, closed }, bounds);

    this.state = { type: 'idle' };
    ctx.requestRender();
  }
}

function buildPenPathData(points: PenPoint[], closed: boolean): string {
  if (points.length < 2) return '';

  const parts: string[] = [`M ${points[0].anchor.x} ${points[0].anchor.y}`];

  for (let i = 1; i < points.length; i++) {
    const prev = points[i - 1];
    const cur = points[i];
    const cp1 = prev.handleOut ?? prev.anchor;
    const cp2 = cur.handleIn ?? cur.anchor;
    parts.push(`C ${cp1.x} ${cp1.y} ${cp2.x} ${cp2.y} ${cur.anchor.x} ${cur.anchor.y}`);
  }

  if (closed) {
    const last = points[points.length - 1];
    const first = points[0];
    const cp1 = last.handleOut ?? last.anchor;
    const cp2 = first.handleIn ?? first.anchor;
    parts.push(`C ${cp1.x} ${cp1.y} ${cp2.x} ${cp2.y} ${first.anchor.x} ${first.anchor.y}`);
    parts.push('Z');
  }

  return parts.join(' ');
}
