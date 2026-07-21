import type { CanvasTool, CanvasMouseEvent, ToolContext } from './types';
import type { ToolOverlay } from '../CanvasRenderer';
import { hitTestPoint } from '../hitTest';
import { getCaretIndexFromClick } from '../textMeasure';
import { commitPendingTextEdit, isNewEmptyText } from '../textEditSession';
import { useProjectStore } from '../../stores/projectStore';
import { useUiStore } from '../../stores/uiStore';

export class TextTool implements CanvasTool {
  name = 'text';

  onMouseDown(e: CanvasMouseEvent, ctx: ToolContext): void {
    void this.handleMouseDown(e, ctx);
  }

  private async handleMouseDown(e: CanvasMouseEvent, ctx: ToolContext): Promise<void> {
    const screenPt = { x: e.screenX, y: e.screenY };

    // Hit-test ALL objects (not just text) to respect z-order.
    // Pass includeLocked=true so we can detect locked text and do nothing.
    const topHit = hitTestPoint(screenPt, ctx.objects, ctx.vp, true);

    if (topHit && topHit.data.type === 'text') {
      // Locked text → ignore (do not create on top, do not edit)
      if (topHit.locked) return;

      if (useUiStore.getState().textEditObjectId && useUiStore.getState().textEditObjectId !== topHit.id) {
        const prevId = useUiStore.getState().textEditObjectId;
        const prevMode = useUiStore.getState().textEditMode;
        const shouldDelete = isNewEmptyText(prevId, prevMode);
        const committed = await commitPendingTextEdit();
        if (!committed) return;
        useUiStore.setState({
          textEditObjectId: null, textEditClickPos: null,
          textEditMode: null, textEditCaretIndex: null,
        });
        if (shouldDelete && prevId) {
          await useProjectStore.getState().removeObject(prevId);
        }
      }

      // Click on existing text → try to compute caret index from click position.
      const caretIndex = getCaretIndexFromClick(
        { x: e.worldX, y: e.worldY }, topHit, ctx.vp,
      );
      ctx.selectObjects([topHit.id]);
      useUiStore.getState().beginTextEditSession(
        topHit.id, 'tool-click', undefined, caretIndex ?? undefined,
      );
      return;
    }

    // No text hit → explicitly commit current edit, then create new text.
    const prevId = useUiStore.getState().textEditObjectId;
    const prevMode = useUiStore.getState().textEditMode;
    const shouldDelete = isNewEmptyText(prevId, prevMode);
    const committed = await commitPendingTextEdit();
    if (!committed) return;
    useUiStore.setState({
      textEditObjectId: null, textEditClickPos: null,
      textEditMode: null, textEditCaretIndex: null,
    });
    if (shouldDelete && prevId) {
      await useProjectStore.getState().removeObject(prevId);
    }

    // projectStore.addObject resolves content-type routing.
    const layerId = ctx.selectedLayerId ?? '__auto__';

    const x = e.snappedX;
    const y = e.snappedY;

    const td = useUiStore.getState().textDefaults;

    // Size the initial bounding box to fit the font.
    const w = Math.max(td.font_size_mm * 2, 20);
    const h = td.font_size_mm;

    // Anchor the bounding box at the click position based on alignment settings.
    let minX = x;
    if (td.alignment === 'center') minX = x - w / 2;
    else if (td.alignment === 'right') minX = x - w;

    let minY = y;
    if (td.alignment_v === 'middle') minY = y - h / 2;
    else if (td.alignment_v === 'bottom') minY = y - h;

    const createdObject = await ctx.addObject(
      'Text',
      layerId,
      {
        type: 'text',
        content: '',
        font_family: td.font_family,
        font_size_mm: td.font_size_mm,
        alignment: td.alignment,
        alignment_v: td.alignment_v,
        bold: td.bold,
        italic: td.italic,
        upper_case: td.upper_case,
        welded: td.welded,
        h_spacing: td.h_spacing,
        v_spacing: td.v_spacing,
        layout_mode: td.layout_mode,
        on_path: td.on_path,
        path_offset: td.path_offset,
        distort: td.distort,
        rtl: false,
        bend_radius: td.bend_radius,
        transform_style: td.transform_style,
        transform_curve: td.transform_curve,
        circle_placement: td.circle_placement,
        squeeze: false,
        ignore_empty_vars: false,
        missing_font: false,
      },
      {
        min: { x: minX, y: minY },
        max: { x: minX + w, y: minY + h },
      },
    );
    if (createdObject) {
      // Stay in text tool — enter edit session for the new text
      useUiStore.getState().beginTextEditSession(createdObject.id, 'new', { x, y });
    }
  }

  onMouseMove(_e: CanvasMouseEvent, _ctx: ToolContext): void {
    // No-op: text tool has no drag behavior
  }

  onMouseUp(_e: CanvasMouseEvent, _ctx: ToolContext): void {
    // No-op
  }

  getCursor(): string {
    return 'text';
  }

  getOverlay(): ToolOverlay {
    return { type: 'none' };
  }

  reset(): void {
    // No state to reset
  }
}
