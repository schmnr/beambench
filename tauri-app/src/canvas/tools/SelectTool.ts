import type { CanvasTool, CanvasMouseEvent, ToolContext } from './types';
import type { ToolOverlay } from '../CanvasRenderer';
import type { HandleId } from '../../types/canvas';
import type { Point2D, Transform2D, Bounds, ProjectObject } from '../../types/project';
import { hitTestPoint, hitTestPointAll, hitTestRect, hitTestRectContained, hitTestHandle, hitTestSelectionEdge, hitTestSnapPoint } from '../hitTest';
import { DRAG_THRESHOLD_PX, SNAP_THRESHOLD_PX, ROTATION_SNAP_SHIFT_DEG, ROTATION_SNAP_CTRL_DEG } from '../constants';
import { computePointSnap, computeSelectionPivot, computeSelectionSnap, computeVisualBoundsWorld, getCombinedBounds, getObjectSnapPoints, getRulerGuideAxis, applyAroundCenter, type SnapLine } from '../alignment';
import { screenToWorldDist } from '../ViewportTransform';
import { useAppStore } from '../../stores/appStore';
import { useProjectStore } from '../../stores/projectStore';
import { useUiStore } from '../../stores/uiStore';
import { isTransformLocked, notifyObjectLocked, notifyTransformLocked } from '../../utils/transformLocks';
import { commitPendingTextEdit, isNewEmptyText } from '../textEditSession';
import { queryWorldBoundsCandidates } from '../sceneIndex';
import i18n from '../../i18n';

type SelectState =
  | { type: 'idle' }
  | { type: 'maybe-drag'; startScreen: Point2D; startWorld: Point2D; objectId: string; shiftKey: boolean; ctrlKey: boolean }
  | { type: 'dragging'; startWorld: Point2D; lastWorld: Point2D; objectIds: string[];
      origBounds: Map<string, Bounds>; snapOrigin?: Point2D }
  | { type: 'rubber-band'; startScreen: Point2D; currentScreen: Point2D; crossing: boolean }
  | { type: 'handle-drag'; handleId: HandleId; startWorld: Point2D; lastWorld: Point2D; objectIds: string[];
      origBounds: Map<string, Bounds>; origTransforms: Map<string, Transform2D>;
      sharedCenter: Point2D; selectionWidth: number; selectionHeight: number;
      origVisualSelBounds?: Bounds;
      startAngle?: number };

/**
 * Smallest allowed resize scale factor. Prevents handle drags past the
 * opposite edge from producing a negative scale (inverted bounds).
 */
const MIN_RESIZE_SCALE = 0.01;

type SelectionModifierMode = 'replace' | 'add' | 'toggle' | 'remove';

function selectionModifierMode(shiftKey: boolean, ctrlOrCmd: boolean): SelectionModifierMode {
  if (shiftKey && ctrlOrCmd) return 'remove';
  if (ctrlOrCmd) return 'toggle';
  if (shiftKey) return 'add';
  return 'replace';
}

function applySelectionModifierToOne(currentIds: string[], id: string, mode: SelectionModifierMode): string[] {
  switch (mode) {
    case 'add':
      return currentIds.includes(id) ? currentIds : [...currentIds, id];
    case 'toggle':
      return currentIds.includes(id) ? currentIds.filter((currentId) => currentId !== id) : [...currentIds, id];
    case 'remove':
      return currentIds.filter((currentId) => currentId !== id);
    case 'replace':
    default:
      return [id];
  }
}

function applySelectionModifierToMany(currentIds: string[], ids: string[], mode: SelectionModifierMode): string[] {
  switch (mode) {
    case 'add':
      return [...currentIds, ...ids.filter((id) => !currentIds.includes(id))];
    case 'toggle': {
      const idSet = new Set(ids);
      const kept = currentIds.filter((id) => !idSet.has(id));
      const added = ids.filter((id) => !currentIds.includes(id));
      return [...kept, ...added];
    }
    case 'remove': {
      const idSet = new Set(ids);
      return currentIds.filter((id) => !idSet.has(id));
    }
    case 'replace':
    default:
      return ids;
  }
}

export class SelectTool implements CanvasTool {
  name = 'select';
  private state: SelectState = { type: 'idle' };
  private snapGuides: SnapLine[] = [];
  private activeSnapTargetKey: string | null = null;
  private lastAltClickScreen: Point2D | null = null;
  private altCycleIndex = 0;
  private lastClickCycleScreen: Point2D | null = null;

