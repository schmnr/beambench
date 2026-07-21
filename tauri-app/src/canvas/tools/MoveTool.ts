import type { CanvasTool, CanvasMouseEvent, ToolContext } from './types';
import type { ToolOverlay } from '../CanvasRenderer';
import type { Point2D, Bounds, ProjectObject } from '../../types/project';
import { hitTestPoint } from '../hitTest';
import { DRAG_THRESHOLD_PX, SNAP_THRESHOLD_PX } from '../constants';
import { getSnapCandidates, findBoundsSnap, snapThresholdWorld } from '../snapping';
import { useAppStore } from '../../stores/appStore';
import type { SnapGuide } from '../snapping';
import { isTransformLocked, notifyTransformLocked } from '../../utils/transformLocks';

type DragObjectState = {
  bounds: Bounds;
};

type MoveState =
  | { type: 'idle' }
  | {
    type: 'maybe-drag';
    startScreen: Point2D;
    startRawWorld: Point2D;
    startSnappedWorld: Point2D;
    objectId: string;
  }
  | {
    type: 'dragging';
    startRawWorld: Point2D;
    startSnappedWorld: Point2D;
    objectIds: string[];
    initialStates: Map<string, DragObjectState>;
  };

export class MoveTool implements CanvasTool {
  name = 'move';
  private state: MoveState = { type: 'idle' };
  private snapGuides: SnapGuide[] = [];

  onMouseDown(e: CanvasMouseEvent, ctx: ToolContext): void {
    const screenPt = { x: e.screenX, y: e.screenY };
    const hit = hitTestPoint(screenPt, ctx.objects, ctx.vp);

    if (hit) {
      const objectId = topLevelSelectableObjectId(hit.id, ctx.objects);
      const currentSelectionIds = normalizeSelectableIds(ctx.selectedObjectIds, ctx.objects);
      if (!currentSelectionIds.includes(objectId)) {
        ctx.selectObjects([objectId]);
      }
      if (isTransformLocked(ctx.transformLocks, 'position')) {
        notifyTransformLocked('position');
        return;
      }
      this.state = {
        type: 'maybe-drag',
        startScreen: screenPt,
        startRawWorld: { x: e.worldX, y: e.worldY },
        startSnappedWorld: { x: e.snappedX, y: e.snappedY },
        objectId,
      };
    }
  }

  onMouseMove(e: CanvasMouseEvent, ctx: ToolContext): void {
    const screenPt = { x: e.screenX, y: e.screenY };
    const rawPt = { x: e.worldX, y: e.worldY };
    const snappedPt = { x: e.snappedX, y: e.snappedY };

    switch (this.state.type) {
      case 'maybe-drag': {
        const dx = screenPt.x - this.state.startScreen.x;
        const dy = screenPt.y - this.state.startScreen.y;
        if (Math.sqrt(dx * dx + dy * dy) > DRAG_THRESHOLD_PX) {
          if (isTransformLocked(ctx.transformLocks, 'position')) {
            notifyTransformLocked('position');
            return;
          }
          const selectionIds = normalizeSelectableIds(ctx.selectedObjectIds, ctx.objects);
          const rootIds = selectionIds.includes(this.state.objectId)
            ? selectionIds
            : [this.state.objectId];
          const objectIds = expandTransformObjectIds(rootIds, ctx.objects);

          const initialStates = new Map<string, DragObjectState>();
          for (const id of objectIds) {
            const obj = ctx.objects.find((candidate) => candidate.id === id);
            if (!obj) continue;
            initialStates.set(id, {
              bounds: {
                min: { ...obj.bounds.min },
                max: { ...obj.bounds.max },
              },
            });
          }

          this.state = {
            type: 'dragging',
            startRawWorld: this.state.startRawWorld,
            startSnappedWorld: this.state.startSnappedWorld,
            objectIds,
            initialStates,
          };
        }
        break;
      }

      case 'dragging': {
        let dx = ctx.snapToObjects
          ? rawPt.x - this.state.startRawWorld.x
          : snappedPt.x - this.state.startSnappedWorld.x;
        let dy = ctx.snapToObjects
          ? rawPt.y - this.state.startRawWorld.y
          : snappedPt.y - this.state.startSnappedWorld.y;
        this.snapGuides = [];

        if (ctx.snapToObjects && this.state.objectIds.length > 0) {
          const combinedBounds = this.getCombinedBounds(dx, dy);
          if (combinedBounds) {
            const candidates = getSnapCandidates(ctx.objects, this.state.objectIds);
            const snapPx = useAppStore.getState().settings?.snap_threshold_px ?? SNAP_THRESHOLD_PX;
            const threshold = snapThresholdWorld(snapPx, ctx.vp.zoom);
            const snapResult = findBoundsSnap(combinedBounds, candidates, threshold);
            this.snapGuides = snapResult.guides;

            const currentCenterX = (combinedBounds.min.x + combinedBounds.max.x) / 2;
            const currentCenterY = (combinedBounds.min.y + combinedBounds.max.y) / 2;
            const snapDx = snapResult.snappedX - currentCenterX;
            const snapDy = snapResult.snappedY - currentCenterY;

            if (snapDx !== 0 || snapDy !== 0) {
              dx += snapDx;
              dy += snapDy;
            } else if (ctx.snapEnabled) {
              dx = snappedPt.x - this.state.startSnappedWorld.x;
              dy = snappedPt.y - this.state.startSnappedWorld.y;
            }
          }
        }

        this.applyDrag(ctx, dx, dy);
        ctx.requestRender();
        break;
      }
    }
  }

