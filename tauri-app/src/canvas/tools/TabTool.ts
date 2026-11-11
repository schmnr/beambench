import type { CanvasTool, CanvasMouseEvent, ToolContext } from './types';
import type { ToolOverlay } from '../CanvasRenderer';
import { screenToWorldDist } from '../ViewportTransform';
import { vectorService } from '../../services/vectorService';
import { useProjectStore } from '../../stores/projectStore';
import { useNotificationStore } from '../../stores/notificationStore';
import { wrapBackendError } from '../../i18n/errors';

export interface TabMarkerDto {
  subpathIndex: number;
  position: number;
  worldX: number;
  worldY: number;
}

export class TabTool implements CanvasTool {
  name = 'tabs';
  private tabMarkers: { objectId: string; markers: TabMarkerDto[] } | null = null;
  private hoveredTabIndex: number | null = null;
  private refreshSeq = 0;
  private refreshing = false;

  onMouseDown(e: CanvasMouseEvent, ctx: ToolContext): void {
    if (e.button !== 0 || ctx.selectedObjectIds.length !== 1 || this.refreshing) return;
    const objectId = ctx.selectedObjectIds[0];
    // Pointer-down may precede the queued pointer-move. Hit-test this click,
    // including marker ownership, instead of trusting an older hover result.
    this.onMouseMove(e, ctx);
    const marker = this.hoveredTabIndex !== null
      ? this.tabMarkers?.markers[this.hoveredTabIndex]
      : undefined;
    const refreshSeq = this.refreshSeq;
    const { placeTab, removeTab } = useProjectStore.getState();
    void (async () => {
      try {
        if (marker) {
          // The screen-space hit area can exceed 5 mm when zoomed out.
          // Remove the marker we hit, using its resolved path coordinates.
          await removeTab(objectId, marker.worldX, marker.worldY);
        } else {
          await placeTab(objectId, e.worldX, e.worldY);
        }
        // A selection change, reset, or newer refresh owns the overlay now.
        if (refreshSeq === this.refreshSeq) await this.refreshMarkers(objectId);
        ctx.requestRender();
      } catch (err) {
        useNotificationStore.getState().push(wrapBackendError(String(err)), 'warning');
      }
    })();
  }

  onMouseMove(e: CanvasMouseEvent, ctx: ToolContext): void {
    const markers = ctx.selectedObjectIds.length === 1
      && this.tabMarkers?.objectId === ctx.selectedObjectIds[0]
      ? this.tabMarkers.markers
      : [];
    const thresholdMm = Math.max(3, screenToWorldDist(8, ctx.vp.zoom));
    let closestIdx: number | null = null;
    let closestDist = Infinity;

    for (let i = 0; i < markers.length; i++) {
      const m = markers[i];
      const dx = e.worldX - m.worldX;
      const dy = e.worldY - m.worldY;
      const d = Math.sqrt(dx * dx + dy * dy);
      if (d < thresholdMm && d < closestDist) {
        closestDist = d;
        closestIdx = i;
      }
    }

    if (closestIdx !== this.hoveredTabIndex) {
      this.hoveredTabIndex = closestIdx;
      ctx.requestRender();
    }
  }

  onMouseUp(_e: CanvasMouseEvent, _ctx: ToolContext): void {
    // No-op
  }

  getCursor(): string {
    return this.hoveredTabIndex !== null ? 'pointer' : 'crosshair';
  }

  getOverlay(): ToolOverlay {
    if (this.tabMarkers && this.tabMarkers.markers.length > 0) {
      return {
        type: 'tab-markers',
        objectId: this.tabMarkers.objectId,
        markers: this.tabMarkers.markers.map((m, i) => ({
          worldX: m.worldX,
          worldY: m.worldY,
          hovered: i === this.hoveredTabIndex,
        })),
      };
    }
    return { type: 'none' };
  }

  async refreshMarkers(objectId: string): Promise<void> {
    const seq = ++this.refreshSeq;
    this.refreshing = true;
    this.tabMarkers = null;
    this.hoveredTabIndex = null;
    try {
      const markers = await vectorService.resolveTabMarkers(objectId);
      // Discard stale response if a newer refresh was initiated
      if (seq !== this.refreshSeq) return;
      this.tabMarkers = { objectId, markers };
    } catch {
      if (seq !== this.refreshSeq) return;
      this.tabMarkers = null;
    }
    this.refreshing = false;
    this.hoveredTabIndex = null;
  }

  reset(): void {
    this.refreshSeq++;
    this.refreshing = false;
    this.tabMarkers = null;
    this.hoveredTabIndex = null;
  }
}
