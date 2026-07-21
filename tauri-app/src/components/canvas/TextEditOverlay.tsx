import { useRef, useEffect, useCallback, useState } from 'react';
import { useProjectStore } from '../../stores/projectStore';
import { useUiStore } from '../../stores/uiStore';
import type { ViewportParams } from '../../canvas/ViewportTransform';
import { worldToScreen, worldToScreenDist } from '../../canvas/ViewportTransform';
import type { ObjectData } from '../../types/project';
import {
  setPendingEdit, releasePendingEdit, updatePendingContent,
  getPendingContentForObject,
} from '../../canvas/textEditSession';

interface TextEditOverlayProps {
  vp: ViewportParams;
}

export function TextEditOverlay({ vp }: TextEditOverlayProps) {
  const textEditObjectId = useUiStore((s) => s.textEditObjectId);
  const textEditClickPos = useUiStore((s) => s.textEditClickPos);
  const textEditMode = useUiStore((s) => s.textEditMode);
  const textEditCaretIndex = useUiStore((s) => s.textEditCaretIndex);
  const setTextEditObjectId = useUiStore((s) => s.setTextEditObjectId);
  const project = useProjectStore((s) => s.project);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const initialContentRef = useRef<string>('');
  const mountedRef = useRef(false);
  const [localContent, setLocalContent] = useState('');

  const obj = textEditObjectId
    ? project?.objects?.find((o) => o.id === textEditObjectId) ?? null
    : null;

  const textData = obj?.data?.type === 'text'
    ? (obj.data as Extract<ObjectData, { type: 'text' }>)
    : null;

  // Capture initial content, seed local state, and register pending edit on mount
  useEffect(() => {
    if (textData && obj) {
      initialContentRef.current = textData.content;
      setLocalContent(getPendingContentForObject(obj.id) ?? textData.content);
      setPendingEdit(obj.id, textData.content);
    }
    return () => releasePendingEdit(obj?.id);
  }, [textEditObjectId]); // eslint-disable-line react-hooks/exhaustive-deps

  // Auto-focus and mode-aware caret placement on mount
  useEffect(() => {
    if (textData && textareaRef.current) {
      requestAnimationFrame(() => {
        const ta = textareaRef.current;
        if (!ta) return;
        ta.focus();
        if (textEditMode === 'double-click') {
          ta.select();
        } else if (textEditMode === 'tool-click' && textEditCaretIndex != null) {
          ta.setSelectionRange(textEditCaretIndex, textEditCaretIndex);
        } else if (textEditMode === 'tool-click') {
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
    ta.style.height = 'auto';
    ta.style.height = `${Math.max(ta.scrollHeight, minH)}px`;
    ta.style.width = 'auto';
    ta.style.width = `${Math.max(ta.scrollWidth + 4, minW)}px`;
  });

  const handleChange = useCallback((e: React.ChangeEvent<HTMLTextAreaElement>) => {
    setLocalContent(e.target.value);
    updatePendingContent(e.target.value);
  }, []);

  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      // setTextEditObjectId(null) commits the pending edit, clears state,
      // and removes the object if it was a brand-new empty text.
      setTextEditObjectId(null);
      return;
    }
    // Enter inserts newline (textarea handles it natively — no preventDefault).
    // Stop propagation for all keys to prevent canvas tool shortcuts.
    e.stopPropagation();
  }, [setTextEditObjectId]);

  const handleBlur = useCallback(() => {
    setTextEditObjectId(null);
  }, [setTextEditObjectId]);

  if (!textEditObjectId || !obj || !textData) return null;

  const fontSize = worldToScreenDist(textData.font_size_mm, vp.zoom);

  // Use the layer's color for both text and caret.
  const layer = project?.layers?.find((l) => l.id === obj.layer_id);
  const layerColor = layer?.color_tag ?? '#ffffff';

  const fontStyle = `${textData.italic ? 'italic ' : ''}${textData.bold ? 'bold ' : ''}`;
  const minW = 60;
  // Tight line-height (equal to font size) so the caret's bottom edge
  // lines up with the text baseline — no descender gap below the caret.
  // Scales cleanly with any font size.
  const minH = Math.max(fontSize, 16);

  // Position: for new text, use click position; for existing text, use alignment anchor
  const alignment = textData.alignment ?? 'left';
  const alignmentV = textData.alignment_v ?? 'top';

  let worldX: number, worldY: number;
  if (textEditMode === 'new' && textEditClickPos) {
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

  return (
    <textarea
      key={textEditObjectId}
      ref={textareaRef}
      value={localContent}
      onChange={handleChange}
      onKeyDown={handleKeyDown}
      onBlur={handleBlur}
      wrap="off"
      className="absolute outline-none resize-none bg-transparent border-none"
      style={{
        left: screenPos.x,
        top: screenPos.y + vOffsetPx,
        minWidth: minW,
        minHeight: minH,
        font: `${fontStyle}${fontSize}px ${textData.font_family}`,
        textAlign: alignment,
        color: layerColor,
        padding: 0,
        margin: 0,
        lineHeight: `${fontSize}px`,
        overflow: 'hidden',
        whiteSpace: 'nowrap',
        zIndex: 20,
        caretColor: layerColor,
        transform,
      }}
      data-testid="text-edit-overlay"
    />
  );
}
