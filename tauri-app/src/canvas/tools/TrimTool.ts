import type { CanvasTool, CanvasMouseEvent, ToolContext } from './types';
import type { ToolOverlay } from '../CanvasRenderer';
import { screenToWorldDist, worldToScreen, type ViewportParams } from '../ViewportTransform';
import { vectorService } from '../../services/vectorService';
import { useProjectStore } from '../../stores/projectStore';
import { usePreviewStore } from '../../stores/previewStore';
import { useNotificationStore } from '../../stores/notificationStore';
import { wrapBackendError } from '../../i18n/errors';

export class TrimTool implements CanvasTool {
  name = 'trim';
  private previewWorldPoints: [number, number][] | null = null;
  private previewTimer: number | null = null;
  private lastVp: ViewportParams | null = null;
  private requestSeq = 0;

  onMouseDown(e: CanvasMouseEvent, ctx: ToolContext): void {
    // Clear preview immediately on click
    this.clearPreview(ctx);

    const thresholdMm = screenToWorldDist(5, ctx.vp.zoom);
    const heal = !e.altKey;

    void (async () => {
      try {
        const result = await vectorService.trimShape(e.worldX, e.worldY, thresholdMm, heal);
        const { loadProject, selectObjects } = useProjectStore.getState();
        await loadProject();
        selectObjects(result.objects.map((o) => o.id));
        usePreviewStore.getState().invalidate();
        if (result.healFailed) {
          useNotificationStore.getState().push(
            'Trim produced multiple pieces \u2014 select them and use Close & Join to consolidate',
            'info',
          );
        }
        if (result.openResult) {
          useNotificationStore.getState().push(
            'Trimmed path is open (not fill-ready) \u2014 use Close & Join if a closed shape is needed',
            'info',
          );
        }
      } catch (err) {
        useNotificationStore.getState().push(wrapBackendError(String(err)), 'warning');
      }
    })();
  }

  onMouseMove(e: CanvasMouseEvent, ctx: ToolContext): void {
    this.lastVp = ctx.vp;
    if (this.previewTimer !== null) clearTimeout(this.previewTimer);

    this.previewTimer = window.setTimeout(() => {
      this.fetchPreview(e.worldX, e.worldY, ctx);
    }, 50) as unknown as number;
  }

  private fetchPreview(worldX: number, worldY: number, ctx: ToolContext): void {
    const seq = ++this.requestSeq;
    const thresholdMm = screenToWorldDist(5, ctx.vp.zoom);
    void (async () => {
      try {
        const result = await vectorService.previewTrimSegment(worldX, worldY, thresholdMm);
        if (seq !== this.requestSeq) return; // stale — discard
        this.previewWorldPoints = result?.segmentPoints ?? null;
        ctx.requestRender();
      } catch {
        if (seq !== this.requestSeq) return;
        this.previewWorldPoints = null;
        ctx.requestRender();
      }
    })();
  }

  onMouseUp(_e: CanvasMouseEvent, _ctx: ToolContext): void {
    // No-op
  }

  getCursor(): string {
    return 'crosshair';
  }

  getOverlay(): ToolOverlay {
    if (this.previewWorldPoints && this.previewWorldPoints.length >= 2 && this.lastVp) {
      const vp = this.lastVp;
      const screenPts = this.previewWorldPoints.map(([x, y]) =>
        worldToScreen({ x, y }, vp),
      );
      return { type: 'trim-preview', segmentScreenPoints: screenPts };
    }
    return { type: 'none' };
  }

  reset(): void {
    this.previewWorldPoints = null;
    this.requestSeq++;
    if (this.previewTimer !== null) {
      clearTimeout(this.previewTimer);
      this.previewTimer = null;
    }
    this.lastVp = null;
  }

  private clearPreview(ctx: ToolContext): void {
    this.previewWorldPoints = null;
    this.requestSeq++;
    if (this.previewTimer !== null) {
      clearTimeout(this.previewTimer);
      this.previewTimer = null;
    }
    ctx.requestRender();
  }
}
