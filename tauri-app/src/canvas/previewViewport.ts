import type { Workspace } from '../types/project';
import type { ViewportParams } from './ViewportTransform';

/**
 * Preview data is generated from the machine execution plan. Bottom-left
 * workspaces flip canvas Y into machine Y, so render those previews with a
 * matching Y-up viewport instead of mutating the plan geometry.
 */
export function previewViewportForWorkspace(
  vp: ViewportParams,
  workspace: Workspace | null | undefined,
): ViewportParams {
  if (workspace?.origin !== 'bottom_left') {
    return vp;
  }

  return {
    ...vp,
    offset: {
      x: vp.offset.x,
      y: workspace.bed_height_mm - vp.offset.y,
    },
    yAxis: 'up',
  };
}
