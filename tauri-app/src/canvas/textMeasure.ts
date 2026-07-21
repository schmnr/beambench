import type { ProjectObject, Transform2D } from '../types/project';
import type { ViewportParams } from './ViewportTransform';
import { worldToScreen, worldToScreenDist } from './ViewportTransform';

/** Offscreen context for text measurement (created once, reused). */
let measureCtx: CanvasRenderingContext2D | null = null;
function getMeasureCtx(): CanvasRenderingContext2D {
  if (!measureCtx) {
    const canvas = document.createElement('canvas');
    canvas.width = 1; canvas.height = 1;
    measureCtx = canvas.getContext('2d')!;
  }
  return measureCtx;
}

/** Check if a transform is effectively identity (no rotation/scale/skew). */
export function isIdentityTransform(t: Transform2D): boolean {
  const EPS = 1e-6;
  return Math.abs(t.a - 1) < EPS && Math.abs(t.b) < EPS &&
         Math.abs(t.c) < EPS && Math.abs(t.d - 1) < EPS &&
         Math.abs(t.tx) < EPS && Math.abs(t.ty) < EPS;
}

/**
 * Inverse-transform a point from the transformed coordinate frame back to
 * the untransformed frame.  The canvas transform pivots around bounds center:
 *   x' = a*(x-cx) + c*(y-cy) + tx + cx
 *   y' = b*(x-cx) + d*(y-cy) + ty + cy
 * Returns null when the matrix is singular (det ≈ 0).
 */
export function applyInverseTransformToPoint(
  transformedPt: { x: number; y: number },
  transform: Transform2D,
  bounds: { min: { x: number; y: number }; max: { x: number; y: number } },
): { x: number; y: number } | null {
  const det = transform.a * transform.d - transform.c * transform.b;
  if (Math.abs(det) < 1e-10) return null;
  const cx = (bounds.min.x + bounds.max.x) / 2;
  const cy = (bounds.min.y + bounds.max.y) / 2;
  const dx = transformedPt.x - cx - transform.tx;
  const dy = transformedPt.y - cy - transform.ty;
  const invDet = 1 / det;
  return {
    x: (transform.d * dx - transform.c * dy) * invDet + cx,
    y: (-transform.b * dx + transform.a * dy) * invDet + cy,
  };
}

/** Compute effective text layout mode (on_path + straight → 'path'). */
function getEffectiveLayout(d: { layout_mode?: string; on_path?: boolean; transform_style?: string }): string {
  if (d.transform_style && d.transform_style !== 'none') return 'transform';
  const mode = d.layout_mode ?? 'straight';
  if (d.on_path && mode === 'straight') return 'path';
  return mode;
}

/**
 * Convert a world-space click inside a text object into a caret index
 * (character offset into the content string, accounting for newlines).
 *
 * Mirrors drawText() from drawObjects.ts for straight text only.
 * Returns null when caret-at-click is not possible (non-straight layout,
 * non-identity transform), signaling caller to use select-all instead.
 */
export function getCaretIndexFromClick(
  worldClick: { x: number; y: number },
  obj: ProjectObject,
  vp: ViewportParams,
): number | null {
  if (obj.data.type !== 'text') return null;
  const d = obj.data;

  // Non-straight layout (path/bend): can't measure character positions
  if (getEffectiveLayout(d) !== 'straight') return null;

  // Non-identity transform: inverse-map the click to untransformed space
  let effectiveClick = worldClick;
  if (!isIdentityTransform(obj.transform)) {
    const invPt = applyInverseTransformToPoint(worldClick, obj.transform, obj.bounds);
    if (!invPt) return null; // singular matrix
    effectiveClick = invPt;
  }

  // max_width: Rust word-wraps lines exceeding max_width and applies squeeze
  if (d.max_width != null && d.max_width > 0) return null;

  const ctx = getMeasureCtx();
  const fontSize = worldToScreenDist(d.font_size_mm, vp.zoom);
  const style = `${d.italic ? 'italic ' : ''}${d.bold ? 'bold ' : ''}`;
  ctx.font = `${style}${fontSize}px ${d.font_family}`;

  const displayText = d.upper_case ? d.content.toUpperCase() : d.content;
  const lines = displayText.split('\n');

  const topLeft = worldToScreen(obj.bounds.min, vp);
  const screenClick = worldToScreen(effectiveClick, vp);
  const boundsW = worldToScreenDist(obj.bounds.max.x - obj.bounds.min.x, vp.zoom);
  const boundsH = worldToScreenDist(obj.bounds.max.y - obj.bounds.min.y, vp.zoom);
  const letterSpacing = d.h_spacing ? worldToScreenDist(d.h_spacing, vp.zoom) : 0;
  const lineHeight = fontSize + (d.v_spacing ? worldToScreenDist(d.v_spacing, vp.zoom) : 0);
  const totalTextH = lines.length > 1 ? fontSize + (lines.length - 1) * lineHeight : fontSize;

  // Vertical alignment baseline (matches drawText)
  let baseY: number;
  if (d.alignment_v === 'middle') baseY = topLeft.y + (boundsH - totalTextH) / 2;
  else if (d.alignment_v === 'bottom') baseY = topLeft.y + boundsH - totalTextH;
  else baseY = topLeft.y;

  // Determine which line was clicked
  const relY = screenClick.y - baseY;
  const lineIdx = Math.max(0, Math.min(lines.length - 1, Math.floor(relY / lineHeight)));

  // Measure character widths for that line (matches drawText per-char measurement)
  const lineText = lines[lineIdx];
  const charWidths: number[] = [];
  let naturalW = 0;
  for (const ch of lineText) {
    const cw = ctx.measureText(ch).width;
    charWidths.push(cw);
    naturalW += cw;
  }
  const totalW = naturalW + letterSpacing * Math.max(0, charWidths.length - 1);

  // Line starting X (matches drawText alignment)
  let sx = topLeft.x;
  if (d.alignment === 'center') sx = topLeft.x + (boundsW - totalW) / 2;
  else if (d.alignment === 'right') sx = topLeft.x + boundsW - totalW;

  // Find character index by bisecting at character midpoints
  const relX = screenClick.x - sx;
  let cx = 0;
  let charInLine = lineText.length; // default: past end
  for (let i = 0; i < charWidths.length; i++) {
    const mid = cx + charWidths[i] / 2;
    if (relX < mid) { charInLine = i; break; }
    cx += charWidths[i] + letterSpacing;
  }

  // Convert (lineIdx, charInLine) → flat index into original content
  const contentLines = d.content.split('\n');
  let flatIndex = 0;
  for (let i = 0; i < lineIdx && i < contentLines.length; i++) {
    flatIndex += contentLines[i].length + 1; // +1 for the '\n'
  }
  flatIndex += Math.min(charInLine, contentLines[lineIdx]?.length ?? 0);

  return Math.min(flatIndex, d.content.length);
}
