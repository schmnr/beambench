import type { CanvasTool, CanvasMouseEvent, ToolContext } from './types';
import type { ToolOverlay } from '../CanvasRenderer';
import { screenToWorldDist } from '../ViewportTransform';
import { vectorService } from '../../services/vectorService';
import { useProjectStore } from '../../stores/projectStore';
import { useUiStore } from '../../stores/uiStore';
import { useAppStore } from '../../stores/appStore';
import { useNotificationStore } from '../../stores/notificationStore';
import { wrapBackendError } from '../../i18n/errors';

export interface FilletCandidateDto {
  subpathIndex: number;
  vertexIndex: number;
  x: number;
  y: number;
  alreadyFilleted: boolean;
}

export class RadiusTool implements CanvasTool {
  name = 'radius';
  private candidates: { objectId: string; markers: FilletCandidateDto[] } | null = null;
  private hoveredIndex: number | null = null;
  private refreshSeq = 0;

  onMouseDown(_e: CanvasMouseEvent, ctx: ToolContext): void {
    const objectId = ctx.selectedObjectIds[0];
    if (!objectId || this.hoveredIndex === null || !this.candidates) return;
    // Reject clicks when candidates are from a different object
    if (this.candidates.objectId !== objectId) return;

    const candidate = this.candidates.markers[this.hoveredIndex];
    if (!candidate) return;

    const radiusToolValue = useUiStore.getState().radiusToolValue;
    const settings = useAppStore.getState().settings;
    const activeRadius = radiusToolValue ?? settings?.last_radius_mm ?? 5;
    // Toggle: clicking an already-filleted corner sends 0 to remove it
    const radius = candidate.alreadyFilleted ? 0 : activeRadius;

    const { applyCornerRadius } = useProjectStore.getState();

    void (async () => {
      try {
        await applyCornerRadius(objectId, candidate.subpathIndex, candidate.vertexIndex, radius);
        // Update the UI value (persisted when deactivating the tool)
        if (radius > 0) useUiStore.getState().setRadiusToolValue(radius);
        // Refresh candidates — the filleted corner will disappear
        await this.refreshCandidates(objectId);
        ctx.requestRender();
      } catch (err) {
        useNotificationStore.getState().push(wrapBackendError(String(err)), 'warning');
      }
    })();
  }

  onMouseMove(e: CanvasMouseEvent, ctx: ToolContext): void {
    if (!this.candidates || this.candidates.markers.length === 0) {
      this.hoveredIndex = null;
      return;
    }

    const thresholdMm = Math.max(3, screenToWorldDist(8, ctx.vp.zoom));
    let closestIdx: number | null = null;
    let closestDist = Infinity;

    for (let i = 0; i < this.candidates.markers.length; i++) {
      const m = this.candidates.markers[i];
      const dx = e.worldX - m.x;
      const dy = e.worldY - m.y;
      const d = Math.sqrt(dx * dx + dy * dy);
      if (d < thresholdMm && d < closestDist) {
        closestDist = d;
        closestIdx = i;
      }
    }

    if (closestIdx !== this.hoveredIndex) {
      this.hoveredIndex = closestIdx;
      ctx.requestRender();
    }
  }

  onMouseUp(_e: CanvasMouseEvent, _ctx: ToolContext): void {
    // No-op
  }

  getCursor(): string {
    return this.hoveredIndex !== null ? 'pointer' : 'crosshair';
  }

  getOverlay(): ToolOverlay {
    if (this.candidates && this.candidates.markers.length > 0) {
      return {
        type: 'radius-corners',
        objectId: this.candidates.objectId,
        markers: this.candidates.markers.map((m, i) => ({
          worldX: m.x,
          worldY: m.y,
          hovered: i === this.hoveredIndex,
          alreadyFilleted: m.alreadyFilleted,
        })),
      };
    }
    return { type: 'none' };
  }

  async refreshCandidates(objectId: string): Promise<void> {
    const seq = ++this.refreshSeq;
    // Clear stale state immediately so clicks during fetch are rejected
    this.candidates = null;
    this.hoveredIndex = null;
    try {
      const candidates = await vectorService.getFilletCandidates(objectId);
      if (seq !== this.refreshSeq) return;
      this.candidates = { objectId, markers: candidates };
    } catch (err) {
      if (seq !== this.refreshSeq) return;
      this.candidates = null;
      if (err) {
        useNotificationStore.getState().push(wrapBackendError(String(err)), 'warning');
      }
    }
    this.hoveredIndex = null;
  }

  reset(): void {
    this.refreshSeq++;
    this.candidates = null;
    this.hoveredIndex = null;
  }
}
