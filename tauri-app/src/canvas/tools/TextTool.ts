import type { CanvasTool, CanvasMouseEvent, ToolContext } from './types';
import type { ToolOverlay } from '../CanvasRenderer';
import { hitTestPoint } from '../hitTest';
import { getCaretIndexFromClick } from '../textMeasure';
import {
  commitPendingTextEdit,
  isNewEmptyText,
  setPendingEdit,
  updatePendingContent,
} from '../textEditSession';
import { useProjectStore } from '../../stores/projectStore';
import { useUiStore } from '../../stores/uiStore';

const TEXT_BOX_DRAG_THRESHOLD_PX = 4;
const MIN_TEXT_BOX_WIDTH_MM = 1;
const DEFAULT_TEXT_BOX_LINES = 4;

type TextToolState =
  | { type: 'idle' }
  | {
      type: 'pending-create';
      startWorld: { x: number; y: number };
      currentWorld: { x: number; y: number };
      startScreen: { x: number; y: number };
      currentScreen: { x: number; y: number };
      textBoxMode: boolean;
    };

export class TextTool implements CanvasTool {
  name = 'text';
  private state: TextToolState = { type: 'idle' };
  private creatingText = false;
  private pendingTypedText = '';

  onMouseDown(e: CanvasMouseEvent, ctx: ToolContext): void {
    void this.handleMouseDown(e, ctx);
  }

  private async handleMouseDown(e: CanvasMouseEvent, ctx: ToolContext): Promise<void> {
    const screenPt = { x: e.screenX, y: e.screenY };

    // Hit-test ALL objects (not just text) to respect z-order.
    // Pass includeLocked=true so we can detect locked text and do nothing.
    const topHit = hitTestPoint(screenPt, ctx.objects, ctx.vp, true, ctx.layers);

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

    // Empty canvas (or a non-text hit): defer creation until pointer-up.
    // Point mode always creates natural text; only explicit Box mode turns a
    // drag into a wrapping region.
    const textBoxMode = (useUiStore.getState().textDefaults.max_width ?? 0) > 0;
    this.state = {
      type: 'pending-create',
      startWorld: { x: e.snappedX, y: e.snappedY },
      currentWorld: { x: e.snappedX, y: e.snappedY },
      startScreen: screenPt,
      currentScreen: screenPt,
      textBoxMode,
    };
    ctx.requestRender();
  }

  onMouseMove(e: CanvasMouseEvent, ctx: ToolContext): void {
    if (this.state.type !== 'pending-create') return;
    this.state = {
      ...this.state,
      currentWorld: { x: e.snappedX, y: e.snappedY },
      currentScreen: { x: e.screenX, y: e.screenY },
    };
    ctx.requestRender();
  }

  onMouseUp(e: CanvasMouseEvent, ctx: ToolContext): void {
    if (this.state.type !== 'pending-create') return;
    const pending = {
      ...this.state,
      currentWorld: { x: e.snappedX, y: e.snappedY },
      currentScreen: { x: e.screenX, y: e.screenY },
    };
    this.state = { type: 'idle' };
    ctx.requestRender();
    this.creatingText = true;
    this.pendingTypedText = '';
    void this.createText(pending, ctx).finally(() => {
      this.creatingText = false;
      this.pendingTypedText = '';
    });
  }

  private async createText(
    pending: Extract<TextToolState, { type: 'pending-create' }>,
    ctx: ToolContext,
  ): Promise<void> {
    // Explicitly commit the current edit before creating a second text object.
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

    const x = pending.startWorld.x;
    const y = pending.startWorld.y;

    const td = useUiStore.getState().textDefaults;
    const dragDistance = Math.hypot(
      pending.currentScreen.x - pending.startScreen.x,
      pending.currentScreen.y - pending.startScreen.y,
    );
    const draggedWidth = Math.abs(pending.currentWorld.x - pending.startWorld.x);
    const draggedTextBox = pending.textBoxMode
      && dragDistance >= TEXT_BOX_DRAG_THRESHOLD_PX
      && draggedWidth >= MIN_TEXT_BOX_WIDTH_MM;
    const isAreaText = pending.textBoxMode;

    const w = draggedTextBox
      ? draggedWidth
      : isAreaText
        ? Math.max(td.max_width ?? 0, MIN_TEXT_BOX_WIDTH_MM)
        : Math.max(td.font_size_mm * 2, 20);
    const h = draggedTextBox
      ? Math.max(Math.abs(pending.currentWorld.y - pending.startWorld.y), td.font_size_mm)
      : isAreaText
        ? td.font_size_mm * DEFAULT_TEXT_BOX_LINES
        : td.font_size_mm;

    let minX: number;
    let minY: number;
    if (isAreaText) {
      minX = Math.min(pending.startWorld.x, pending.currentWorld.x);
      minY = Math.min(pending.startWorld.y, pending.currentWorld.y);
    } else {
      minX = x;
      if (td.alignment === 'center') minX = x - w / 2;
      else if (td.alignment === 'right') minX = x - w;
      minY = y;
      if (td.alignment_v === 'middle') minY = y - h / 2;
      else if (td.alignment_v === 'bottom') minY = y - h;
    }

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
        rtl: td.rtl,
        bend_radius: td.bend_radius,
        transform_style: td.transform_style,
        transform_curve: td.transform_curve,
        circle_placement: td.circle_placement,
        max_width: isAreaText ? w : null,
        squeeze: isAreaText ? td.squeeze : false,
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
      useUiStore.getState().beginTextEditSession(
        createdObject.id,
        'new',
        isAreaText ? undefined : { x, y },
      );
      // Object creation crosses the Tauri bridge. Preserve characters typed in
      // that brief interval so fast users never lose the first letter. Seed
      // after opening the session so it is not mistaken for an older edit that
      // must be committed before the transition.
      setPendingEdit(createdObject.id, '');
      if (this.pendingTypedText) {
        updatePendingContent(this.pendingTypedText);
      }
    }
  }

  onKeyDown(e: KeyboardEvent): void {
    if (!this.creatingText || e.metaKey || e.ctrlKey || e.altKey) return;
    const text = e.key === 'Enter' ? '\n' : e.key.length === 1 ? e.key : '';
    if (!text) return;
    e.preventDefault();
    e.stopPropagation();
    this.pendingTypedText += text;
  }

  getCursor(): string {
    const textBoxMode = this.state.type === 'pending-create'
      ? this.state.textBoxMode
      : (useUiStore.getState().textDefaults.max_width ?? 0) > 0;
    return textBoxMode ? 'crosshair' : 'text';
  }

  getOverlay(): ToolOverlay {
    if (this.state.type === 'pending-create' && this.state.textBoxMode) {
      return {
        type: 'text-box-preview',
        startWorld: this.state.startWorld,
        endWorld: this.state.currentWorld,
      };
    }
    return { type: 'none' };
  }

  reset(): void {
    this.state = { type: 'idle' };
    this.creatingText = false;
    this.pendingTypedText = '';
  }
}
