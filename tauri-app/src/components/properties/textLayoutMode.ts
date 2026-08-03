import { projectService } from '../../services/projectService';
import { useNotificationStore } from '../../stores/notificationStore';
import { wrapBackendError } from '../../i18n/errors';
import { usePreviewStore } from '../../stores/previewStore';
import { useProjectStore } from '../../stores/projectStore';
import { useUndoStore } from '../../stores/undoStore';

async function refreshProjectAfterGuideChange(): Promise<void> {
  const refreshed = await projectService.getProject();
  if (!refreshed) {
    return;
  }
  useProjectStore.setState({ project: { ...refreshed, dirty: true } });
  usePreviewStore.getState().invalidate();
  await useUndoStore.getState().refresh();
}

export async function clearTextGuidePath(textObjectId: string): Promise<void> {
  try {
    await projectService.setTextGuidePath(textObjectId, null);
    await refreshProjectAfterGuideChange();
  } catch (err) {
    useNotificationStore.getState().push(wrapBackendError(String(err)), 'error');
  }
}
