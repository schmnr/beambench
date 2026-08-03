import type { Bounds, Point2D, ProjectObject } from '../types/project';

export interface OffsetPreviewSourceFrame {
  objectIds: string[];
  bounds: Bounds;
}

export function createOffsetPreviewSourceFrame(
  objects: ProjectObject[],
  objectIds: string[],
): OffsetPreviewSourceFrame | null {
  const ids = new Set(objectIds);
  const sources = objects.filter((object) => ids.has(object.id));
  if (sources.length === 0) return null;

  return {
    objectIds: [...objectIds],
    bounds: {
      min: {
        x: Math.min(...sources.map((object) => object.bounds.min.x)),
        y: Math.min(...sources.map((object) => object.bounds.min.y)),
      },
      max: {
        x: Math.max(...sources.map((object) => object.bounds.max.x)),
        y: Math.max(...sources.map((object) => object.bounds.max.y)),
      },
    },
  };
}

/**
 * Follow a source selection during its optimistic canvas drag. Offset preview
 * paths are world-space geometry produced by the backend, which is not updated
 * until mouse-up. A pure translation can be applied exactly on the canvas in
 * the meantime; resize/rotate/shear previews are regenerated after commit.
 */
export function offsetPreviewTranslation(
  sourceFrame: OffsetPreviewSourceFrame | null,
  objects: ProjectObject[],
): Point2D | null {
  if (!sourceFrame) return null;
  const currentFrame = createOffsetPreviewSourceFrame(objects, sourceFrame.objectIds);
  if (!currentFrame) return null;

  const originalWidth = sourceFrame.bounds.max.x - sourceFrame.bounds.min.x;
  const originalHeight = sourceFrame.bounds.max.y - sourceFrame.bounds.min.y;
  const currentWidth = currentFrame.bounds.max.x - currentFrame.bounds.min.x;
  const currentHeight = currentFrame.bounds.max.y - currentFrame.bounds.min.y;
  if (
    Math.abs(originalWidth - currentWidth) > 1e-6
    || Math.abs(originalHeight - currentHeight) > 1e-6
  ) {
    return null;
  }

  return {
    x: currentFrame.bounds.min.x - sourceFrame.bounds.min.x,
    y: currentFrame.bounds.min.y - sourceFrame.bounds.min.y,
  };
}