  onMouseDown(e: CanvasMouseEvent, ctx: ToolContext): void {
    // Clear inline text editing on any click
    if (useUiStore.getState().textEditObjectId) {
      const prevId = useUiStore.getState().textEditObjectId;
      const prevMode = useUiStore.getState().textEditMode;
      const shouldDelete = isNewEmptyText(prevId, prevMode);
      void (async () => {
        const committed = await commitPendingTextEdit();
        if (!committed) return;
        useUiStore.setState({
          textEditObjectId: null,
          textEditClickPos: null,
          textEditMode: null,
          textEditCaretIndex: null,
        });
        if (shouldDelete && prevId) {
          await useProjectStore.getState().removeObject(prevId);
        }
      })();
      return;
    }

    const screenPt = { x: e.screenX, y: e.screenY };
    const worldPt = { x: e.worldX, y: e.worldY };

    // Check handles first (only if something is selected)
    const selectionIds = normalizeSelectableIds(ctx.selectedObjectIds, ctx.objects);
    const transformIds = expandTransformObjectIds(selectionIds, ctx.objects);
    const selectedObjects = selectionIds
      .map((id) => ctx.objects.find((o) => o.id === id))
      .filter(Boolean) as typeof ctx.objects;
    const selectionHasLockedObjects = selectedObjects.some((object) => object.locked);
    const handleId = selectionHasLockedObjects
      ? null
      : hitTestHandle(screenPt, selectedObjects, ctx.vp, ctx.transformLocks, ctx.objects);

    if (handleId) {
      // Determine the lock kind for this handle
      const isRotateHandle = handleId.startsWith('rotate_');
      const isShearHandle = handleId.startsWith('shear_');
      const isCenterHandle = handleId === 'center';
      const isResizeHandle = !isRotateHandle && !isShearHandle && !isCenterHandle;

      if (isRotateHandle && isTransformLocked(ctx.transformLocks, 'rotation')) {
        notifyTransformLocked('rotation');
        return;
      }
      if (isShearHandle && isTransformLocked(ctx.transformLocks, 'shear')) {
        notifyTransformLocked('shear');
        return;
      }
      if (isResizeHandle && isTransformLocked(ctx.transformLocks, 'scale')) {
        notifyTransformLocked('scale');
        return;
      }
      if (isCenterHandle && isTransformLocked(ctx.transformLocks, 'position')) {
        notifyTransformLocked('position');
        return;
      }

      if (isCenterHandle) {
        // Center handle → start move drag
        const origBounds = captureOrigBounds(ctx, transformIds);
        this.state = {
          type: 'dragging',
          startWorld: worldPt,
          lastWorld: ctx.snapToObjects ? { x: e.worldX, y: e.worldY } : worldPt,
          objectIds: transformIds,
          origBounds,
        };
        return;
      }

      // Capture originals for handle drag
      const origBounds = captureOrigBounds(ctx, transformIds);
      const origTransforms = new Map<string, Transform2D>();
      for (const id of transformIds) {
        const o = ctx.objects.find((ob) => ob.id === id);
        if (o) origTransforms.set(id, { ...o.transform });
      }

      const sharedCenter = computeSelectionPivot(selectedObjects, ctx.objects);

      // Compute selection visual bounds for shear ratio and resize reference
      let sMinX = Infinity, sMinY = Infinity, sMaxX = -Infinity, sMaxY = -Infinity;
      for (const obj of selectedObjects) {
        const vb = computeVisualBoundsWorld(obj, ctx.objects);
        sMinX = Math.min(sMinX, vb.min.x); sMinY = Math.min(sMinY, vb.min.y);
        sMaxX = Math.max(sMaxX, vb.max.x); sMaxY = Math.max(sMaxY, vb.max.y);
      }
      const selectionWidth = sMaxX - sMinX;
      const selectionHeight = sMaxY - sMinY;
      const origVisualSelBounds: Bounds = { min: { x: sMinX, y: sMinY }, max: { x: sMaxX, y: sMaxY } };

      if (isRotateHandle) {
        const startAngle = Math.atan2(worldPt.y - sharedCenter.y, worldPt.x - sharedCenter.x);
        this.state = {
          type: 'handle-drag',
          handleId,
          startWorld: worldPt,
          lastWorld: worldPt,
          objectIds: transformIds,
          origBounds,
          origTransforms,
          sharedCenter,
          selectionWidth,
          selectionHeight,
          origVisualSelBounds,
          startAngle,
        };
      } else {
        this.state = {
          type: 'handle-drag',
          handleId,
          startWorld: worldPt,
          lastWorld: worldPt,
          objectIds: transformIds,
          origBounds,
          origTransforms,
          sharedCenter,
          selectionWidth,
          selectionHeight,
          origVisualSelBounds,
        };
      }
      return;
    }

    // Check snap-point hit for snap-point drag (before general object hit)
    if (selectedObjects.length > 0) {
      const snapHit = selectionHasLockedObjects
        ? null
        : hitTestSnapPoint(screenPt, selectedObjects, ctx.vp, ctx.transformLocks, ctx.objects);
      if (snapHit) {
        const origBounds = captureOrigBounds(ctx, transformIds);
        this.state = {
          type: 'dragging',
          startWorld: worldPt,
          lastWorld: ctx.snapToObjects ? { x: e.worldX, y: e.worldY } : worldPt,
          objectIds: transformIds,
          origBounds,
          snapOrigin: snapHit.snapPoint,
        };
        return;
      }

      // Check edge hit for edge drag
      const edgeHit = selectionHasLockedObjects
        ? null
        : hitTestSelectionEdge(screenPt, selectedObjects, ctx.vp, ctx.transformLocks, ctx.objects);
      if (edgeHit) {
        const origBounds = captureOrigBounds(ctx, transformIds);
        this.state = {
          type: 'dragging',
          startWorld: worldPt,
          lastWorld: ctx.snapToObjects ? { x: e.worldX, y: e.worldY } : worldPt,
          objectIds: transformIds,
          origBounds,
        };
        return;
      }
    }

    // Check object hit
    const hit = hitTestPoint(screenPt, ctx.objects, ctx.vp, true);

    if (hit) {
      this.state = {
        type: 'maybe-drag',
        startScreen: screenPt,
        startWorld: worldPt,
        objectId: topLevelSelectableObjectId(hit.id, ctx.objects),
        shiftKey: e.shiftKey,
        ctrlKey: e.ctrlKey,
      };
    } else {
      // Start rubber-band
      if (!e.shiftKey && !e.ctrlKey) {
        ctx.selectObjects([]);
      }
      this.state = {
        type: 'rubber-band',
        startScreen: screenPt,
        currentScreen: screenPt,
        crossing: false,
      };
      ctx.requestRender();
    }
  }

