import { describe, expect, it } from 'vitest';

import {
  getClipboardArtworkBlobs,
  isSupportedClipboardArtworkType,
} from '../systemClipboard';

function makePasteEvent(clipboardData: Partial<DataTransfer>): ClipboardEvent {
  return { clipboardData } as ClipboardEvent;
}

describe('systemClipboard', () => {
  it('accepts raster and SVG artwork MIME types', () => {
    expect(isSupportedClipboardArtworkType('image/png')).toBe(true);
    expect(isSupportedClipboardArtworkType('image/jpeg')).toBe(true);
    expect(isSupportedClipboardArtworkType('image/svg+xml')).toBe(true);
    expect(isSupportedClipboardArtworkType('text/plain')).toBe(false);
  });

  it('extracts pasted image files from clipboard items', () => {
    const file = new File([new Uint8Array([1, 2, 3])], 'shot.png', { type: 'image/png' });
    const event = makePasteEvent({
      items: [
        {
          kind: 'file',
          type: 'image/png',
          getAsFile: () => file,
        } as DataTransferItem,
      ] as unknown as DataTransferItemList,
      files: [] as unknown as FileList,
      getData: () => '',
    });

    expect(getClipboardArtworkBlobs(event)).toEqual([
      {
        blob: file,
        filename: 'shot.png',
        mediaType: 'image/png',
      },
    ]);
  });

  it('infers artwork type from filename when the clipboard file has no MIME type', () => {
    const file = new File([new Uint8Array([1, 2, 3])], 'mark.svg', { type: '' });
    const event = makePasteEvent({
      items: [] as unknown as DataTransferItemList,
      files: [file] as unknown as FileList,
      getData: () => '',
    });

    expect(getClipboardArtworkBlobs(event)).toEqual([
      {
        blob: file,
        filename: 'mark.svg',
        mediaType: 'image/svg+xml',
      },
    ]);
  });

  it('falls back to SVG text when no file item is available', async () => {
    const event = makePasteEvent({
      items: [] as unknown as DataTransferItemList,
      files: [] as unknown as FileList,
      getData: (type: string) => (type === 'text/plain' ? '<svg viewBox="0 0 10 10" />' : ''),
    });

    const blobs = getClipboardArtworkBlobs(event);

    expect(blobs).toHaveLength(1);
    expect(blobs[0].filename).toBe('Clipboard Artwork.svg');
    expect(blobs[0].mediaType).toBe('image/svg+xml');
    expect(blobs[0].blob.size).toBeGreaterThan(0);
  });

  it('ignores non-artwork clipboard contents', () => {
    const event = makePasteEvent({
      items: [] as unknown as DataTransferItemList,
      files: [] as unknown as FileList,
      getData: () => 'plain text',
    });

    expect(getClipboardArtworkBlobs(event)).toEqual([]);
  });
});
