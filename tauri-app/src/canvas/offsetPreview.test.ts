import { describe, expect, it } from 'vitest';
import { makeProjectObject } from '../test-utils/projectFixtures';
import { createOffsetPreviewSourceFrame, offsetPreviewTranslation } from './offsetPreview';

describe('offset preview source tracking', () => {
  it('returns the exact translation while a source object is dragged', () => {
    const original = makeProjectObject({
      id: 'shape-1',
      bounds: { min: { x: 10, y: 20 }, max: { x: 30, y: 50 } },
    });
    const sourceFrame = createOffsetPreviewSourceFrame([original], ['shape-1']);
    const moved = makeProjectObject({
      ...original,
      bounds: { min: { x: 17, y: 16 }, max: { x: 37, y: 46 } },
    });

    expect(offsetPreviewTranslation(sourceFrame, [moved])).toEqual({ x: 7, y: -4 });
  });

  it('does not fake an offset result while the source is being resized', () => {
    const original = makeProjectObject({
      id: 'shape-1',
      bounds: { min: { x: 10, y: 20 }, max: { x: 30, y: 50 } },
    });
    const sourceFrame = createOffsetPreviewSourceFrame([original], ['shape-1']);
    const resized = makeProjectObject({
      ...original,
      bounds: { min: { x: 10, y: 20 }, max: { x: 40, y: 50 } },
    });

    expect(offsetPreviewTranslation(sourceFrame, [resized])).toBeNull();
  });
});