  onMouseMove(e: CanvasMouseEvent, ctx: ToolContext): void {
    const screenPt = { x: e.screenX, y: e.screenY };
    const worldPt = { x: e.snappedX, y: e.snappedY };

    switch (this.state.type) {
      case 'maybe-drag': {
        const dx = screenPt.x - this.state.startScreen.x;
        const dy = screenPt.y - this.state.startScreen.y;
        if (Math.sqrt(dx * dx + dy * dy) > DRAG_THRESHOLD_PX) {
          if (isTransformLocked(ctx.transformLocks, 'position')) {
            notifyTransformLocked('position');
            return;
          }
          // Transition to dragging
          const { objectId, shiftKey, ctrlKey } = this.state;
          const mode = selectionModifierMode(shiftKey, ctrlKey);
          let dragSelectionIds: string[];
          const currentSelectionIds = normalizeSelectableIds(ctx.selectedObjectIds, ctx.objects);

          if (mode === 'replace') {
            if (!currentSelectionIds.includes(objectId)) {
              ctx.selectObjects([objectId]);
              dragSelectionIds = [objectId];
            } else {
              dragSelectionIds = [...currentSelectionIds];
            }
          } else {
            const nextSelectionIds = applySelectionModifierToOne(currentSelectionIds, objectId, mode);
            ctx.selectObjects(nextSelectionIds);
            if ((mode === 'toggle' && currentSelectionIds.includes(objectId)) || mode === 'remove') {
              this.state = { type: 'idle' };
              ctx.requestRender();
              break;
            }
            dragSelectionIds = nextSelectionIds;
          }
          if (selectionIncludesLockedObjects(dragSelectionIds, ctx.objects)) {
            notifyObjectLocked();
            this.state = { type: 'idle' };
            ctx.requestRender();
            break;
          }
          const dragIds = expandTransformObjectIds(dragSelectionIds, ctx.objects);

          const origBounds = new Map<string, Bounds>();
          for (const id of dragIds) {
            const o = ctx.objects.find((ob) => ob.id === id);
            if (o) origBounds.set(id, { min: { ...o.bounds.min }, max: { ...o.bounds.max } });
          }

          this.state = {
            type: 'dragging',
            startWorld: this.state.startWorld,
            lastWorld: ctx.snapToObjects ? { x: e.worldX, y: e.worldY } : worldPt,
            objectIds: dragIds,
            origBounds,
          };
        }
        break;
      }

      case 'dragging': {
        if (isTransformLocked(ctx.transformLocks, 'position')) {
          notifyTransformLocked('position');
          break;
        }
        const { objectIds } = this.state;
        const visibleLayerIds = new Set(
          ctx.layers.filter((l) => l.enabled && l.visible !== false).map((l) => l.id),
        );
        const selectedObjects = objectIds
          .map((id) => ctx.objects.find((o) => o.id === id))
          .filter(Boolean) as typeof ctx.objects;
        const guideAxis =
          selectedObjects.length === 1 ? getRulerGuideAxis(selectedObjects[0]) : null;

        this.snapGuides = [];

        // Pre-constrain cursor to nearest 45° axis when Shift is held,
        // then feed through normal snap logic so Shift composes with snapping/guides.
        let rawPt = { x: e.worldX, y: e.worldY };
        let gridPt = worldPt; // grid-snapped position
        if (e.shiftKey) {
          const totalDx = rawPt.x - this.state.startWorld.x;
          const totalDy = rawPt.y - this.state.startWorld.y;
          const angle = Math.atan2(totalDy, totalDx);
          const snapAngle = Math.round(angle / (Math.PI / 4)) * (Math.PI / 4);
          const axisDx = Math.cos(snapAngle);
          const axisDy = Math.sin(snapAngle);
          const proj = totalDx * axisDx + totalDy * axisDy;
          rawPt = {
            x: this.state.startWorld.x + proj * axisDx,
            y: this.state.startWorld.y + proj * axisDy,
          };
          gridPt = rawPt; // shift overrides grid snap
        }

        let moveDx: number;
        let moveDy: number;
        // Whether to run object-snap: enabled in settings, or Alt forces it on
        const useObjectSnap = ctx.snapToObjects || e.altKey;
        const snapPx = useAppStore.getState().settings?.snap_threshold_px ?? SNAP_THRESHOLD_PX;
        const thresholdMm = screenToWorldDist(e.altKey ? snapPx * 1.5 : snapPx, ctx.vp.zoom);

        if (e.ctrlKey) {
          // Ctrl/Cmd: skip all snapping (but shift constraint is already applied)
          moveDx = rawPt.x - this.state.lastWorld.x;
          moveDy = rawPt.y - this.state.lastWorld.y;
          this.state.lastWorld = rawPt;
          this.activeSnapTargetKey = null;
        } else if (this.state.snapOrigin && useObjectSnap) {
          // Snap-point drag: use point-to-point snapping
          moveDx = rawPt.x - this.state.lastWorld.x;
          moveDy = rawPt.y - this.state.lastWorld.y;
          this.state.lastWorld = rawPt;

          const tentativeSnapPt = {
            x: this.state.snapOrigin.x + (rawPt.x - this.state.startWorld.x),
            y: this.state.snapOrigin.y + (rawPt.y - this.state.startWorld.y),
          };
          const pointCandidates = queryWorldBoundsCandidates(
            {
              min: { x: tentativeSnapPt.x - thresholdMm, y: tentativeSnapPt.y - thresholdMm },
              max: { x: tentativeSnapPt.x + thresholdMm, y: tentativeSnapPt.y + thresholdMm },
            },
            ctx.objects,
          ).filter(
            (o) => !objectIds.includes(o.id) && visibleLayerIds.has(o.layer_id),
          );
          const snap = computePointSnap(tentativeSnapPt, pointCandidates, thresholdMm, ctx.objects);

          if (snap.snappedTo) {
            moveDx += snap.dx;
            moveDy += snap.dy;
            this.snapGuides = snap.guides;
            this.activeSnapTargetKey = null;
          }
        } else if (useObjectSnap) {
          // Standard object snapping (or Alt-forced)
          moveDx = rawPt.x - this.state.lastWorld.x;
          moveDy = rawPt.y - this.state.lastWorld.y;
          this.state.lastWorld = rawPt;
          // Use visual bounds for dragged objects (transform-aware, clone-aware)
          const tentativeBounds = selectedObjects.map((o) => {
            const vb = computeVisualBoundsWorld(o, ctx.objects);
            return {
              min: { x: vb.min.x + moveDx, y: vb.min.y + moveDy },
              max: { x: vb.max.x + moveDx, y: vb.max.y + moveDy },
            };
          });
          const combined = getCombinedBounds(tentativeBounds);
          const combinedCenter = { x: (combined.min.x + combined.max.x) / 2, y: (combined.min.y + combined.max.y) / 2 };
          const combinedAnchors = [
            combined.min,
            { x: combined.max.x, y: combined.min.y },
            combined.max,
            { x: combined.min.x, y: combined.max.y },
            { x: combinedCenter.x, y: combined.min.y },
            { x: combined.max.x, y: combinedCenter.y },
            { x: combinedCenter.x, y: combined.max.y },
            { x: combined.min.x, y: combinedCenter.y },
            combinedCenter,
          ];
          const movedAnchors = [
            ...combinedAnchors,
            ...selectedObjects.flatMap((obj) =>
              getObjectSnapPoints(obj, ctx.objects).map((pt) => ({
                x: pt.x + moveDx,
                y: pt.y + moveDy,
              })),
            ),
          ];
          const snapCandidates = queryWorldBoundsCandidates(
            {
              min: { x: combined.min.x - thresholdMm, y: combined.min.y - thresholdMm },
              max: { x: combined.max.x + thresholdMm, y: combined.max.y + thresholdMm },
            },
            ctx.objects,
          ).filter(
            (o) => !objectIds.includes(o.id) && visibleLayerIds.has(o.layer_id),
          );
          const snap = computeSelectionSnap(
            combined,
            movedAnchors,
            snapCandidates,
            thresholdMm,
            ctx.objects,
            {
              preferredTargetKey: e.altKey ? this.activeSnapTargetKey : null,
              preferredReleaseMultiplier: e.altKey ? 2.1 : 1.8,
            },
          );
          if (snap) {
            moveDx += snap.dx;
            moveDy += snap.dy;
            this.snapGuides = snap.guides;
            this.activeSnapTargetKey = snap.targetKey;
          } else if (ctx.snapEnabled) {
            moveDx = gridPt.x - (rawPt.x - moveDx);
            moveDy = gridPt.y - (rawPt.y - moveDy);
            this.activeSnapTargetKey = null;
          }
        } else {
          // Standard behavior: use grid-snapped (or shift-constrained) coordinates
          moveDx = gridPt.x - this.state.lastWorld.x;
          moveDy = gridPt.y - this.state.lastWorld.y;
          this.state.lastWorld = gridPt;
          this.activeSnapTargetKey = null;
        }

        if (guideAxis === 'vertical') {
          moveDy = 0;
        } else if (guideAxis === 'horizontal') {
          moveDx = 0;
        }
        const workspace = ctx.workspace ?? { bed_width_mm: Infinity, bed_height_mm: Infinity };

        // Update bounds for all objects
        for (const id of objectIds) {
          const obj = ctx.objects.find((o) => o.id === id);
          if (obj) {
            if (guideAxis === 'vertical') {
              const width = obj.bounds.max.x - obj.bounds.min.x;
              const nextX = Math.max(0, Math.min(workspace.bed_width_mm, obj.bounds.min.x + moveDx));
              obj.bounds.min.x = nextX;
              obj.bounds.max.x = nextX + width;
            } else if (guideAxis === 'horizontal') {
              const height = obj.bounds.max.y - obj.bounds.min.y;
              const nextY = Math.max(0, Math.min(workspace.bed_height_mm, obj.bounds.min.y + moveDy));
              obj.bounds.min.y = nextY;
              obj.bounds.max.y = nextY + height;
            } else {
              obj.bounds.min.x += moveDx;
              obj.bounds.min.y += moveDy;
              obj.bounds.max.x += moveDx;
              obj.bounds.max.y += moveDy;
            }
          }
        }
        ctx.requestRender();
        break;
      }

      case 'rubber-band':
        this.state.currentScreen = screenPt;
        this.state.crossing = screenPt.x < this.state.startScreen.x;
        ctx.requestRender();
        break;

      case 'handle-drag': {
        const { handleId, objectIds: hIds, origBounds, origTransforms, sharedCenter } = this.state;
        const isRotateHandle = handleId.startsWith('rotate_');
        const isShearHandle = handleId.startsWith('shear_');
        const isResizeHandle = !isRotateHandle && !isShearHandle;

        if (isRotateHandle) {
          if (isTransformLocked(ctx.transformLocks, 'rotation')) break;

          const rawWorldPt = { x: e.worldX, y: e.worldY };
          const currentAngle = Math.atan2(rawWorldPt.y - sharedCenter.y, rawWorldPt.x - sharedCenter.x);
          let deltaDeg = (currentAngle - this.state.startAngle!) * (180 / Math.PI);

          // Snap rotation
          if (e.shiftKey) {
            deltaDeg = Math.round(deltaDeg / ROTATION_SNAP_SHIFT_DEG) * ROTATION_SNAP_SHIFT_DEG;
          } else if (e.ctrlKey) {
            deltaDeg = Math.round(deltaDeg / ROTATION_SNAP_CTRL_DEG) * ROTATION_SNAP_CTRL_DEG;
          }

          const deltaRad = deltaDeg * (Math.PI / 180);
          const cosA = Math.cos(deltaRad);
          const sinA = Math.sin(deltaRad);

          // Live preview: orbit + self-rotation
          for (const id of hIds) {
            const obj = ctx.objects.find((o) => o.id === id);
            const ob = origBounds.get(id);
            const ot = origTransforms.get(id);
            if (!obj || !ob || !ot) continue;

            const bc = { x: (ob.min.x + ob.max.x) / 2, y: (ob.min.y + ob.max.y) / 2 };
            const tc = applyAroundCenter(ot, bc, bc);

            // Orbit
            const dx = tc.x - sharedCenter.x;
            const dy = tc.y - sharedCenter.y;
            const newX = sharedCenter.x + dx * cosA - dy * sinA;
            const newY = sharedCenter.y + dx * sinA + dy * cosA;
            const shiftX = newX - tc.x;
            const shiftY = newY - tc.y;

            obj.bounds.min.x = ob.min.x + shiftX;
            obj.bounds.min.y = ob.min.y + shiftY;
            obj.bounds.max.x = ob.max.x + shiftX;
            obj.bounds.max.y = ob.max.y + shiftY;

            // Self-rotation: compose rotate * origTransform
            const rotT = rotateTransform(deltaRad);
            obj.transform = composeTransforms(rotT, ot);
          }

          ctx.setStatusMessage(i18n.t('canvas_status.rotate', { deg: deltaDeg.toFixed(1) }));
          ctx.requestRender();
        } else if (isShearHandle) {
          if (isTransformLocked(ctx.transformLocks, 'shear')) break;

          const rawWorldPt = { x: e.worldX, y: e.worldY };
          const totalDx = rawWorldPt.x - this.state.startWorld.x;
          const totalDy = rawWorldPt.y - this.state.startWorld.y;

          let shearX = 0, shearY = 0;
          if (handleId === 'shear_n' && this.state.selectionHeight > 0) {
            shearX = -totalDx / this.state.selectionHeight;
          } else if (handleId === 'shear_e' && this.state.selectionWidth > 0) {
            shearY = totalDy / this.state.selectionWidth;
          }

          // Live preview: orbit + self-shear
          for (const id of hIds) {
            const obj = ctx.objects.find((o) => o.id === id);
            const ob = origBounds.get(id);
            const ot = origTransforms.get(id);
            if (!obj || !ob || !ot) continue;

            const bc = { x: (ob.min.x + ob.max.x) / 2, y: (ob.min.y + ob.max.y) / 2 };
            const tc = applyAroundCenter(ot, bc, bc);

            // Orbit with shear matrix
            const dx = tc.x - sharedCenter.x;
            const dy = tc.y - sharedCenter.y;
            const newDx = dx + shearX * dy;
            const newDy = shearY * dx + dy;
            const shiftX = (sharedCenter.x + newDx) - tc.x;
            const shiftY = (sharedCenter.y + newDy) - tc.y;

            obj.bounds.min.x = ob.min.x + shiftX;
            obj.bounds.min.y = ob.min.y + shiftY;
            obj.bounds.max.x = ob.max.x + shiftX;
            obj.bounds.max.y = ob.max.y + shiftY;

            // Self-shear
            const shearT: Transform2D = { a: 1, b: shearY, c: shearX, d: 1, tx: 0, ty: 0 };
            obj.transform = composeTransforms(shearT, ot);
          }

          ctx.setStatusMessage(i18n.t('canvas_status.shear', { x: shearX.toFixed(3), y: shearY.toFixed(3) }));
          ctx.requestRender();
        } else if (isResizeHandle) {
          if (isTransformLocked(ctx.transformLocks, 'scale')) break;

          // Selection-level resize using visual selection bounds (matches handle positions)
          const rawWorldPt = { x: e.worldX, y: e.worldY };
          const totalDx = rawWorldPt.x - this.state.startWorld.x;
          const totalDy = rawWorldPt.y - this.state.startWorld.y;

          // Use the snapshotted visual selection bounds (same box the handles are drawn from)
          const vsb = this.state.origVisualSelBounds!;
          const selMinX = vsb.min.x, selMinY = vsb.min.y;
          const selMaxX = vsb.max.x, selMaxY = vsb.max.y;

          const origSelW = selMaxX - selMinX;
          const origSelH = selMaxY - selMinY;

          // Determine anchor (opposite corner/edge)
          let anchorX: number, anchorY: number;
          let newSelMinX = selMinX, newSelMinY = selMinY, newSelMaxX = selMaxX, newSelMaxY = selMaxY;

          if (e.ctrlKey) {
            // Resize from center
            anchorX = (selMinX + selMaxX) / 2;
            anchorY = (selMinY + selMaxY) / 2;
          } else {
            anchorX = handleId.endsWith('w') || handleId === 'w' ? selMaxX : selMinX;
            anchorY = handleId.startsWith('n') || handleId === 'n' ? selMaxY : selMinY;
            if (handleId === 'e' || handleId === 'w') anchorY = selMinY; // edge handles: anchor opposite edge
            if (handleId === 'n' || handleId === 's') anchorX = selMinX;
          }

          // Apply delta to the appropriate edges
          switch (handleId) {
            case 'nw': newSelMinX += totalDx; newSelMinY += totalDy; break;
            case 'n':  newSelMinY += totalDy; break;
            case 'ne': newSelMaxX += totalDx; newSelMinY += totalDy; break;
            case 'w':  newSelMinX += totalDx; break;
            case 'e':  newSelMaxX += totalDx; break;
            case 'sw': newSelMinX += totalDx; newSelMaxY += totalDy; break;
            case 's':  newSelMaxY += totalDy; break;
            case 'se': newSelMaxX += totalDx; newSelMaxY += totalDy; break;
          }

          if (e.ctrlKey) {
            // Symmetric resize from center
            const halfDx = (newSelMaxX - newSelMinX - origSelW) / 2;
            const halfDy = (newSelMaxY - newSelMinY - origSelH) / 2;
            newSelMinX = selMinX - halfDx;
            newSelMaxX = selMaxX + halfDx;
            newSelMinY = selMinY - halfDy;
            newSelMaxY = selMaxY + halfDy;
          }

          let newSelW = newSelMaxX - newSelMinX;
          let newSelH = newSelMaxY - newSelMinY;

          // Proportional constraint for corner handles
          const isCorner = ['nw', 'ne', 'sw', 'se'].includes(handleId);
          if (isCorner && !e.shiftKey && origSelW > 0 && origSelH > 0) {
            const aspect = origSelW / origSelH;
            if (Math.abs(newSelW - origSelW) / origSelW >= Math.abs(newSelH - origSelH) / origSelH) {
              newSelH = newSelW / aspect;
            } else {
              newSelW = newSelH * aspect;
            }
            // Re-derive edges based on anchor
            if (e.ctrlKey) {
              const cX = (selMinX + selMaxX) / 2;
              const cY = (selMinY + selMaxY) / 2;
              newSelMinX = cX - newSelW / 2;
              newSelMaxX = cX + newSelW / 2;
              newSelMinY = cY - newSelH / 2;
              newSelMaxY = cY + newSelH / 2;
            } else {
              if (handleId.endsWith('w') || handleId === 'w') {
                newSelMinX = newSelMaxX - newSelW;
              } else {
                newSelMaxX = newSelMinX + newSelW;
              }
              if (handleId.startsWith('n') || handleId === 'n') {
                newSelMinY = newSelMaxY - newSelH;
              } else {
                newSelMaxY = newSelMinY + newSelH;
              }
            }
          }

          // Scale factors derived from visual selection box. Clamp to a small
          // positive minimum so dragging a handle past the opposite edge can
          // never produce a negative scale (which would invert bounds, min > max,
          // and commit inverted bounds to the backend). No flip behavior: the
          // selection simply stops shrinking at a minimal size. The commit path
          // in onMouseUp reuses these previewed bounds, so it is clamped too.
          const scaleX = Math.max(origSelW > 0 ? (newSelMaxX - newSelMinX) / origSelW : 1, MIN_RESIZE_SCALE);
          const scaleY = Math.max(origSelH > 0 ? (newSelMaxY - newSelMinY) / origSelH : 1, MIN_RESIZE_SCALE);
          const aX = e.ctrlKey ? (selMinX + selMaxX) / 2 : anchorX;
          const aY = e.ctrlKey ? (selMinY + selMaxY) / 2 : anchorY;

          // Apply scale to raw object bounds (position relative to visual-bounds anchor)
          for (const id of hIds) {
            const obj = ctx.objects.find((o) => o.id === id);
            const ob = origBounds.get(id);
            if (!obj || !ob) continue;

            obj.bounds.min.x = aX + (ob.min.x - aX) * scaleX;
            obj.bounds.min.y = aY + (ob.min.y - aY) * scaleY;
            obj.bounds.max.x = aX + (ob.max.x - aX) * scaleX;
            obj.bounds.max.y = aY + (ob.max.y - aY) * scaleY;
          }

          ctx.requestRender();
        }
        break;
      }
    }
  }

