import type { ObjectData, TextLayoutMode } from '../../types/project';
import { projectService } from '../../services/projectService';
import { useNotificationStore } from '../../stores/notificationStore';
import { wrapBackendError } from '../../i18n/errors';
import { usePreviewStore } from '../../stores/previewStore';
import { useProjectStore } from '../../stores/projectStore';
import { useUiStore } from '../../stores/uiStore';
import { useUndoStore } from '../../stores/undoStore';

type TextData = Extract<ObjectData, { type: 'text' }>;

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

export async function applyTextLayoutMode(
  textObjectId: string,
  textData: TextData,
  mode: TextLayoutMode,
  options?: {
    bendRadiusFallback?: number;
  },
): Promise<void> {
  const updateObjectData = useProjectStore.getState().updateObjectData;

  if (mode === 'path' && !textData.guide_path_id) {
    const ok = await updateObjectData(textObjectId, {
      ...textData,
      layout_mode: mode,
      on_path: true,
    });
    if (ok) {
      useUiStore.getState().setPendingGuidePathText(textObjectId);
      useNotificationStore.getState().push(
        'Click a vector or shape object on the canvas to use as guide path (Escape to cancel)',
        'info',
      );
    }
    return;
  }

  if (mode === 'straight') {
    useUiStore.getState().setPendingGuidePathText(null);
    if (textData.guide_path_id) {
      await clearTextGuidePath(textObjectId);
      return;
    }
    await updateObjectData(textObjectId, {
      ...textData,
      layout_mode: mode,
      on_path: false,
    });
    return;
  }

  if (mode === 'bend') {
    const bendRadius = textData.bend_radius === 0
      ? (options?.bendRadiusFallback ?? 50)
      : textData.bend_radius;
    await updateObjectData(textObjectId, {
      ...textData,
      layout_mode: mode,
      bend_radius: bendRadius,
    });
    return;
  }

  await updateObjectData(textObjectId, { ...textData, layout_mode: mode });
}
