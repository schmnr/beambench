import type { CanvasMouseEvent, CanvasTool, ToolContext } from './types';
import type { ToolOverlay } from '../CanvasRenderer';
import type { Bounds, Point2D, ProjectObject } from '../../types/project';
import { worldToScreen } from '../ViewportTransform';
import { computeVisualBoundsWorld } from '../alignment';
import { vectorService } from '../../services/vectorService';
import { useNotificationStore } from '../../stores/notificationStore';
import { usePreviewStore } from '../../stores/previewStore';
import { useProjectStore } from '../../stores/projectStore';
import { useUndoStore } from '../../stores/undoStore';
import { useUiStore, type MeshDeformMode } from '../../stores/uiStore';
import { resolveEffectiveData } from '../../commands/selectionContext';
import i18n from '../../i18n';

const HANDLE_HIT_PX = 10;
const COMPATIBLE_TYPES = new Set(['vector_path', 'shape', 'text', 'polygon', 'star', 'raster_image', 'barcode']);

type MeshHandle = {
  worldX: number;
  worldY: number;
  active?: boolean;
  hovered?: boolean;
};

type ToolState =
  | { type: 'idle' }
  | {
      type: 'dragging';
      handleIndex: number;
      startWorld: Point2D;
      originalHandles: Point2D[];
      mode: MeshDeformMode;
      moved: boolean;
    };

function isCompatibleObject(object: ProjectObject, allObjects: ProjectObject[]): boolean {
  const data = resolveEffectiveData(object, allObjects);
  return data !== null && COMPATIBLE_TYPES.has(data.type);
}

function collectEditableSelection(ctx: ToolContext): string[] | null {
  if (ctx.selectedObjectIds.length === 0) return null;

  const ids: string[] = [];
  for (const id of ctx.selectedObjectIds) {
    const object = ctx.objects.find((candidate) => candidate.id === id);
    if (!object || object.locked || !isCompatibleObject(object, ctx.objects)) {
      return null;
    }
    ids.push(id);
  }
  return ids;
}

function combinedSelectionBounds(ctx: ToolContext, ids: string[]): Bounds | null {
  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;

  for (const id of ids) {
    const object = ctx.objects.find((candidate) => candidate.id === id);
    if (!object) continue;
    const bounds = computeVisualBoundsWorld(object, ctx.objects);
    minX = Math.min(minX, bounds.min.x);
    minY = Math.min(minY, bounds.min.y);
    maxX = Math.max(maxX, bounds.max.x);
    maxY = Math.max(maxY, bounds.max.y);
  }

  if (!Number.isFinite(minX) || maxX - minX <= 1e-6 || maxY - minY <= 1e-6) {
    return null;
  }

  return { min: { x: minX, y: minY }, max: { x: maxX, y: maxY } };
}

function buildGrid(bounds: Bounds, gridSize: number): Point2D[] {
  const handles: Point2D[] = [];
  for (let row = 0; row < gridSize; row++) {
    const y = bounds.min.y + (bounds.max.y - bounds.min.y) * (row / (gridSize - 1));
    for (let col = 0; col < gridSize; col++) {
      const x = bounds.min.x + (bounds.max.x - bounds.min.x) * (col / (gridSize - 1));
      handles.push({ x, y });
    }
  }
  return handles;
}

function samePoint(a: Point2D, b: Point2D): boolean {
  return Math.hypot(a.x - b.x, a.y - b.y) <= 1e-6;
}

export class SelectionMeshDeformTool implements CanvasTool {
  name = 'warp';
  private state: ToolState = { type: 'idle' };
  private selectionKey: string | null = null;
  private sourceBounds: Bounds | null = null;
  private handles: Point2D[] = [];
  private hoveredIndex: number | null = null;
  private activeIndex: number | null = null;

  onMouseDown(e: CanvasMouseEvent, ctx: ToolContext): void {
    if (e.button !== 0) return;
    if (!this.syncGrid(ctx)) {
      this.reportUnavailable(ctx);
      return;
    }

    const hit = this.hitTestHandle({ x: e.screenX, y: e.screenY }, ctx);
    if (hit === null) {
      ctx.setStatusMessage(i18n.t('canvas_status.mesh_drag_handle', { label: this.labelForMode(this.currentMode()) }));
      return;
    }

    this.activeIndex = hit;
    this.hoveredIndex = hit;
    const mode = this.currentMode();
    this.state = {
      type: 'dragging',
      handleIndex: hit,
      startWorld: { x: this.handles[hit].x, y: this.handles[hit].y },
      originalHandles: this.handles.map((handle) => ({ ...handle })),
      mode,
      moved: false,
    };
    ctx.setStatusMessage(i18n.t('canvas_status.mesh_release_apply', { label: this.labelForMode(mode) }));
    ctx.requestRender();
  }

