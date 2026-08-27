export interface PreparedCanvasSurface {
  ctx: CanvasRenderingContext2D;
  resized: boolean;
}

/**
 * Keep the backing store stable at fractional display scales. Canvas width and
 * height are integers, so comparing them with an unrounded CSS-size × DPR value
 * causes every render to resize and clear the bitmap on Windows at 125–175%.
 */
export function prepareCanvasSurface(
  canvas: HTMLCanvasElement,
  cssWidth: number,
  cssHeight: number,
  dpr: number,
): PreparedCanvasSurface | null {
  const scale = Number.isFinite(dpr) && dpr > 0 ? dpr : 1;
  const targetWidth = Math.max(1, Math.round(cssWidth * scale));
  const targetHeight = Math.max(1, Math.round(cssHeight * scale));
  let resized = false;

  if (canvas.width !== targetWidth) {
    canvas.width = targetWidth;
    resized = true;
  }
  if (canvas.height !== targetHeight) {
    canvas.height = targetHeight;
    resized = true;
  }

  const ctx = canvas.getContext('2d');
  if (!ctx) return null;
  ctx.setTransform(scale, 0, 0, scale, 0, 0);
  return { ctx, resized };
}

/** Replace the visible overlay in one draw instead of clearing it in place. */
export function presentCanvasBuffer(
  target: CanvasRenderingContext2D,
  buffer: HTMLCanvasElement,
): void {
  target.save();
  target.setTransform(1, 0, 0, 1, 0, 0);
  target.globalCompositeOperation = 'copy';
  target.drawImage(buffer, 0, 0);
  target.restore();
}
