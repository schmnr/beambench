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

/** Only Select performs the normal object-drag interaction optimization. */
export function tracksObjectDragInteraction(
  workspaceMode: 'design' | 'run',
  activeTool: ToolType,
): boolean {
  return workspaceMode === 'design'
    && resolveWorkspaceCanvasTool(workspaceMode, activeTool) === 'select';
}

/**
 * Workspace mode changes editing permissions, not artwork appearance. Both
 * canvases must render Line as wireframe and fill-like operations as their
 * true compound fill so switching to Run never changes visible geometry.
 */
export function usesLayerOperationAppearance(_workspaceMode: 'design' | 'run'): boolean {
  return true;
}