  onMouseUp(e: CanvasMouseEvent, ctx: ToolContext): void {
    switch (this.state.type) {
      case 'maybe-drag': {
        // Click-select
        const { objectId, shiftKey, ctrlKey } = this.state;
        const mode = selectionModifierMode(shiftKey, ctrlKey);
        const screenPt = { x: e.screenX, y: e.screenY };

        if (e.altKey && mode === 'replace') {
          // Alt+click: cycle through overlapping objects
          const hits = normalizeHitObjects(hitTestPointAll(screenPt, ctx.objects, ctx.vp, true), ctx.objects);
          if (hits.length > 1) {
            const sameSpot = this.lastAltClickScreen != null &&
              Math.abs(screenPt.x - this.lastAltClickScreen.x) <= 2 &&
              Math.abs(screenPt.y - this.lastAltClickScreen.y) <= 2;
            if (sameSpot) {
              this.altCycleIndex = (this.altCycleIndex + 1) % hits.length;
            } else {
              this.altCycleIndex = 1;
            }
            this.lastAltClickScreen = screenPt;
            ctx.selectObjects([hits[this.altCycleIndex].id]);
            break;
          }
        }

        this.lastAltClickScreen = null;
        this.altCycleIndex = 0;

        if (mode === 'replace') {
          const hits = normalizeHitObjects(hitTestPointAll(screenPt, ctx.objects, ctx.vp, true), ctx.objects);
          const currentSelectionIds = normalizeSelectableIds(ctx.selectedObjectIds, ctx.objects);
          const selectedHitIndex = hits.findIndex((hit) => currentSelectionIds.includes(hit.id));
          const sameSpot = this.lastClickCycleScreen != null &&
            Math.abs(screenPt.x - this.lastClickCycleScreen.x) <= 2 &&
            Math.abs(screenPt.y - this.lastClickCycleScreen.y) <= 2;
          const shouldCycle = hits.length > 1 && selectedHitIndex >= 0 && (sameSpot || selectedHitIndex === 0);
          if (shouldCycle) {
            const nextHitIndex = sameSpot
              ? (selectedHitIndex + 1) % hits.length
              : 1;
            this.lastClickCycleScreen = screenPt;
            ctx.selectObjects([hits[nextHitIndex].id]);
            break;
          }

          this.lastClickCycleScreen = screenPt;
        } else {
          this.lastClickCycleScreen = null;
        }

        const currentSelectionIds = normalizeSelectableIds(ctx.selectedObjectIds, ctx.objects);
        ctx.selectObjects(applySelectionModifierToOne(currentSelectionIds, objectId, mode));
        break;
      }

      case 'dragging': {
        // Commit position — batch bounds update for atomic undo. Keep the
        // mutated frontend bounds visible (optimistic UI) while the async
        // commit + project refetch round-trips through the backend. Reverting
        // to the pre-drag bounds here caused a visible flash on slow objects
        // (e.g. complex SVGs) during the ~100-300 ms until the refetch lands.
        // If the commit fails, the refetch will restore the backend's state.
        const { objectIds: dragIds } = this.state;
        const entries: { id: string; bounds: Bounds }[] = [];
        for (const id of dragIds) {
          const obj = ctx.objects.find((o) => o.id === id);
          if (obj) {
            entries.push({ id, bounds: { min: { ...obj.bounds.min }, max: { ...obj.bounds.max } } });
          }
        }
        if (entries.length > 0) {
          void ctx.updateObjectBoundsBatch(entries);
        }
        break;
      }

      case 'rubber-band': {
        const { startScreen, currentScreen, crossing } = this.state;
        const rect = {
          min: {
            x: Math.min(startScreen.x, currentScreen.x),
            y: Math.min(startScreen.y, currentScreen.y),
          },
          max: {
            x: Math.max(startScreen.x, currentScreen.x),
            y: Math.max(startScreen.y, currentScreen.y),
          },
        };
        const hits = crossing
          ? hitTestRect(rect, ctx.objects, ctx.vp, true)
          : hitTestRectContained(rect, ctx.objects, ctx.vp, true);
        const ids = normalizeSelectableIds(
          orderMultiSelectBatchForAnchor(hits.map((o) => o.id), ctx.objects),
          ctx.objects,
        );

        const mode = selectionModifierMode(e.shiftKey, e.ctrlKey);
        const currentSelectionIds = normalizeSelectableIds(ctx.selectedObjectIds, ctx.objects);
        ctx.selectObjects(applySelectionModifierToMany(currentSelectionIds, ids, mode));
        break;
      }

      case 'handle-drag': {
        const { handleId, objectIds: hIds, origBounds, origTransforms, sharedCenter } = this.state;
        const isRotateHandle = handleId.startsWith('rotate_');
        const isShearHandle = handleId.startsWith('shear_');

        if (isRotateHandle) {
          // Restore originals first
          for (const id of hIds) {
            const obj = ctx.objects.find((o) => o.id === id);
            const ob = origBounds.get(id);
            const ot = origTransforms.get(id);
            if (obj && ob && ot) {
              obj.bounds = { min: { ...ob.min }, max: { ...ob.max } };
              obj.transform = { ...ot };
            }
          }

          // Compute final rotation angle
          const rawWorldPt = { x: e.worldX, y: e.worldY };
          const currentAngle = Math.atan2(rawWorldPt.y - sharedCenter.y, rawWorldPt.x - sharedCenter.x);
          let deltaDeg = (currentAngle - this.state.startAngle!) * (180 / Math.PI);

          if (e.shiftKey) {
            deltaDeg = Math.round(deltaDeg / ROTATION_SNAP_SHIFT_DEG) * ROTATION_SNAP_SHIFT_DEG;
          } else if (e.ctrlKey) {
            deltaDeg = Math.round(deltaDeg / ROTATION_SNAP_CTRL_DEG) * ROTATION_SNAP_CTRL_DEG;
          }

          if (Math.abs(deltaDeg) > 0.1) {
            void ctx.rotateObjects(hIds, deltaDeg, sharedCenter);
          }
        } else if (isShearHandle) {
          // Restore originals first
          for (const id of hIds) {
            const obj = ctx.objects.find((o) => o.id === id);
            const ob = origBounds.get(id);
            const ot = origTransforms.get(id);
            if (obj && ob && ot) {
              obj.bounds = { min: { ...ob.min }, max: { ...ob.max } };
              obj.transform = { ...ot };
            }
          }

          // Compute final shear values
          const rawWorldPt = { x: e.worldX, y: e.worldY };
          const totalDx = rawWorldPt.x - this.state.startWorld.x;
          const totalDy = rawWorldPt.y - this.state.startWorld.y;

          let shearX = 0, shearY = 0;
          if (handleId === 'shear_n' && this.state.selectionHeight > 0) {
            shearX = -totalDx / this.state.selectionHeight;
          } else if (handleId === 'shear_e' && this.state.selectionWidth > 0) {
            shearY = totalDy / this.state.selectionWidth;
          }

          if (Math.abs(shearX) > 0.001 || Math.abs(shearY) > 0.001) {
            void ctx.shearObjects(hIds, shearX, shearY, sharedCenter);
          }
        } else {
          // Resize commit: use batch bounds update with exact preview bounds.
          // Keep mutated bounds visible during the async round-trip to avoid
          // the post-resize flash. A refetch after the commit will reconcile
          // (or revert, on failure).
          const entries: { id: string; bounds: Bounds }[] = [];
          for (const id of hIds) {
            const obj = ctx.objects.find((o) => o.id === id);
            if (obj) {
              entries.push({ id, bounds: { min: { ...obj.bounds.min }, max: { ...obj.bounds.max } } });
            }
          }

          if (entries.length > 0) {
            void ctx.updateObjectBoundsBatch(entries);
          }
        }
        break;
      }
    }

    this.state = { type: 'idle' };
    this.snapGuides = [];
    this.activeSnapTargetKey = null;
    ctx.setStatusMessage('');
    ctx.requestRender();
  }

