import type { CanvasMouseEvent, CanvasTool, ToolContext } from './types';
import type { ToolOverlay } from '../CanvasRenderer';
import type { Bounds, Point2D, ProjectObject, Transform2D } from '../../types/project';
import { applyAroundCenter } from '../alignment';
import { isTransformLocked, notifyTransformLocked } from '../../utils/transformLocks';
import i18n from '../../i18n';

type ToolState =
  | { type: 'idle' }
  | { type: 'pivot'; pivot: Point2D; pivotScreen: Point2D }
  | {
      type: 'dragging';
      pivot: Point2D;
      pivotScreen: Point2D;
      source: Point2D;
      sourceScreen: Point2D;
      current: Point2D;
      currentScreen: Point2D;
      objectIds: string[];
      origBounds: Map<string, Bounds>;
      origTransforms: Map<string, Transform2D>;
    };

function distance(a: Point2D, b: Point2D): number {
  return Math.hypot(b.x - a.x, b.y - a.y);
}

function angle(a: Point2D, b: Point2D): number {
  return Math.atan2(b.y - a.y, b.x - a.x);
}

function scalePoint(point: Point2D, pivot: Point2D, scale: number): Point2D {
  return {
    x: pivot.x + (point.x - pivot.x) * scale,
    y: pivot.y + (point.y - pivot.y) * scale,
  };
}

function scaleBounds(bounds: Bounds, pivot: Point2D, scale: number): Bounds {
  const a = scalePoint(bounds.min, pivot, scale);
  const b = scalePoint(bounds.max, pivot, scale);
  return {
    min: { x: Math.min(a.x, b.x), y: Math.min(a.y, b.y) },
    max: { x: Math.max(a.x, b.x), y: Math.max(a.y, b.y) },
  };
}

function boundsCenter(bounds: Bounds): Point2D {
  return {
    x: (bounds.min.x + bounds.max.x) / 2,
    y: (bounds.min.y + bounds.max.y) / 2,
  };
}

function translateBounds(bounds: Bounds, dx: number, dy: number): Bounds {
  return {
    min: { x: bounds.min.x + dx, y: bounds.min.y + dy },
    max: { x: bounds.max.x + dx, y: bounds.max.y + dy },
  };
}

function rotatePoint(point: Point2D, pivot: Point2D, radians: number): Point2D {
  const c = Math.cos(radians);
  const s = Math.sin(radians);
  const dx = point.x - pivot.x;
  const dy = point.y - pivot.y;
  return {
    x: pivot.x + c * dx - s * dy,
    y: pivot.y + s * dx + c * dy,
  };
}

function rotateTransform(radians: number): Transform2D {
  const c = Math.cos(radians);
  const s = Math.sin(radians);
  return { a: c, b: s, c: -s, d: c, tx: 0, ty: 0 };
}

function composeTransforms(a: Transform2D, b: Transform2D): Transform2D {
  return {
    a: a.a * b.a + a.c * b.b,
    b: a.b * b.a + a.d * b.b,
    c: a.a * b.c + a.c * b.d,
    d: a.b * b.c + a.d * b.d,
    tx: a.a * b.tx + a.c * b.ty + a.tx,
    ty: a.b * b.tx + a.d * b.ty + a.ty,
  };
}

function collectGroupMemberIds(objects: ProjectObject[], objectId: string, output: string[], seen: Set<string>): void {
  if (seen.has(objectId)) return;
  const object = objects.find((candidate) => candidate.id === objectId);
  if (!object) return;
  seen.add(objectId);
  output.push(objectId);
  if (object.data.type !== 'group') return;
  for (const childId of object.data.children) {
    collectGroupMemberIds(objects, childId, output, seen);
  }
}

function expandGroupSelectionMembers(objects: ProjectObject[], objectIds: string[]): string[] {
  const expanded: string[] = [];
  const seen = new Set<string>();
  for (const objectId of objectIds) {
    collectGroupMemberIds(objects, objectId, expanded, seen);
  }
  return expanded;
}

export class TwoPointRotateScaleTool implements CanvasTool {
  name = 'two_point_rotate_scale';
  private state: ToolState = { type: 'idle' };

