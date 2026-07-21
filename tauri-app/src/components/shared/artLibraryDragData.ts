export const ART_LIBRARY_DRAG_MIME = 'application/x-beambench-art-library-item';

export interface ArtLibraryTransferPayload {
  sourceLibraryId: string;
  itemId: string;
}

interface DragDataReader {
  types?: Iterable<string> | ArrayLike<string>;
  getData?: (format: string) => string;
}

function hasTransferType(types: DragDataReader['types'], mime: string): boolean {
  if (!types) return false;
  return Array.from(types).includes(mime);
}

export function encodeArtLibraryDragData(payload: ArtLibraryTransferPayload): string {
  return JSON.stringify(payload);
}

export function parseArtLibraryDragData(raw: string): ArtLibraryTransferPayload | null {
  try {
    const parsed = JSON.parse(raw) as Partial<ArtLibraryTransferPayload>;
    if (typeof parsed.sourceLibraryId !== 'string' || typeof parsed.itemId !== 'string') {
      return null;
    }
    return {
      sourceLibraryId: parsed.sourceLibraryId,
      itemId: parsed.itemId,
    };
  } catch {
    return null;
  }
}

export function getArtLibraryDragData(dataTransfer: DragDataReader | null | undefined): ArtLibraryTransferPayload | null {
  if (!dataTransfer?.getData) return null;
  const raw = dataTransfer.getData(ART_LIBRARY_DRAG_MIME);
  if (!raw) return null;
  return parseArtLibraryDragData(raw);
}

export function isArtLibraryDragDataTransfer(dataTransfer: DragDataReader | null | undefined): boolean {
  if (!dataTransfer) return false;
  return hasTransferType(dataTransfer.types, ART_LIBRARY_DRAG_MIME) || getArtLibraryDragData(dataTransfer) !== null;
}
