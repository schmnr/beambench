import { useProjectStore } from '../../stores/projectStore';
import { useUiStore } from '../../stores/uiStore';

export async function cancelPendingGuidePathSelection(textId?: string | null): Promise<boolean> {
  const pendingTextId = textId ?? useUiStore.getState().pendingGuidePathTextId;
  if (!pendingTextId) return true;

  const textObj = useProjectStore.getState().project?.objects.find((o) => o.id === pendingTextId);
  if (!textObj || textObj.data.type !== 'text' || textObj.data.guide_path_id) {
    useUiStore.getState().setPendingGuidePathText(null);
    return true;
  }

  const reverted = await useProjectStore.getState().updateObjectData(
    pendingTextId,
    { ...textObj.data, layout_mode: 'straight', on_path: false },
  );
  if (!reverted) return false;

  useUiStore.getState().setPendingGuidePathText(null);
  return true;
}