  onMouseDown(e: CanvasMouseEvent, ctx: ToolContext): void {
    if (e.button !== 0) return;
    const point = { x: e.snappedX, y: e.snappedY };
    const screen = { x: e.screenX, y: e.screenY };
    if (this.state.type === 'idle') {
      this.state = { type: 'pivot', pivot: point, pivotScreen: screen };
      ctx.setStatusMessage(i18n.t('canvas_status.two_point_drag_second'));
      ctx.requestRender();
      return;
    }
    if (this.state.type === 'pivot') {
      const selectedRootIds = ctx.selectedObjectIds
        .filter((id) => ctx.objects.find((object) => object.id === id && !object.locked));
      const ids = expandGroupSelectionMembers(ctx.objects, selectedRootIds);
      const origBounds = new Map<string, Bounds>();
      const origTransforms = new Map<string, Transform2D>();
      for (const id of ids) {
        const object = ctx.objects.find((candidate) => candidate.id === id);
        if (!object) continue;
        origBounds.set(id, { min: { ...object.bounds.min }, max: { ...object.bounds.max } });
        origTransforms.set(id, { ...object.transform });
      }
      this.state = {
        type: 'dragging',
        pivot: this.state.pivot,
        pivotScreen: this.state.pivotScreen,
        source: point,
        sourceScreen: screen,
        current: point,
        currentScreen: screen,
        objectIds: ids,
        origBounds,
        origTransforms,
      };
      ctx.requestRender();
    }
  }

  onMouseMove(e: CanvasMouseEvent, ctx: ToolContext): void {
    if (this.state.type !== 'dragging') return;
    this.state.current = { x: e.snappedX, y: e.snappedY };
    this.state.currentScreen = { x: e.screenX, y: e.screenY };
    this.applyLivePreview(this.state, ctx, this.state.current, e.shiftKey);
    ctx.requestRender();
  }

  onMouseUp(e: CanvasMouseEvent, ctx: ToolContext): void {
    if (this.state.type !== 'dragging') return;
    const state = this.state;
    const ids = state.objectIds;
    if (ids.length === 0) {
      this.reset();
      ctx.setStatusMessage('');
      ctx.requestRender();
      return;
    }
    if (ids.some((id) => ctx.objects.find((object) => object.id === id && object.locked))) {
      this.restoreSnapshots(state, ctx);
      this.reset();
      ctx.setStatusMessage('');
      ctx.requestRender();
      return;
    }

    if (isTransformLocked(ctx.transformLocks, 'rotation')) {
      this.restoreSnapshots(state, ctx);
      this.reset();
      ctx.setStatusMessage('');
      ctx.requestRender();
      notifyTransformLocked('rotation');
      return;
    }

    const sourceDistance = distance(state.pivot, state.source);
    const target = { x: e.snappedX, y: e.snappedY };
    if (sourceDistance < 1e-6 || distance(state.pivot, target) < 1e-6) {
      this.restoreSnapshots(state, ctx);
      this.reset();
      ctx.setStatusMessage('');
      ctx.requestRender();
      return;
    }

    const deltaDeg = (angle(state.pivot, target) - angle(state.pivot, state.source)) * 180 / Math.PI;
    const applyScale = e.shiftKey;
    const scale = distance(state.pivot, target) / sourceDistance;
    const shouldScale = applyScale && Math.abs(scale - 1) > 1e-6;
    const shouldRotate = Math.abs(deltaDeg) > 0.01;

    if (shouldScale && isTransformLocked(ctx.transformLocks, 'scale')) {
      this.restoreSnapshots(state, ctx);
      this.reset();
      ctx.setStatusMessage('');
      ctx.requestRender();
      notifyTransformLocked('scale');
      return;
    }

    if (!shouldScale && !shouldRotate) {
      this.restoreSnapshots(state, ctx);
      this.reset();
      ctx.setStatusMessage('');
      ctx.requestRender();
      return;
    }

    this.reset();
    ctx.setStatusMessage('');
    ctx.requestRender();

    void (async () => {
      if (shouldScale) {
        const entries = ids
          .map((id) => {
            const bounds = state.origBounds.get(id);
            return bounds ? { id, bounds: scaleBounds(bounds, state.pivot, scale) } : null;
          })
          .filter((entry): entry is { id: string; bounds: Bounds } => entry !== null);
        if (entries.length > 0) await ctx.updateObjectBoundsBatch(entries);
      }
      if (shouldRotate) {
        await ctx.rotateObjects(ids, deltaDeg, state.pivot);
      }
    })();
  }

