import type { CanvasTool, CanvasMouseEvent, ToolContext } from './types';
import type { ToolOverlay } from '../CanvasRenderer';
import type { Point2D } from '../../types/project';
import { useUiStore } from '../../stores/uiStore';
import { dragBoundsWithModifiers } from './modifierGeometry';

type StarState =
  | { type: 'idle' }
  | { type: 'drawing'; startWorld: Point2D; currentWorld: Point2D; previewBoundsMin: Point2D; previewBoundsMax: Point2D };

export class StarTool implements CanvasTool {
  name = 'star';
  dualRadius = false;
  private readonly points = 5;
  private readonly bulge = 0;
  private readonly ratio = 0.5;
  private readonly ratio2 = 0.7;

  private state: StarState = { type: 'idle' };

  onMouseDown(e: CanvasMouseEvent, ctx: ToolContext): void {
    this.dualRadius = useUiStore.getState().lastShapeSubTool === 'dual_star';
    this.state = {
      type: 'drawing',
      startWorld: { x: e.snappedX, y: e.snappedY },
      currentWorld: { x: e.snappedX, y: e.snappedY },
      previewBoundsMin: { x: e.snappedX, y: e.snappedY },
      previewBoundsMax: { x: e.snappedX, y: e.snappedY },
    };
    ctx.requestRender();
  }

  onMouseMove(e: CanvasMouseEvent, ctx: ToolContext): void {
    if (this.state.type !== 'drawing') return;
    this.state.currentWorld = { x: e.snappedX, y: e.snappedY };
    const bounds = dragBoundsWithModifiers(this.state.startWorld, this.state.currentWorld, e.shiftKey, e.ctrlKey);
    this.state.previewBoundsMin = bounds.min;
    this.state.previewBoundsMax = bounds.max;
    ctx.requestRender();
  }

  onMouseUp(e: CanvasMouseEvent, ctx: ToolContext): void {
    if (this.state.type !== 'drawing') return;

    const { startWorld, currentWorld } = this.state;
    const bounds = dragBoundsWithModifiers(startWorld, currentWorld, e.shiftKey, e.ctrlKey);
    const width = bounds.max.x - bounds.min.x;
    const height = bounds.max.y - bounds.min.y;

    this.state = { type: 'idle' };

    if (width < 1 || height < 1) {
      ctx.requestRender();
      return;
    }

    // projectStore.addObject resolves content-type routing.
    const layerId = ctx.selectedLayerId ?? '__auto__';

    ctx.addObject(
      this.dualRadius ? 'Dual Radius Star' : 'Star',
      layerId,
      {
        type: 'star',
        points: this.points,
        bulge: this.bulge,
        ratio: this.ratio,
        dual_radius: this.dualRadius,
        ratio2: this.dualRadius ? this.ratio2 : null,
        corner_radius: 0,
        corner_radii: [],
      },
      bounds,
    );

    ctx.requestRender();
  }

  getCursor(): string {
    return 'crosshair';
  }

  getOverlay(): ToolOverlay {
    if (this.state.type !== 'drawing') return { type: 'none' };
    return {
      type: 'shape-preview',
      startWorld: this.state.previewBoundsMin,
      endWorld: this.state.previewBoundsMax,
      kind: 'star',
      sides: this.points,
      bulge: this.bulge,
      ratio: this.ratio,
      ratio2: this.ratio2,
      dualRadius: this.dualRadius,
    };
  }

  reset(): void {
    this.state = { type: 'idle' };
  }
}
