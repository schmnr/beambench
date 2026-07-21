import type { CanvasTool, CanvasMouseEvent, ToolContext } from './types';
import type { ToolOverlay } from '../CanvasRenderer';
import { machineService } from '../../services/machineService';
import { useMachineStore } from '../../stores/machineStore';
import { useNotificationStore } from '../../stores/notificationStore';
import { useProjectStore } from '../../stores/projectStore';
import { useUiStore } from '../../stores/uiStore';
import { canvasToMachinePoint } from '../../utils/workspaceCoordinates';
import i18n from '../../i18n';

export class LaserPositionTool implements CanvasTool {
  name = 'laser_position';

  onMouseDown(e: CanvasMouseEvent, ctx: ToolContext): void {
    const sessionState = useMachineStore.getState().sessionState;
    if (sessionState === 'disconnected') {
      ctx.setStatusMessage(i18n.t('canvas_status.machine_not_connected'));
      return;
    }

    // Apply start-from offset (matches planner's apply_start_from_offset)
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

    const machinePoint = workspace
      ? canvasToMachinePoint(clickedPoint, workspace)
      : clickedPoint;
    const startFrom = project?.start_from ?? 'absolute_coords';
    let ox = 0, oy = 0;
    if (startFrom === 'user_origin') {
      const uo = project?.user_origin;
      if (uo) { ox = uo[0]; oy = uo[1]; }
    } else if (startFrom === 'current_position') {
      const wp = useMachineStore.getState().machineStatus?.work_position;
      if (wp) { ox = wp.x; oy = wp.y; }
    }

    void (async () => {
      try {
        const feedRate = useUiStore.getState().moveWindowJogFeedRateMmMin;
        await machineService.moveLaserTo(machinePoint.x + ox, machinePoint.y + oy, feedRate);
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