  onDoubleClick(e: CanvasMouseEvent, ctx: ToolContext): void {
    void (async () => {
      const screenPt = { x: e.screenX, y: e.screenY };
      const hit = hitTestPoint(screenPt, ctx.objects, ctx.vp);
      if (hit && hit.data.type === 'text') {
        if (useUiStore.getState().textEditObjectId && useUiStore.getState().textEditObjectId !== hit.id) {
          const prevId = useUiStore.getState().textEditObjectId;
          const prevMode = useUiStore.getState().textEditMode;
          const shouldDelete = isNewEmptyText(prevId, prevMode);
          const committed = await commitPendingTextEdit();
          if (!committed) return;
          useUiStore.setState({
            textEditObjectId: null,
            textEditClickPos: null,
            textEditMode: null,
            textEditCaretIndex: null,
          });
          if (shouldDelete && prevId) {
            await useProjectStore.getState().removeObject(prevId);
          }
        }
        ctx.selectObjects([hit.id]);
        useUiStore.getState().beginTextEditSession(hit.id, 'double-click');
      } else if (hit && hit.data.type === 'raster_image') {
        ctx.selectObjects([hit.id]);
        const obj = ctx.objects.find((o) => o.id === hit.id);
        if (obj) {
          useProjectStore.getState().selectLayer(obj.layer_id);
          const ui = useUiStore.getState();
          // Ensure side panels are globally visible — user may have collapsed them
          if (!ui.sidePanelsVisible) {
            ui.toggleSidePanels();
          }
          // Ensure Properties panel itself is not individually hidden
          if (useUiStore.getState().panelLayout.hiddenPanelIds.includes('properties')) {
            useUiStore.getState().togglePanelVisibility('properties');
          }
          useUiStore.getState().setZoneActiveTab('upper-right', 'properties');
        }
      }
    })();
  }

