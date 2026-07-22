import { useState, useEffect, useRef } from 'react';
import type React from 'react';
import type { AnchorPoint, ProjectObject } from '../../types/project';

const TOOL_TEXT = 'text' as const;
const TEXT_ALIGNMENT_LEFT = 'left' as const;
const TEXT_ALIGNMENT_RIGHT = 'right' as const;
const TEXT_ALIGNMENT_TOP = 'top' as const;
const TEXT_ALIGNMENT_BOTTOM = 'bottom' as const;

export const anchorPoints: AnchorPoint[] = [
  'top_left', 'top_center', 'top_right',
  'center_left', 'center', 'center_right',
  'bottom_left', 'bottom_center', 'bottom_right',
];

export function getAnchorOffset(anchor: AnchorPoint, w: number, h: number): { ax: number; ay: number } {
  const col = anchorPoints.indexOf(anchor) % 3;
  const row = Math.floor(anchorPoints.indexOf(anchor) / 3);
  return { ax: (col / 2) * w, ay: (row / 2) * h };
}

/** For a single text object, compute the alignment anchor point. Non-text returns undefined. */
export function textAnchorPoint(obj: ProjectObject): { x: number; y: number } | undefined {
  if (obj.data.type !== TOOL_TEXT) return undefined;
  const { alignment, alignment_v } = obj.data;
  const b = obj.bounds;
  const w = b.max.x - b.min.x;
  const h = b.max.y - b.min.y;
  const x = alignment === TEXT_ALIGNMENT_LEFT ? b.min.x
          : alignment === TEXT_ALIGNMENT_RIGHT ? b.max.x
          : b.min.x + w / 2;
  const y = (alignment_v ?? TEXT_ALIGNMENT_TOP) === TEXT_ALIGNMENT_TOP ? b.min.y
          : alignment_v === TEXT_ALIGNMENT_BOTTOM ? b.max.y
          : b.min.y + h / 2;
  return { x, y };
}

export const anchorLabelKeys: Record<AnchorPoint, string> = {
  top_left: 'toolbars.properties.anchor.top_left',
  top_center: 'toolbars.properties.anchor.top_center',
  top_right: 'toolbars.properties.anchor.top_right',
  center_left: 'toolbars.properties.anchor.center_left',
  center: 'toolbars.properties.anchor.center',
  center_right: 'toolbars.properties.anchor.center_right',
  bottom_left: 'toolbars.properties.anchor.bottom_left',
  bottom_center: 'toolbars.properties.anchor.bottom_center',
  bottom_right: 'toolbars.properties.anchor.bottom_right',
};

export interface BufferedNumericFieldProps {
  value: string | number;
  onChange: (e: React.ChangeEvent<HTMLInputElement>) => void;
  onBlur: (e: React.FocusEvent<HTMLInputElement>) => void;
  onKeyDown: (e: React.KeyboardEvent<HTMLInputElement>) => void;
}

/**
 * NumberStepper's arrow buttons synthesize a plain `Event('input')`, while real
 * typing (and paste) always arrives as an `InputEvent`. Stepper-driven changes
 * commit immediately per click; typed changes buffer until blur/Enter.
 */
function isStepperCommitEvent(e: React.ChangeEvent<HTMLInputElement>): boolean {
  const native: Event = e.nativeEvent;
  return native.type === 'input'
    && (typeof InputEvent === 'undefined' || !(native instanceof InputEvent));
}

/**
 * Buffers typed input locally so each keystroke does not dispatch a store commit
 * (IPC round-trip + undo entry). Commits on blur or Enter; Escape reverts the
 * buffer to the committed value. Stepper-arrow clicks still commit per click.
 * While an uncommitted edit is pending, external/store updates never clobber the
 * buffer; once committed, the buffer is released when the store-derived value
 * catches up (avoids flashing the stale value during the async IPC round-trip).
 */
export function useBufferedNumericField(
  committedValue: number | string,
  onCommit: (value: number) => void,
  resetKey: string,
): BufferedNumericFieldProps {
  const [buffer, setBuffer] = useState<string | null>(null);
  const dirtyRef = useRef(false);

  // Selection or display-unit changes invalidate any pending edit.
  useEffect(() => {
    dirtyRef.current = false;
    setBuffer(null);
  }, [resetKey]);

  // Release a committed (non-dirty) buffer once the store-derived value changes.
  useEffect(() => {
    if (!dirtyRef.current) setBuffer(null);
  }, [committedValue]);

  const commit = (raw: string) => {
    dirtyRef.current = false;
    const value = Number(raw);
    if (raw.trim() === '' || !Number.isFinite(value)) {
      setBuffer(null); // invalid input reverts to the committed value
      return;
    }
    onCommit(value);
  };

  return {
    value: buffer ?? committedValue,
    onChange: (e) => {
      if (isStepperCommitEvent(e)) {
        setBuffer(null);
        commit(e.target.value);
      } else {
        dirtyRef.current = true;
        setBuffer(e.target.value);
      }
    },
    onBlur: (e) => {
      if (buffer === null) return;
      if (dirtyRef.current) commit(e.target.value);
      setBuffer(null);
    },
    onKeyDown: (e) => {
      if (e.key === 'Enter') {
        if (dirtyRef.current) commit(e.currentTarget.value);
      } else if (e.key === 'Escape') {
        dirtyRef.current = false;
        setBuffer(null);
      }
    },
  };
}
