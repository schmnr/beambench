import { screenToWorld, type ViewportParams } from '../../canvas/ViewportTransform';
import type { Point2D } from '../../types/project';
import { getArtLibraryDragData } from '../shared/artLibraryDragData';

export interface CanvasArtLibraryDragState {
  sourceLibraryId: string;
  itemId: string;
  dropEffect: 'copy' | 'move';
  dropAllowed: boolean;
  targetLibraryId: string | null;
}

export interface CanvasArtLibraryDropPayload {
  libraryId: string;
  itemId: string;
  world: Point2D;
}

interface CanvasRectLike {
  left: number;
  top: number;
}

interface DragDataReader {
  getData?: (format: string) => string;
}

export function getCanvasArtLibraryDropEffect(_shiftKey: boolean): 'copy' {
  return 'copy';
}

export function buildCanvasArtLibraryDragOverState<T extends CanvasArtLibraryDragState>(
  dragState: T,
  shiftKey: boolean,
): T {
  return {
    ...dragState,
    dropAllowed: true,
    targetLibraryId: null,
    dropEffect: getCanvasArtLibraryDropEffect(shiftKey),
  };
}

export function resolveCanvasArtLibraryDragState(params: {
  dragState: CanvasArtLibraryDragState | null;
  dataTransfer?: DragDataReader | null;
}): CanvasArtLibraryDragState | null {
  if (params.dragState) return params.dragState;
  const transferPayload = getArtLibraryDragData(params.dataTransfer);
  if (!transferPayload) return null;
  return {
    sourceLibraryId: transferPayload.sourceLibraryId,
    itemId: transferPayload.itemId,
    dropEffect: 'copy',
    dropAllowed: true,
    targetLibraryId: null,
  };
}

export function buildCanvasArtLibraryDropPayload(params: {
  dragState: CanvasArtLibraryDragState;
  clientX: number;
  clientY: number;
  canvasRect: CanvasRectLike;
  vp: ViewportParams;
}): CanvasArtLibraryDropPayload {
  const { dragState, clientX, clientY, canvasRect, vp } = params;
  return {
    libraryId: dragState.sourceLibraryId,
    itemId: dragState.itemId,
    world: screenToWorld(
      { x: clientX - canvasRect.left, y: clientY - canvasRect.top },
      vp,
    ),
  };
}