  onKeyDown(e: KeyboardEvent, ctx: ToolContext): void {
    if (e.key === 'Escape') {
      // Cancel any in-progress transform, restore originals, deselect
      this.cancelDrag(ctx);
      ctx.selectObjects([]);
    }
  }

  /**
   * Cancel any in-progress drag (move, handle resize/rotate/shear, rubber-band),
   * restoring the original bounds/transforms. Unlike Escape this does not change
   * the selection — used by the canvas when a right-click (context menu) arrives
   * mid-drag, so the half-applied drag never stays live or gets committed.
   * Returns true when an active drag was cancelled.
   */
  cancelDrag(ctx: ToolContext): boolean {
    const hadActiveDrag =
      this.state.type === 'dragging' ||
      this.state.type === 'handle-drag' ||
      this.state.type === 'rubber-band';

    if (this.state.type === 'handle-drag') {
      const { objectIds: hIds, origBounds, origTransforms } = this.state;
      for (const id of hIds) {
        const obj = ctx.objects.find((o) => o.id === id);
        const ob = origBounds.get(id);
        const ot = origTransforms.get(id);
        if (obj && ob) {
          obj.bounds = { min: { ...ob.min }, max: { ...ob.max } };
        }
        if (obj && ot) {
          obj.transform = { ...ot };
        }
      }
    } else if (this.state.type === 'dragging') {
      const { objectIds: dIds, origBounds } = this.state;
      for (const id of dIds) {
        const obj = ctx.objects.find((o) => o.id === id);
        const ob = origBounds.get(id);
        if (obj && ob) {
          obj.bounds = { min: { ...ob.min }, max: { ...ob.max } };
        }
      }
    }

    this.state = { type: 'idle' };
    this.snapGuides = [];
    this.activeSnapTargetKey = null;
    ctx.setStatusMessage('');
    ctx.requestRender();
    return hadActiveDrag;
  }

