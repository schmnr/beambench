import type { ToolType } from '../stores/uiStore';

/**
 * Run keeps the canvas read-only, with Laser Position as its sole direct
 * canvas action. Design never exposes that machine-motion tool.
 */
export function resolveWorkspaceCanvasTool(
  workspaceMode: 'design' | 'run',
  activeTool: ToolType,
): ToolType {
  if (workspaceMode === 'run') {
    return activeTool === 'laser_position' ? 'laser_position' : 'select';
  }
  return activeTool === 'laser_position' ? 'select' : activeTool;
}
