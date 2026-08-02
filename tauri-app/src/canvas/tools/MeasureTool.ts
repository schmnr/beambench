import type { CanvasTool, CanvasMouseEvent, ToolContext } from './types';
import type { ToolOverlay } from '../CanvasRenderer';
import { constrainAngle } from './modifierGeometry';
import { hitTestPoint } from '../hitTest';
import {
  buildAngleMeasurement,
  buildDragMeasurement,
  buildGapMeasurement,
  buildHoverMeasurement,
  buildLinearMeasurement,
  buildRadiusMeasurement,
  visibleMeasurementObjects,
} from '../measurement';
import { useMeasurementStore } from '../../stores/measurementStore';
import { useUiStore } from '../../stores/uiStore';

const DRAG_THRESHOLD_PX = 3;

interface MeasurePointerDown {
  screenX: number;
  screenY: number;
  start: { x: number; y: number };
  mode: ReturnType<typeof useMeasurementStore.getState>['mode'];
}

export class MeasureTool implements CanvasTool {
  name = 'measure';
  private pointerDown: MeasurePointerDown | null = null;

  onMouseDown(e: CanvasMouseEvent, ctx: ToolContext): void {
    if (e.button !== 0) return;
    const store = useMeasurementStore.getState();
    const start = store.mode === 'linear' && store.pending?.kind === 'linear'
      ? store.pending.start
      : { x: e.snappedX, y: e.snappedY };
    this.pointerDown = {
      screenX: e.screenX,
      screenY: e.screenY,
      start,
      mode: store.mode,
    };
    if (store.mode === 'linear') {
      if (store.pending?.kind !== 'linear') store.setResult(null);
      store.setDraft(buildDragMeasurement(start, { x: e.snappedX, y: e.snappedY }));
    }
    ctx.requestRender();
  }

  onMouseMove(e: CanvasMouseEvent, ctx: ToolContext): void {
    this.updateHover(e, ctx);
    const store = useMeasurementStore.getState();
    if (store.mode === 'linear' && this.pointerDown?.mode === 'linear') {
      const rawEndWorld = { x: e.snappedX, y: e.snappedY };
      const endWorld = e.shiftKey ? constrainAngle(this.pointerDown.start, rawEndWorld) : rawEndWorld;
      store.setDraft(buildDragMeasurement(this.pointerDown.start, endWorld));
      ctx.requestRender();
      return;
    }
    if (store.mode === 'linear' && store.pending?.kind === 'linear') {
      const rawEndWorld = { x: e.snappedX, y: e.snappedY };
      const endWorld = e.shiftKey ? constrainAngle(store.pending.start, rawEndWorld) : rawEndWorld;
      store.setDraft(buildDragMeasurement(store.pending.start, endWorld));
    }
    ctx.requestRender();
  }

  onMouseUp(e: CanvasMouseEvent, ctx: ToolContext): void {
    if (e.button !== 0 || !this.pointerDown) return;
    const pointerDown = this.pointerDown;
    this.pointerDown = null;
    const store = useMeasurementStore.getState();
    if (pointerDown.mode !== store.mode) {
      store.setDraft(null);
      return;
    }

    if (store.mode === 'linear') {
      const rawEnd = { x: e.snappedX, y: e.snappedY };
      const end = e.shiftKey ? constrainAngle(pointerDown.start, rawEnd) : rawEnd;
      const measurement = buildLinearMeasurement(pointerDown.start, end);
      const dragged = Math.hypot(e.screenX - pointerDown.screenX, e.screenY - pointerDown.screenY) >= DRAG_THRESHOLD_PX;
      if (dragged && measurement.lengthMm > 1e-6) {
        store.setResult(measurement);
      } else if (store.pending?.kind === 'linear') {
        if (measurement.lengthMm > 1e-6) store.setResult(measurement);
      } else {
        store.setPending({ kind: 'linear', start: { x: e.snappedX, y: e.snappedY } });
      }
      ctx.requestRender();
      return;
    }

    const hover = this.hoverAt(e, ctx);
    store.setHover(hover);
    if (!hover) {
      ctx.requestRender();
      return;
    }

    if (store.mode === 'angle' && hover.segment) {
      if (store.pending?.kind === 'angle') {
        if (store.pending.objectId !== hover.objectId || store.pending.segment.segmentIndex !== hover.segment.segmentIndex) {
          store.setResult(buildAngleMeasurement(store.pending.segment, hover.segment));
        }
      } else {
        store.setPending({ kind: 'angle', objectId: hover.objectId, segment: hover.segment });
      }
    } else if (store.mode === 'radius') {
      const object = ctx.objects.find((candidate) => candidate.id === hover.objectId);
      store.setResult(object ? buildRadiusMeasurement(object) : null);
    } else if (store.mode === 'gap') {
      if (store.pending?.kind === 'gap') {
        const pending = store.pending;
        const first = ctx.objects.find((candidate) => candidate.id === pending.objectId);
        const second = ctx.objects.find((candidate) => candidate.id === hover.objectId);
        if (first && second && first.id !== second.id) {
          store.setResult(buildGapMeasurement(first, second, ctx.objects));
        }
      } else {
        store.setPending({
          kind: 'gap',
          objectId: hover.objectId,
          objectName: hover.objectMetrics.objectName,
        });
      }
    }
    ctx.requestRender();
  }

  getCursor(): string {
    return 'crosshair';
  }

  getOverlay(): ToolOverlay {
    const store = useMeasurementStore.getState();
    return {
      type: 'measure-inspection',
      hoverObjectId: store.hover?.objectId,
      hoverSegment: store.hover?.segment,
      draft: store.draft,
      result: store.result,
      pending: store.pending,
    };
  }

  reset(): void {
    this.pointerDown = null;
    useMeasurementStore.getState().clear();
  }

  onKeyDown(e: KeyboardEvent, ctx: ToolContext): void {
    if (e.key !== 'Escape') return;
    const store = useMeasurementStore.getState();
    if (store.draft || store.pending || store.result) {
      this.pointerDown = null;
      store.clearMeasurement();
      ctx.requestRender();
      e.preventDefault();
      return;
    }
    useUiStore.getState().setActiveTool('select');
    e.preventDefault();
  }

  private updateHover(e: CanvasMouseEvent, ctx: ToolContext): void {
    useMeasurementStore.getState().setHover(this.hoverAt(e, ctx));
  }

  private hoverAt(e: CanvasMouseEvent, ctx: ToolContext) {
    const visibleObjects = visibleMeasurementObjects(ctx.objects, ctx.layers);
    const object = hitTestPoint(
      { x: e.screenX, y: e.screenY },
      visibleObjects,
      ctx.vp,
      true,
      ctx.layers,
    );

    if (!object) {
      return null;
    }

    return buildHoverMeasurement(object, { x: e.screenX, y: e.screenY }, ctx.vp, ctx.objects);
  }
}
