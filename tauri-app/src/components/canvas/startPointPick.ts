import { useUiStore } from '../../stores/uiStore';

/** End the modal start-point picker and restore the normal canvas tool. */
export function exitStartPointPickMode(): void {
  const ui = useUiStore.getState();
  ui.setPendingStartPoint(null);
  ui.setActiveTool('select');
}
