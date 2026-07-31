import type { ProjectObject } from '../types/project';

export type LayerStackLayer = { id: string; visible?: boolean };

/** True when both the object and its containing layer are visible. */
export function isObjectVisibleInLayerStack(
  object: ProjectObject,
  layers?: LayerStackLayer[],
): boolean {
  if (!object.visible) return false;
  if (!layers) return true;
  return layers.find((layer) => layer.id === object.layer_id)?.visible !== false;
}

/**
 * Canvas draw order from back to front. Layer tabs are ordered front-to-back
 * (the first/leftmost layer is visually on top), while objects within a layer
 * retain their z-order.
 */
export function sortObjectsForLayerStack(
  objects: ProjectObject[],
  layers: LayerStackLayer[],
): ProjectObject[] {
  const layerIndex = new Map(layers.map((layer, index) => [layer.id, index]));
  const unknownLayerIndex = layers.length;
  return [...objects].sort((a, b) => {
    const aLayerIndex = layerIndex.get(a.layer_id) ?? unknownLayerIndex;
    const bLayerIndex = layerIndex.get(b.layer_id) ?? unknownLayerIndex;
    const layerOrder = bLayerIndex - aLayerIndex;
    return layerOrder !== 0 ? layerOrder : a.z_index - b.z_index;
  });
}

/** Canvas hit-test order from front to back, matching the visible layer stack. */
export function sortObjectsForHitTesting(
  objects: ProjectObject[],
  layers: LayerStackLayer[],
): ProjectObject[] {
  return sortObjectsForLayerStack(objects, layers).reverse();
}
