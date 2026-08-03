import { useRef, useEffect, useCallback, useState, useSyncExternalStore } from 'react';
import { useProjectStore } from '../../stores/projectStore';
import { useUiStore } from '../../stores/uiStore';
import type { ViewportParams } from '../../canvas/ViewportTransform';
import { screenToWorldDist, worldToScreen, worldToScreenDist } from '../../canvas/ViewportTransform';
import type { ObjectData } from '../../types/project';
import {
  setPendingEdit, releasePendingEdit, updatePendingContent,
  getPendingContentForObject, subscribePendingTextEdit,
} from '../../canvas/textEditSession';
import { useTranslation } from 'react-i18next';

interface TextEditOverlayProps {
  vp: ViewportParams;
}

export function TextEditOverlay({ vp }: TextEditOverlayProps) {
  const { t } = useTranslation();
  const textEditObjectId = useUiStore((s) => s.textEditObjectId);
  const textEditClickPos = useUiStore((s) => s.textEditClickPos);
  const textEditMode = useUiStore((s) => s.textEditMode);
  const textEditCaretIndex = useUiStore((s) => s.textEditCaretIndex);
  const setTextEditObjectId = useUiStore((s) => s.setTextEditObjectId);
  const zoomIn = useUiStore((s) => s.zoomIn);
  const zoomOut = useUiStore((s) => s.zoomOut);
  const project = useProjectStore((s) => s.project);
  const resizeTextArea = useProjectStore((s) => s.resizeTextArea);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const resizeStateRef = useRef<{
    pointerId: number;
    startClientX: number;
    startClientY: number;
    startWidth: number;
    startHeight: number;
  } | null>(null);
  const initialContentRef = useRef<string>('');
  const mountedRef = useRef(false);
  const [localContent, setLocalContent] = useState('');
  const [areaSizePreview, setAreaSizePreview] = useState<{ width: number; height: number } | null>(null);
  const pendingContent = useSyncExternalStore(
    subscribePendingTextEdit,
    () => textEditObjectId ? getPendingContentForObject(textEditObjectId) : null,
    () => null,
  );

  const obj = textEditObjectId
    ? project?.objects?.find((o) => o.id === textEditObjectId) ?? null
    : null;

  const textData = obj?.data?.type === 'text'
    ? (obj.data as Extract<ObjectData, { type: 'text' }>)
    : null;
  const fontSize = worldToScreenDist(textData?.font_size_mm ?? 0, vp.zoom);
  const lineHeight = Math.max(fontSize * 0.2, fontSize + worldToScreenDist(textData?.v_spacing ?? 0, vp.zoom));
  const areaText = (textData?.max_width ?? 0) > 0;
  const minW = 60;
  const minH = Math.max(lineHeight, 16);

  // Capture initial content, seed local state, and register pending edit on mount
  useEffect(() => {
    if (textData && obj) {
      initialContentRef.current = textData.content;
      setLocalContent(getPendingContentForObject(obj.id) ?? textData.content);
      setPendingEdit(obj.id, textData.content);
    }
    return () => releasePendingEdit(obj?.id);
  }, [textEditObjectId]); // eslint-disable-line react-hooks/exhaustive-deps

  // Capture characters routed through the canvas during the tiny interval
  // between opening an edit session and the textarea receiving focus.
  useEffect(() => {
    if (pendingContent !== null) setLocalContent(pendingContent);
  }, [pendingContent]);

  // Auto-focus and mode-aware caret placement on mount
  useEffect(() => {
    if (textData && textareaRef.current) {
      requestAnimationFrame(() => {
        const ta = textareaRef.current;
        if (!ta) return;
        ta.focus();
        if ((textEditMode === 'double-click' || textEditMode === 'tool-click') && textEditCaretIndex != null) {
          ta.setSelectionRange(textEditCaretIndex, textEditCaretIndex);
        } else if (textEditMode === 'double-click' || textEditMode === 'tool-click') {
          ta.select(); // tool-click but no caret (path/bend/transformed) → select all
        } else {
          ta.setSelectionRange(0, 0); // new text: start
        }
      });
    }
  }, [textEditObjectId]); // eslint-disable-line react-hooks/exhaustive-deps

  // Track mount state for same-object re-click caret updates
  useEffect(() => {
    mountedRef.current = true;
    return () => { mountedRef.current = false; };
  }, []);

  // Handle caret updates without remount (same-object re-click).
  // Depends on both textEditCaretIndex AND textEditMode so that a
  // mode change (e.g. 'new' → 'tool-click') reruns even when
  // caretIndex stays null (path/bend/transformed text → select-all).
  useEffect(() => {
    if (!mountedRef.current) return;
    const ta = textareaRef.current;
    if (!ta) return;
    if (textEditCaretIndex != null) {
      ta.setSelectionRange(textEditCaretIndex, textEditCaretIndex);
    } else if (textEditMode === 'tool-click' || textEditMode === 'double-click') {
      ta.select();
    }
  }, [textEditCaretIndex, textEditMode]);

  // Auto-resize textarea to fit content
  useEffect(() => {
    const ta = textareaRef.current;
    if (!ta) return;
    if (areaText) {
      ta.style.width = '100%';
      ta.style.height = '100%';
      return;
    }
    ta.style.height = 'auto';
    ta.style.height = `${Math.max(ta.scrollHeight, minH)}px`;
    if (ta.wrap === 'off') {
      ta.style.width = 'auto';
      ta.style.width = `${Math.max(ta.scrollWidth + 4, minW)}px`;
    }
  }, [areaText, localContent, minH]);

  const handleChange = useCallback((e: React.ChangeEvent<HTMLTextAreaElement>) => {
    setLocalContent(e.target.value);
    updatePendingContent(e.target.value);
  }, []);

  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    const zoomModifier = e.metaKey || e.ctrlKey;
    if (zoomModifier && (e.key === '=' || e.key === '+')) {
      e.preventDefault();
      e.stopPropagation();
      zoomIn();
      return;
    }
    if (zoomModifier && e.key === '-') {
      e.preventDefault();
      e.stopPropagation();
      zoomOut();
      return;
    }
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      // setTextEditObjectId(null) commits the pending edit, clears state,
      // and removes the object if it was a brand-new empty text.
      setTextEditObjectId(null);
      return;
    }
    if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      e.stopPropagation();
      setTextEditObjectId(null);
      return;
    }
    // Enter inserts newline (textarea handles it natively — no preventDefault).
    // Stop propagation for all keys to prevent canvas tool shortcuts.
    e.stopPropagation();
  }, [setTextEditObjectId, zoomIn, zoomOut]);

  const handleBlur = useCallback(() => {
    setTextEditObjectId(null);
  }, [setTextEditObjectId]);

  const handleResizePointerDown = useCallback((event: React.PointerEvent<HTMLDivElement>) => {
    if (!obj || !areaText) return;
    event.preventDefault();
    event.stopPropagation();
    event.currentTarget.setPointerCapture(event.pointerId);
    const startWidth = Math.max(24, worldToScreenDist(obj.bounds.max.x - obj.bounds.min.x, vp.zoom));
    const startHeight = Math.max(minH, worldToScreenDist(obj.bounds.max.y - obj.bounds.min.y, vp.zoom));
    resizeStateRef.current = {
      pointerId: event.pointerId,
      startClientX: event.clientX,
      startClientY: event.clientY,
      startWidth,
      startHeight,
    };
    setAreaSizePreview({ width: startWidth, height: startHeight });
  }, [areaText, minH, obj, vp.zoom]);

  const handleResizePointerMove = useCallback((event: React.PointerEvent<HTMLDivElement>) => {
    const resize = resizeStateRef.current;
    if (!resize || resize.pointerId !== event.pointerId) return;
    event.preventDefault();
    event.stopPropagation();
    setAreaSizePreview({
      width: Math.max(24, resize.startWidth + event.clientX - resize.startClientX),
      height: Math.max(minH, resize.startHeight + event.clientY - resize.startClientY),
    });
  }, [minH]);

  const handleResizePointerUp = useCallback((event: React.PointerEvent<HTMLDivElement>) => {
    const resize = resizeStateRef.current;
    if (!resize || resize.pointerId !== event.pointerId || !obj) return;
    event.preventDefault();
    event.stopPropagation();
    const width = Math.max(24, resize.startWidth + event.clientX - resize.startClientX);
    const height = Math.max(minH, resize.startHeight + event.clientY - resize.startClientY);
    resizeStateRef.current = null;
    event.currentTarget.releasePointerCapture(event.pointerId);
    void resizeTextArea(obj.id, {
      min: obj.bounds.min,
      max: {
        x: obj.bounds.min.x + screenToWorldDist(width, vp.zoom),
        y: obj.bounds.min.y + screenToWorldDist(height, vp.zoom),
      },
    }).finally(() => setAreaSizePreview(null));
  }, [minH, obj, resizeTextArea, vp.zoom]);

  const handleResizePointerCancel = useCallback((event: React.PointerEvent<HTMLDivElement>) => {
    if (resizeStateRef.current?.pointerId !== event.pointerId) return;
    resizeStateRef.current = null;
    setAreaSizePreview(null);
  }, []);

  if (!textEditObjectId || !obj || !textData) return null;

  const areaWidth = areaText
    ? areaSizePreview?.width ?? Math.max(24, worldToScreenDist(obj.bounds.max.x - obj.bounds.min.x, vp.zoom))
    : null;
  const areaHeight = areaText
    ? areaSizePreview?.height ?? Math.max(minH, worldToScreenDist(obj.bounds.max.y - obj.bounds.min.y, vp.zoom))
    : null;

  // Use the layer's color for both text and caret.
  const layer = project?.layers?.find((l) => l.id === obj.layer_id);
  const layerColor = layer?.color_tag ?? '#ffffff';

  const fontStyle = `${textData.italic ? 'italic ' : ''}${textData.bold ? 'bold ' : ''}`;
  // Tight line-height (equal to font size) so the caret's bottom edge
  // lines up with the text baseline — no descender gap below the caret.
  // Scales cleanly with any font size.

  // Position: for new text, use click position; for existing text, use alignment anchor
  const alignment = textData.alignment ?? 'left';
  const alignmentV = textData.alignment_v ?? 'top';

  let worldX: number, worldY: number;
  if (!areaText && textEditMode === 'new' && textEditClickPos) {
    worldX = textEditClickPos.x;
    worldY = textEditClickPos.y;
  } else {
    worldX = alignment === 'center' ? (obj.bounds.min.x + obj.bounds.max.x) / 2
           : alignment === 'right'  ? obj.bounds.max.x
           : obj.bounds.min.x;
    worldY = alignmentV === 'middle' ? (obj.bounds.min.y + obj.bounds.max.y) / 2
           : alignmentV === 'bottom' ? obj.bounds.max.y
           : obj.bounds.min.y;
  }
  const screenPos = worldToScreen({ x: worldX, y: worldY }, vp);

  // CSS alignment transforms
  const transformParts: string[] = [];
  if (alignment === 'center') transformParts.push('translateX(-50%)');
  else if (alignment === 'right') transformParts.push('translateX(-100%)');
  if (alignmentV === 'middle') transformParts.push('translateY(-50%)');
  else if (alignmentV === 'bottom') transformParts.push('translateY(-100%)');
  const transform = transformParts.length ? transformParts.join(' ') : undefined;

  // With tight line-height, the caret tightly bounds the text. The
  // alignment-based translateY() transforms above already put caret-top
  // (top alignment), caret-middle (middle), or caret-bottom (bottom) at
  // the click point — no additional offset needed.
  const vOffsetPx = 0;

  const editor = (
    <textarea
      key={textEditObjectId}
      ref={textareaRef}
      value={localContent}
      onChange={handleChange}
      onKeyDown={handleKeyDown}
      onBlur={handleBlur}
      wrap={areaText ? 'soft' : 'off'}
      aria-label={t('panels.text_properties.edit_text')}
      className={`text-edit-overlay resize-none border-none bg-transparent outline-none ${areaText ? 'h-full w-full' : 'absolute'}`}
      style={{
        left: areaText ? undefined : screenPos.x,
        top: areaText ? undefined : screenPos.y + vOffsetPx,
        width: areaText ? '100%' : undefined,
        minWidth: areaText ? undefined : minW,
        maxWidth: areaText ? '100%' : undefined,
        height: areaText ? '100%' : undefined,
        minHeight: minH,
        font: `${fontStyle}${fontSize}px ${textData.font_family}`,
        textAlign: alignment,
        color: layerColor,
        boxSizing: 'border-box',
        border: 'none',
        outline: 'none',
        padding: 0,
        margin: 0,
        lineHeight: `${lineHeight}px`,
        overflow: areaText ? 'auto' : 'hidden',
        whiteSpace: areaText ? 'pre-wrap' : 'pre',
        zIndex: areaText ? undefined : 20,
        caretColor: layerColor,
        transform: areaText ? undefined : transform,
      }}
      data-testid="text-edit-overlay"
    />
  );

  if (!areaText) return editor;

  return (
    <div
      className="absolute z-20 border border-dashed border-bb-accent/70 bg-transparent"
      style={{
        left: screenPos.x,
        top: screenPos.y + vOffsetPx,
        width: areaWidth ?? minW,
        height: areaHeight ?? minH,
        transform,
      }}
      data-testid="text-area-frame"
    >
      {editor}
      <div
        role="separator"
        aria-label={t('panels.text_properties.box_width')}
        title={t('panels.text_properties.box_width')}
        className="absolute -bottom-1 -right-1 h-2.5 w-2.5 cursor-nwse-resize border border-bb-panel bg-bb-accent"
        onPointerDown={handleResizePointerDown}
        onPointerMove={handleResizePointerMove}
        onPointerUp={handleResizePointerUp}
        onPointerCancel={handleResizePointerCancel}
        data-testid="text-area-resize-handle"
      />
    </div>
  );
}
