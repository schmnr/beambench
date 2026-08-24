import type { CanvasTool, CanvasMouseEvent, ToolContext } from './types';
import type { ToolOverlay } from '../CanvasRenderer';
import { machineService } from '../../services/machineService';
import { useMachineStore } from '../../stores/machineStore';
import { useNotificationStore } from '../../stores/notificationStore';
import { useProjectStore } from '../../stores/projectStore';
import { useUiStore } from '../../stores/uiStore';
import i18n from '../../i18n';

export class LaserPositionTool implements CanvasTool {
  name = 'laser_position';

  onMouseDown(e: CanvasMouseEvent, ctx: ToolContext): void {
    const sessionState = useMachineStore.getState().sessionState;
    if (sessionState === 'disconnected') {
      ctx.setStatusMessage(i18n.t('canvas_status.machine_not_connected'));
      return;
    }

    const project = useProjectStore.getState().project;
    const clickedPoint = { x: e.worldX, y: e.worldY };
    const workspace = project?.workspace ?? ctx.workspace;
    if (workspace && (
      clickedPoint.x < 0 ||
      clickedPoint.y < 0 ||
      clickedPoint.x > workspace.bed_width_mm ||
      clickedPoint.y > workspace.bed_height_mm
    )) {
      ctx.setStatusMessage(i18n.t('canvas_status.click_workspace_move_laser'));
      return;
    }

    const ui = useUiStore.getState();
    const feedRate = ui.moveWindowJogFeedRateMmMin;
    // Positioning is a one-shot action. Disarm as soon as a valid move is
    // accepted so another canvas click cannot queue an unintended move.
    ui.setActiveTool('select');

    void (async () => {
      try {
        await machineService.moveLaserToProjectPoint(clickedPoint.x, clickedPoint.y, feedRate);
      } catch (error) {
        const message = String(error);
        ctx.setStatusMessage(message);
        useNotificationStore.getState().push(message, 'error');
      }
    })();
  }

  onMouseMove(_e: CanvasMouseEvent, _ctx: ToolContext): void {
    // No-op
  }

  onMouseUp(_e: CanvasMouseEvent, _ctx: ToolContext): void {
    // No-op
  }

  getCursor(): string {
    return 'crosshair';
  }

  getOverlay(): ToolOverlay {
    return { type: 'none' };
  }

  reset(): void {
    // No state to clear
  }
}
