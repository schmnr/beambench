import type { CanvasTool, CanvasMouseEvent, ToolContext } from './types';
import type { ToolOverlay } from '../CanvasRenderer';
import type { Point2D } from '../../types/project';
import { useUiStore } from '../../stores/uiStore';
import { dragBoundsWithModifiers } from './modifierGeometry';

type RectState =
  | { type: 'idle' }
  | { type: 'drawing'; startWorld: Point2D; currentWorld: Point2D; previewBoundsMin: Point2D; previewBoundsMax: Point2D };

export class RectTool implements CanvasTool {
  name = 'rect';
  private state: RectState = { type: 'idle' };

  onMouseDown(e: CanvasMouseEvent, ctx: ToolContext): void {
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
    if (this.state.type === 'drawing') {
      this.state.currentWorld = { x: e.snappedX, y: e.snappedY };
      const bounds = dragBoundsWithModifiers(this.state.startWorld, this.state.currentWorld, e.shiftKey, e.ctrlKey);
      this.state.previewBoundsMin = bounds.min;
      this.state.previewBoundsMax = bounds.max;
      ctx.requestRender();
    }
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

    // Layer-family resolver in projectStore.addObject handles
    // content-type routing. Tools just forward the selection (or
    // `__auto__` for fresh projects).
    const layerId = ctx.selectedLayerId ?? '__auto__';

    ctx.addObject(
      'Rectangle',
      layerId,
      { type: 'shape', kind: 'rectangle', width, height, corner_radius: useUiStore.getState().defaultCornerRadius },
      bounds,
    );

    ctx.requestRender();
  }

  getCursor(): string {
    return 'crosshair';
  }

  getOverlay(): ToolOverlay {
    if (this.state.type === 'drawing') {
      return {
        type: 'shape-preview',
        startWorld: this.state.previewBoundsMin,
        endWorld: this.state.previewBoundsMax,
        kind: 'rectangle',
        cornerRadius: useUiStore.getState().defaultCornerRadius,
      };
    }
    return { type: 'none' };
  }

  reset(): void {
    this.state = { type: 'idle' };
  }
}