  onMouseMove(e: CanvasMouseEvent, ctx: ToolContext): void {
    if (!this.syncGrid(ctx)) return;

    if (this.state.type === 'dragging') {
      const next = { x: e.snappedX, y: e.snappedY };
      this.state.moved = this.state.moved || !samePoint(next, this.state.startWorld);
      this.setHandle(this.state.handleIndex, next, { horizontal: e.shiftKey, vertical: e.altKey });
      ctx.requestRender();
      return;
    }

    const hit = this.hitTestHandle({ x: e.screenX, y: e.screenY }, ctx);
    if (hit !== this.hoveredIndex) {
      this.hoveredIndex = hit;
      ctx.requestRender();
    }
  }

  onMouseUp(_e: CanvasMouseEvent, ctx: ToolContext): void {
    if (this.state.type !== 'dragging') return;

    const { moved, mode, originalHandles } = this.state;
    const ids = collectEditableSelection(ctx);
    const sourceBounds = this.sourceBounds;
    const handles = this.handles.map((handle) => ({ ...handle }));

    this.state = { type: 'idle' };
    this.activeIndex = null;
    ctx.requestRender();

    if (mode !== this.currentMode()) {
      this.handles = originalHandles.map((handle) => ({ ...handle }));
      this.selectionKey = null;
      ctx.setStatusMessage('');
      ctx.requestRender();
      return;
    }

    if (!moved || !ids || !sourceBounds) {
      ctx.setStatusMessage('');
      return;
    }

    void this.applyDeform(ids, sourceBounds, handles, mode, ctx);
  }

  onKeyDown(e: KeyboardEvent, ctx: ToolContext): void {
    if (e.key !== 'Escape') return;
    if (this.state.type === 'dragging') {
      this.handles = this.state.originalHandles.map((handle) => ({ ...handle }));
      this.state = { type: 'idle' };
      this.activeIndex = null;
      ctx.setStatusMessage('');
      ctx.requestRender();
      return;
    }
    this.reset();
    ctx.setStatusMessage('');
    ctx.requestRender();
  }

  onDoubleClick(e: CanvasMouseEvent, ctx: ToolContext): void {
    if (!this.syncGrid(ctx) || !this.sourceBounds) return;
    const hit = this.hitTestHandle({ x: e.screenX, y: e.screenY }, ctx);
    if (hit === null) return;
    const gridSize = this.gridSizeForMode(this.currentMode());
    const originalGrid = buildGrid(this.sourceBounds, gridSize);
    this.setHandle(hit, originalGrid[hit], { horizontal: e.shiftKey, vertical: e.altKey });
    this.activeIndex = null;
    this.hoveredIndex = hit;
    ctx.requestRender();
  }

  getCursor(): string {
    if (this.state.type === 'dragging') return 'grabbing';
    return this.hoveredIndex !== null ? 'grab' : 'crosshair';
  }

  getOverlay(): ToolOverlay {
    if (this.handles.length === 0) return { type: 'none' };
    return {
      type: 'mesh-deform',
      gridSize: this.activeGridSize(),
      handles: this.handles.map<MeshHandle>((handle, index) => ({
        worldX: handle.x,
        worldY: handle.y,
        active: this.activeIndex === index,
        hovered: this.hoveredIndex === index,
      })),
    };
  }

  reset(): void {
    this.state = { type: 'idle' };
    this.selectionKey = null;
    this.sourceBounds = null;
    this.handles = [];
    this.hoveredIndex = null;
    this.activeIndex = null;
  }

  private syncGrid(ctx: ToolContext): boolean {
    const ids = collectEditableSelection(ctx);
    if (!ids) {
      this.reset();
      return false;
    }

    if (this.state.type === 'dragging') {
      return this.sourceBounds !== null && this.handles.length > 0;
    }

    const gridSize = this.gridSizeForMode(this.currentMode());
    const key = `${gridSize}:${ids.join('|')}`;
    if (key === this.selectionKey && this.sourceBounds && this.handles.length > 0) {
      return true;
    }

    const bounds = combinedSelectionBounds(ctx, ids);
    if (!bounds) {
      this.reset();
      return false;
    }

    this.selectionKey = key;
    this.sourceBounds = bounds;
    this.handles = buildGrid(bounds, gridSize);
    this.hoveredIndex = null;
    this.activeIndex = null;
    return true;
  }