  getCursor(_ctx: ToolContext): string {
    switch (this.state.type) {
      case 'dragging':
        return 'move';
      case 'rubber-band':
        return 'crosshair';
      case 'handle-drag':
        return getHandleCursor(this.state.handleId);
      default:
        return 'default';
    }
  }

  getOverlay(): ToolOverlay {
    if (this.state.type === 'rubber-band') {
      return {
        type: 'rubber-band',
        startScreen: this.state.startScreen,
        endScreen: this.state.currentScreen,
        crossing: this.state.crossing,
      };
    }
    if (this.snapGuides.length > 0) {
      return { type: 'snap-guides', guides: this.snapGuides };
    }
    return { type: 'none' };
  }

  reset(): void {
    this.state = { type: 'idle' };
    this.snapGuides = [];
    this.activeSnapTargetKey = null;
    this.lastAltClickScreen = null;
    this.altCycleIndex = 0;
  }
}

/** Exported for testing. */
export function computeResizedBounds(
  orig: { min: Point2D; max: Point2D },
  handleId: HandleId,
  totalDx: number,
  totalDy: number,
  proportional: boolean,
): { min: Point2D; max: Point2D } {
  const newMin = { ...orig.min };
  const newMax = { ...orig.max };

  // Apply raw delta per handle
  switch (handleId) {
    case 'nw': newMin.x += totalDx; newMin.y += totalDy; break;
    case 'n':  newMin.y += totalDy; break;
    case 'ne': newMax.x += totalDx; newMin.y += totalDy; break;
    case 'w':  newMin.x += totalDx; break;
    case 'e':  newMax.x += totalDx; break;
    case 'sw': newMin.x += totalDx; newMax.y += totalDy; break;
    case 's':  newMax.y += totalDy; break;
    case 'se': newMax.x += totalDx; newMax.y += totalDy; break;
  }

  const origW = orig.max.x - orig.min.x;
  const origH = orig.max.y - orig.min.y;
  const isCorner = ['nw', 'ne', 'sw', 'se'].includes(handleId);

  if (isCorner && proportional && origW > 0 && origH > 0) {
    const newW = newMax.x - newMin.x;
    const newH = newMax.y - newMin.y;
    const aspect = origW / origH;

    if (Math.abs(newW - origW) / origW >= Math.abs(newH - origH) / origH) {
      const targetH = newW / aspect;
      if (handleId.startsWith('n')) {
        newMin.y = newMax.y - targetH;
      } else {
        newMax.y = newMin.y + targetH;
      }
    } else {
      const targetW = newH * aspect;
      if (handleId.endsWith('w')) {
        newMin.x = newMax.x - targetW;
      } else {
        newMax.x = newMin.x + targetW;
      }
    }
  }

  return { min: newMin, max: newMax };
}

