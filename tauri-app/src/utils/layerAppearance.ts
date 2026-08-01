import type { Layer } from '../types/project';

const FILLED_OPERATIONS = new Set(['fill', 'offset_fill', 'image']);

/** Whether a layer should read as a filled surface on the Design canvas. */
export function layerUsesFilledAppearance(layer: Layer): boolean {
  return layer.entries.some((entry) => FILLED_OPERATIONS.has(entry.operation));
}

/** Clamp persisted or legacy layer opacity to a canvas-safe value. */
export function layerFillOpacity(layer: Layer): number {
  return Math.min(1, Math.max(0, layer.fill_opacity ?? 1));
}