  private hitTestHandle(screenPoint: Point2D, ctx: ToolContext): number | null {
    let bestIndex: number | null = null;
    let bestDistance = HANDLE_HIT_PX;

    for (let index = 0; index < this.handles.length; index++) {
      const screen = worldToScreen(this.handles[index], ctx.vp);
      const distance = Math.hypot(screenPoint.x - screen.x, screenPoint.y - screen.y);
      if (distance <= bestDistance) {
        bestDistance = distance;
        bestIndex = index;
      }
    }

    return bestIndex;
  }

  private setHandle(
    index: number,
    point: Point2D,
    symmetry: { horizontal?: boolean; vertical?: boolean } = {},
  ): void {
    const next = this.handles.map((handle) => ({ ...handle }));
    next[index] = point;

    if (this.sourceBounds && (symmetry.horizontal || symmetry.vertical)) {
      const gridSize = this.activeGridSize();
      const row = Math.floor(index / gridSize);
      const col = index % gridSize;
      const centerX = (this.sourceBounds.min.x + this.sourceBounds.max.x) / 2;
      const centerY = (this.sourceBounds.min.y + this.sourceBounds.max.y) / 2;
      const mirrored: Array<{ row: number; col: number; point: Point2D }> = [];
      if (symmetry.horizontal) {
        mirrored.push({
          row,
          col: gridSize - 1 - col,
          point: { x: centerX - (point.x - centerX), y: point.y },
        });
      }
      if (symmetry.vertical) {
        mirrored.push({
          row: gridSize - 1 - row,
          col,
          point: { x: point.x, y: centerY - (point.y - centerY) },
        });
      }
      if (symmetry.horizontal && symmetry.vertical) {
        mirrored.push({
          row: gridSize - 1 - row,
          col: gridSize - 1 - col,
          point: {
            x: centerX - (point.x - centerX),
            y: centerY - (point.y - centerY),
          },
        });
      }
      for (const mirror of mirrored) {
        const mirrorIndex = mirror.row * gridSize + mirror.col;
        if (mirrorIndex !== index && next[mirrorIndex]) {
          next[mirrorIndex] = mirror.point;
        }
      }
    }

    this.handles = next;
  }

  private reportUnavailable(ctx: ToolContext): void {
    const message = `${this.labelForMode(this.currentMode())} requires an unlocked vector, image, barcode, shape, text, polygon, or star selection.`;
    ctx.setStatusMessage(message);
    useNotificationStore.getState().push(message, 'warning');
  }

  private async applyDeform(
    ids: string[],
    sourceBounds: Bounds,
    handles: Point2D[],
    mode: MeshDeformMode,
    ctx: ToolContext,
  ): Promise<void> {
    const label = this.labelForMode(mode);
    ctx.setStatusMessage(i18n.t('canvas_status.applying_label', { label }));
    try {
      const updated = await vectorService.meshDeformSelection(
        ids,
        sourceBounds,
        handles,
        this.gridSizeForMode(mode),
        mode === 'warp',
      );
      const updatedMap = new Map(updated.map((object) => [object.id, object]));
      useProjectStore.setState((state) => {
        if (!state.project) return state;
        return {
          project: {
            ...state.project,
            objects: state.project.objects.map((object) => updatedMap.get(object.id) ?? object),
            dirty: true,
          },
        };
      });
      usePreviewStore.getState().invalidate();
      await useUndoStore.getState().refresh();
      this.reset();
      ctx.setStatusMessage('');
      ctx.requestRender();
    } catch (error) {
      const message = String(error);
      ctx.setStatusMessage(message);
      useNotificationStore.getState().push(message, 'error');
    }
  }

  private currentMode(): MeshDeformMode {
    return useUiStore.getState().meshDeformMode;
  }

  private gridSizeForMode(mode: MeshDeformMode): number {
    return mode === 'warp' ? 2 : 4;
  }

  private activeGridSize(): number {
    return this.gridSizeForMode(this.state.type === 'dragging' ? this.state.mode : this.currentMode());
  }

  private labelForMode(mode: MeshDeformMode): string {
    return mode === 'warp' ? 'Warp Selection' : 'Mesh Deform';
  }
}

export class WarpTool extends SelectionMeshDeformTool {}
