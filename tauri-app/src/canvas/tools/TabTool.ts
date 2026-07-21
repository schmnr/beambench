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

  onMouseDown(e: CanvasMouseEvent, ctx: ToolContext): void {
    const objectId = ctx.selectedObjectIds[0];
    if (!objectId) return;

    const { placeTab, removeTab } = useProjectStore.getState();

    if (this.hoveredTabIndex !== null && this.tabMarkers) {
      // Click on existing marker → remove it
      void (async () => {
        try {
          await removeTab(objectId, e.worldX, e.worldY);
          await this.refreshMarkers(objectId);
          ctx.requestRender();
        } catch (err) {
          useNotificationStore.getState().push(wrapBackendError(String(err)), 'warning');
        }
      })();
    } else {
      // Click on path edge → place tab
      void (async () => {
        try {
          await placeTab(objectId, e.worldX, e.worldY);
          await this.refreshMarkers(objectId);
          ctx.requestRender();
        } catch (err) {
          useNotificationStore.getState().push(wrapBackendError(String(err)), 'warning');
        }
      })();
    }
  }

  onMouseMove(e: CanvasMouseEvent, ctx: ToolContext): void {
    if (!this.tabMarkers || this.tabMarkers.markers.length === 0) {
      this.hoveredTabIndex = null;
      return;
    }

    // Hit-test markers: 3mm threshold in world space
    const thresholdMm = Math.max(3, screenToWorldDist(8, ctx.vp.zoom));
    let closestIdx: number | null = null;
    let closestDist = Infinity;

    for (let i = 0; i < this.tabMarkers.markers.length; i++) {
      const m = this.tabMarkers.markers[i];
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
    try {
      const markers = await vectorService.resolveTabMarkers(objectId);
      // Discard stale response if a newer refresh was initiated
      if (seq !== this.refreshSeq) return;
      this.tabMarkers = { objectId, markers };
    } catch {
      if (seq !== this.refreshSeq) return;
      this.tabMarkers = null;
    }
    this.hoveredTabIndex = null;
  }

  reset(): void {
    this.refreshSeq++;
    this.tabMarkers = null;
    this.hoveredTabIndex = null;
  }
}
