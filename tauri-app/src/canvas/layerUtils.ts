/**
 * Smart layer selection helpers.
 * Vector objects (shapes, paths, text) belong on Line/Score/Cut layers.
 * Raster objects (images) belong on Image/Fill layers.
 */

type LayerLike = { id: string; enabled: boolean; operation?: string };

/** Find the first enabled vector-compatible layer (line, score, or cut). */
export function findVectorLayer(layers: LayerLike[]): LayerLike | undefined {
  return layers.find(
    (l) => l.enabled && l.operation != null && !['image', 'fill'].includes(l.operation),
  );
}

/** Find the first enabled raster-compatible layer (image or fill). */
export function findRasterLayer(layers: LayerLike[]): LayerLike | undefined {
  return layers.find(
    (l) => l.enabled && l.operation != null && ['image', 'fill'].includes(l.operation),
  );
}