function getHandleCursor(handleId: HandleId): string {
  switch (handleId) {
    case 'nw': case 'se': return 'nwse-resize';
    case 'ne': case 'sw': return 'nesw-resize';
    case 'n': case 's': return 'ns-resize';
    case 'e': case 'w': return 'ew-resize';
    case 'rotate_nw': case 'rotate_ne': case 'rotate_sw': case 'rotate_se': return 'grab';
    case 'center': return 'move';
    case 'shear_n': return 'ew-resize';
    case 'shear_e': return 'ns-resize';
    default: return 'default';
  }
}

// --- Helpers ---

function captureOrigBounds(ctx: ToolContext, objectIds = ctx.selectedObjectIds): Map<string, Bounds> {
  const origBounds = new Map<string, Bounds>();
  for (const id of objectIds) {
    const o = ctx.objects.find((ob) => ob.id === id);
    if (o) origBounds.set(id, { min: { ...o.bounds.min }, max: { ...o.bounds.max } });
  }
  return origBounds;
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

function normalizeHitObjects(hits: ProjectObject[], objects: ProjectObject[]): ProjectObject[] {
  const byId = new Map(objects.map((object) => [object.id, object]));
  return normalizeSelectableIds(hits.map((hit) => hit.id), objects)
    .map((id) => byId.get(id))
    .filter(Boolean) as ProjectObject[];
}

function selectionIncludesLockedObjects(ids: string[], objects: ProjectObject[]): boolean {
  const selected = new Set(normalizeSelectableIds(ids, objects));
  return objects.some((object) => selected.has(object.id) && object.locked);
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

function orderMultiSelectBatchForAnchor(ids: string[], objects: ProjectObject[]): string[] {
  const drawOrder = new Map(objects.map((object, index) => [object.id, index]));
  // The selection anchor is the final selectedObjectIds entry. When one
  // rubber-band step adds multiple objects, so use the first object in
  // draw order as that anchor, so we append the batch in reverse draw order.
  return [...ids].sort((a, b) => (drawOrder.get(b) ?? -1) - (drawOrder.get(a) ?? -1));
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