  onKeyDown(e: KeyboardEvent, ctx: ToolContext): void {
    if (e.key !== 'Escape') return;
    if (this.state.type === 'dragging') {
      this.restoreSnapshots(this.state, ctx);
    }
    this.reset();
    ctx.setStatusMessage('');
    ctx.requestRender();
  }

  getCursor(): string {
    return 'crosshair';
  }

  getOverlay(): ToolOverlay {
    if (this.state.type === 'pivot') {
      return {
        type: 'measure-line',
        startScreen: this.state.pivotScreen,
        endScreen: this.state.pivotScreen,
        distanceMm: 0,
        angleDeg: 0,
      };
    }
    if (this.state.type === 'dragging') {
      return {
        type: 'measure-line',
        startScreen: this.state.pivotScreen,
        endScreen: this.state.currentScreen,
        distanceMm: distance(this.state.pivot, this.state.current),
        angleDeg: (angle(this.state.pivot, this.state.current) - angle(this.state.pivot, this.state.source)) * 180 / Math.PI,
      };
    }
    return { type: 'none' };
  }

  reset(): void {
    this.state = { type: 'idle' };
  }

  private restoreSnapshots(state: Extract<ToolState, { type: 'dragging' }>, ctx: ToolContext): void {
    for (const id of state.objectIds) {
      const object = ctx.objects.find((candidate) => candidate.id === id);
      const bounds = state.origBounds.get(id);
      const transform = state.origTransforms.get(id);
      if (!object || !bounds || !transform) continue;
      object.bounds = { min: { ...bounds.min }, max: { ...bounds.max } };
      object.transform = { ...transform };
    }
  }

  private applyLivePreview(
    state: Extract<ToolState, { type: 'dragging' }>,
    ctx: ToolContext,
    target: Point2D,
    applyScale: boolean,
  ): void {
    const sourceDistance = distance(state.pivot, state.source);
    const targetDistance = distance(state.pivot, target);
    if (sourceDistance < 1e-6 || targetDistance < 1e-6) {
      this.restoreSnapshots(state, ctx);
      return;
    }
    if (isTransformLocked(ctx.transformLocks, 'rotation')) {
      this.restoreSnapshots(state, ctx);
      return;
    }
    if (applyScale && isTransformLocked(ctx.transformLocks, 'scale')) {
      this.restoreSnapshots(state, ctx);
      return;
    }

    const deltaRad = angle(state.pivot, target) - angle(state.pivot, state.source);
    const scale = applyScale ? targetDistance / sourceDistance : 1;
    const rotation = rotateTransform(deltaRad);

    for (const id of state.objectIds) {
      const object = ctx.objects.find((candidate) => candidate.id === id);
      const origBounds = state.origBounds.get(id);
      const origTransform = state.origTransforms.get(id);
      if (!object || !origBounds || !origTransform) continue;

      const scaledBounds = applyScale ? scaleBounds(origBounds, state.pivot, scale) : origBounds;
      const originalCenter = boundsCenter(origBounds);
      const transformedCenter = applyAroundCenter(origTransform, originalCenter, originalCenter);
      const scaledTransformedCenter = applyScale
        ? scalePoint(transformedCenter, state.pivot, scale)
        : transformedCenter;
      const rotatedCenter = rotatePoint(scaledTransformedCenter, state.pivot, deltaRad);

      object.bounds = translateBounds(
        scaledBounds,
        rotatedCenter.x - scaledTransformedCenter.x,
        rotatedCenter.y - scaledTransformedCenter.y,
      );
      object.transform = composeTransforms(rotation, origTransform);
    }
  }
}
