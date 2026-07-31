import { hitTestPoint } from './hitTest';
import { commitPendingTextEdit, isNewEmptyText } from './textEditSession';
import type { CanvasMouseEvent, ToolContext } from './tools/types';
import { useProjectStore } from '../stores/projectStore';
import { useUiStore } from '../stores/uiStore';
import { getCaretIndexFromClick } from './textMeasure';

/**
 * Start inline editing whenever a text object is double-clicked, regardless
 * of which canvas tool is currently active. Returns true when text was hit so
 * tool-specific double-click behavior does not also run underneath it.
 */
export async function beginTextEditFromDoubleClick(
  event: CanvasMouseEvent,
  ctx: ToolContext,
): Promise<boolean> {
  const hit = hitTestPoint(
    { x: event.screenX, y: event.screenY },
    ctx.objects,
    ctx.vp,
    true,
    ctx.layers,
  );
  if (!hit || hit.data.type !== 'text') return false;
  if (hit.locked) return true;

  const ui = useUiStore.getState();
  if (ui.textEditObjectId && ui.textEditObjectId !== hit.id) {
    const previousId = ui.textEditObjectId;
    const shouldDelete = isNewEmptyText(previousId, ui.textEditMode);
    const committed = await commitPendingTextEdit();
    if (!committed) return true;
    useUiStore.setState({
      textEditObjectId: null,
      textEditClickPos: null,
      textEditMode: null,
      textEditCaretIndex: null,
    });
    if (shouldDelete) {
      await useProjectStore.getState().removeObject(previousId);
    }
  }

  ctx.selectObjects([hit.id]);
  useProjectStore.getState().selectLayer(hit.layer_id);
  const caretIndex = getCaretIndexFromClick(
    { x: event.worldX, y: event.worldY },
    hit,
    ctx.vp,
  );
  useUiStore.getState().beginTextEditSession(
    hit.id,
    'double-click',
    undefined,
    caretIndex ?? undefined,
  );
  return true;
}
