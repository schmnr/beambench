export interface CanvasViewportSize {
  width: number;
  height: number;
}

let currentCanvasViewportSize: CanvasViewportSize | null = null;

export function setCanvasViewportSize(size: CanvasViewportSize): void {
  if (!Number.isFinite(size.width) || !Number.isFinite(size.height)) return;
  if (size.width <= 0 || size.height <= 0) return;
  currentCanvasViewportSize = {
    width: size.width,
    height: size.height,
  };
}

export function clearCanvasViewportSize(): void {
  currentCanvasViewportSize = null;
}

export function getCanvasViewportSize(): CanvasViewportSize | null {
  return currentCanvasViewportSize;
}