  onMouseUp(_e: CanvasMouseEvent, ctx: ToolContext): void {
    if (this.state.type === 'dragging') {
      for (const id of this.state.objectIds) {
        const obj = ctx.objects.find((candidate) => candidate.id === id);
        const initial = this.state.initialStates.get(id);
        if (!obj || !initial) continue;

        ctx.updateObject(id, {
          bounds: { min: { ...obj.bounds.min }, max: { ...obj.bounds.max } },
        });
      }
    }

    this.state = { type: 'idle' };
    this.snapGuides = [];
    ctx.requestRender();
  }

  getCursor(): string {
    return this.state.type === 'dragging' ? 'grabbing' : 'grab';
  }

  getOverlay(): ToolOverlay {
    if (this.state.type === 'dragging' && this.snapGuides.length > 0) {
      return { type: 'snap-guides', guides: this.snapGuides };
    }
    return { type: 'none' };
  }

  reset(): void {
    this.state = { type: 'idle' };
    this.snapGuides = [];
  }

  private getCombinedBounds(dx: number, dy: number): Bounds | null {
    if (this.state.type !== 'dragging') return null;

    let minX = Infinity;
    let minY = Infinity;
    let maxX = -Infinity;
    let maxY = -Infinity;

    for (const id of this.state.objectIds) {
      const initial = this.state.initialStates.get(id);
      if (!initial) continue;
      minX = Math.min(minX, initial.bounds.min.x + dx);
      minY = Math.min(minY, initial.bounds.min.y + dy);
      maxX = Math.max(maxX, initial.bounds.max.x + dx);
      maxY = Math.max(maxY, initial.bounds.max.y + dy);
    }

    if (!isFinite(minX)) return null;
    return { min: { x: minX, y: minY }, max: { x: maxX, y: maxY } };
  }

  private applyDrag(ctx: ToolContext, dx: number, dy: number): void {
    if (this.state.type !== 'dragging') return;

    for (const id of this.state.objectIds) {
      const obj = ctx.objects.find((candidate) => candidate.id === id);
      const initial = this.state.initialStates.get(id);
      if (!obj || !initial) continue;

      obj.bounds.min.x = initial.bounds.min.x + dx;
      obj.bounds.min.y = initial.bounds.min.y + dy;
      obj.bounds.max.x = initial.bounds.max.x + dx;
      obj.bounds.max.y = initial.bounds.max.y + dy;
    }
  }
}

function findParentGroupId(objectId: string, objects: ProjectObject[]): string | null {
  for (const object of objects) {
    if (object.data.type === 'group' && object.data.children.includes(objectId)) {
      return object.id;
    }
  }
  return null;
}

function topLevelSelectableObjectId(objectId: string, objects: ProjectObject[]): string {
  let current = objectId;
  const seen = new Set<string>();
  while (!seen.has(current)) {
    seen.add(current);
    const parent = findParentGroupId(current, objects);
    if (!parent) return current;
    current = parent;
  }
  return objectId;
}

function normalizeSelectableIds(ids: string[], objects: ProjectObject[]): string[] {
  const objectIds = new Set(objects.map((object) => object.id));
  const seen = new Set<string>();
  const normalized: string[] = [];
  for (const id of ids) {
    if (!objectIds.has(id)) continue;
    const selectableId = topLevelSelectableObjectId(id, objects);
    if (seen.has(selectableId)) continue;
    seen.add(selectableId);
    normalized.push(selectableId);
  }
  return normalized;
}

function collectGroupDescendantIds(
  objectId: string,
  objects: ProjectObject[],
  output: string[],
  seen: Set<string>,
): void {
  const object = objects.find((candidate) => candidate.id === objectId);
  if (!object || object.data.type !== 'group') return;
  for (const childId of object.data.children) {
    if (seen.has(childId)) continue;
    seen.add(childId);
    output.push(childId);
    collectGroupDescendantIds(childId, objects, output, seen);
  }
}

function expandTransformObjectIds(ids: string[], objects: ProjectObject[]): string[] {
  const expanded: string[] = [];
  const seen = new Set<string>();
  for (const id of normalizeSelectableIds(ids, objects)) {
    if (!seen.has(id)) {
      seen.add(id);
      expanded.push(id);
    }
    collectGroupDescendantIds(id, objects, expanded, seen);
  }
  return expanded;
}
