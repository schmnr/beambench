import type { CanvasTool, CanvasMouseEvent, ToolContext } from './types';
import type { ToolOverlay } from '../CanvasRenderer';
import { constrainAngle } from './modifierGeometry';
import { hitTestPoint } from '../hitTest';
import {
  buildDragMeasurement,
  buildHoverMeasurement,
  visibleMeasurementObjects,
} from '../measurement';
import { useMeasurementStore } from '../../stores/measurementStore';

export class MeasureTool implements CanvasTool {
  name = 'measure';

  onMouseDown(e: CanvasMouseEvent, ctx: ToolContext): void {
    const start = { x: e.snappedX, y: e.snappedY };
    useMeasurementStore.getState().setDrag(buildDragMeasurement(start, start));
    ctx.requestRender();
  }

  onMouseMove(e: CanvasMouseEvent, ctx: ToolContext): void {
    const current = useMeasurementStore.getState().state;
    if (current.type === 'drag') {
      const rawEndWorld = { x: e.snappedX, y: e.snappedY };
      const endWorld = e.shiftKey ? constrainAngle(current.start, rawEndWorld) : rawEndWorld;
      useMeasurementStore.getState().setDrag(buildDragMeasurement(current.start, endWorld));
      ctx.requestRender();
      return;
    }

    this.updateHover(e, ctx);
    ctx.requestRender();
  }

  onMouseUp(e: CanvasMouseEvent, ctx: ToolContext): void {
    const current = useMeasurementStore.getState().state;
    if (current.type === 'drag') {
      if (current.lengthMm <= 1e-6) {
        this.updateHover(e, ctx);
      } else {
        useMeasurementStore.getState().clear();
      }
      ctx.requestRender();
    }
  }

  getCursor(): string {
    return 'crosshair';
  }

  getOverlay(): ToolOverlay {
    const current = useMeasurementStore.getState().state;
    if (current.type === 'drag') {
      return {
        type: 'measure-inspection',
        drag: current,
      };
    }
    if (current.type === 'hover') {
      return {
        type: 'measure-inspection',
        hoverObjectId: current.objectId,
        hoverSegment: current.segment,
      };
    }
    return { type: 'none' };
  }

  reset(): void {
    useMeasurementStore.getState().clear();
  }

  private updateHover(e: CanvasMouseEvent, ctx: ToolContext): void {
    const visibleObjects = visibleMeasurementObjects(ctx.objects, ctx.layers);
    const object = hitTestPoint(
      { x: e.screenX, y: e.screenY },
      visibleObjects,
      ctx.vp,
      true,
    );

    if (!object) {
      useMeasurementStore.getState().clear();
      return;
    }

    useMeasurementStore.getState().setHover(
      buildHoverMeasurement(object, { x: e.screenX, y: e.screenY }, ctx.vp, ctx.objects),
    );
  }
}
