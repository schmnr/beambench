import { describe, expect, it } from 'vitest';

import {
  buildCanvasArtLibraryDragOverState,
  buildCanvasArtLibraryDropPayload,
  getCanvasArtLibraryDropEffect,
  resolveCanvasArtLibraryDragState,
  type CanvasArtLibraryDragState,
} from '../artLibraryCanvasDrop';
import type { ViewportParams } from '../../../canvas/ViewportTransform';
import {
  ART_LIBRARY_DRAG_MIME,
  encodeArtLibraryDragData,
  isArtLibraryDragDataTransfer,
} from '../../shared/artLibraryDragData';

const dragState: CanvasArtLibraryDragState = {
  sourceLibraryId: 'library-1',
  itemId: 'item-1',
  dropEffect: 'move',
  dropAllowed: false,
  targetLibraryId: 'library-2',
};

const vp: ViewportParams = {
  offset: { x: 10, y: 20 },
  zoom: 100,
  canvasWidth: 800,
  canvasHeight: 600,
};

describe('artLibraryCanvasDrop', () => {
  it('keeps canvas art-library drops copy-only even when shift is held', () => {
    expect(getCanvasArtLibraryDropEffect(false)).toBe('copy');
    expect(getCanvasArtLibraryDropEffect(true)).toBe('copy');
  });

  it('builds drag-over state that clears library targets and forces copy mode', () => {
    expect(buildCanvasArtLibraryDragOverState(dragState, true)).toEqual({
      ...dragState,
      dropAllowed: true,
      targetLibraryId: null,
      dropEffect: 'copy',
    });
  });

  it('builds a canvas drop payload with world coordinates for insertToProject', () => {
    const payload = buildCanvasArtLibraryDropPayload({
      dragState,
      clientX: 500,
      clientY: 360,
      canvasRect: { left: 100, top: 60 },
      vp,
    });

    expect(payload).toEqual({
      libraryId: 'library-1',
      itemId: 'item-1',
      world: { x: 10, y: 20 },
    });
  });

  it('resolves drag state from dataTransfer payload when render state is stale', () => {
    expect(resolveCanvasArtLibraryDragState({
      dragState: null,
      dataTransfer: {
        getData: (format: string) => (
          format === ART_LIBRARY_DRAG_MIME
            ? encodeArtLibraryDragData({ sourceLibraryId: 'library-9', itemId: 'item-9' })
            : ''
        ),
      },
    })).toEqual({
      sourceLibraryId: 'library-9',
      itemId: 'item-9',
      dropEffect: 'copy',
      dropAllowed: true,
      targetLibraryId: null,
    });
  });

  it('recognizes art-library drags from the transfer type before payload data is readable', () => {
    expect(isArtLibraryDragDataTransfer({
      types: [ART_LIBRARY_DRAG_MIME],
      getData: () => '',
    })).toBe(true);
  });
});
