import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import {
  composeCanvasLayers,
  encodeBmp24,
  exportCanvasScreenshot,
  registerCanvasScreenshotProvider,
} from './canvasScreenshotExportService';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

type MockCanvasContext = {
  fillStyle: string;
  save: ReturnType<typeof vi.fn>;
  restore: ReturnType<typeof vi.fn>;
  fillRect: ReturnType<typeof vi.fn>;
  drawImage: ReturnType<typeof vi.fn>;
  getImageData: ReturnType<typeof vi.fn>;
};

function createMockContext(imageData?: Uint8ClampedArray): MockCanvasContext {
  return {
    fillStyle: '',
    save: vi.fn(),
    restore: vi.fn(),
    fillRect: vi.fn(),
    drawImage: vi.fn(),
    getImageData: vi.fn(() => ({
      data: imageData ?? new Uint8ClampedArray([255, 255, 255, 255]),
    })),
  };
}

function createMockCanvas(
  width: number,
  height: number,
  context = createMockContext(),
  toBlobImpl?: HTMLCanvasElement['toBlob'],
): HTMLCanvasElement {
  return {
    width,
    height,
    getContext: vi.fn((contextId: string) => (contextId === '2d' ? context : null)),
    toBlob: vi.fn(toBlobImpl ?? ((callback: BlobCallback) => {
      callback({
        arrayBuffer: async () => new Uint8Array([1, 2, 3]).buffer,
      } as Blob);
    })),
  } as unknown as HTMLCanvasElement;
}

function mockCanvasCreation(canvas: HTMLCanvasElement) {
  const createElement = document.createElement.bind(document);
  return vi.spyOn(document, 'createElement').mockImplementation((tagName, options) => {
    if (tagName.toLowerCase() === 'canvas') {
      return canvas;
    }
    return createElement(tagName, options);
  });
}

beforeEach(() => {
  vi.mocked(invoke).mockReset();
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe('canvasScreenshotExportService', () => {
  it('composites base then overlay using backing-store dimensions', () => {
    const outputContext = createMockContext();
    const outputCanvas = createMockCanvas(0, 0, outputContext);
    const baseCanvas = createMockCanvas(400, 240);
    const overlayCanvas = createMockCanvas(400, 240);
    mockCanvasCreation(outputCanvas);

    const result = composeCanvasLayers({ baseCanvas, overlayCanvas });

    expect(result.width).toBe(400);
    expect(result.height).toBe(240);
    expect(outputContext.drawImage).toHaveBeenNthCalledWith(1, baseCanvas, 0, 0, 400, 240);
    expect(outputContext.drawImage).toHaveBeenNthCalledWith(2, overlayCanvas, 0, 0, 400, 240);
    expect(outputContext.fillRect).not.toHaveBeenCalled();
  });

  it('flattens JPG and BMP captures onto white before drawing canvas layers', () => {
    const outputContext = createMockContext();
    const outputCanvas = createMockCanvas(0, 0, outputContext);
    const baseCanvas = createMockCanvas(10, 8);
    const overlayCanvas = createMockCanvas(10, 8);
    mockCanvasCreation(outputCanvas);

    composeCanvasLayers({ baseCanvas, overlayCanvas }, { flattenWhite: true });

    expect(outputContext.fillStyle).toBe('#ffffff');
    expect(outputContext.fillRect).toHaveBeenCalledWith(0, 0, 10, 8);
    expect(outputContext.fillRect.mock.invocationCallOrder[0])
      .toBeLessThan(outputContext.drawImage.mock.invocationCallOrder[0]);
  });

  it('exports PNG bytes through the narrow Tauri byte writer', async () => {
    const outputContext = createMockContext();
    const outputCanvas = createMockCanvas(0, 0, outputContext, (callback) => {
      callback({
        arrayBuffer: async () => new Uint8Array([9, 8, 7]).buffer,
      } as Blob);
    });
    const baseCanvas = createMockCanvas(20, 12);
    const overlayCanvas = createMockCanvas(20, 12);
    const unregister = registerCanvasScreenshotProvider(() => ({ baseCanvas, overlayCanvas }));
    mockCanvasCreation(outputCanvas);
    vi.mocked(invoke).mockResolvedValue('/tmp/out.png');

    try {
      await expect(exportCanvasScreenshot('/tmp/out.png', 'png')).resolves.toBe('/tmp/out.png');
    } finally {
      unregister();
    }

    expect(outputCanvas.toBlob).toHaveBeenCalledWith(expect.any(Function), 'image/png', undefined);
    expect(invoke).toHaveBeenCalledWith('write_export_bytes', {
      path: '/tmp/out.png',
      bytes: [9, 8, 7],
    });
  });

  it('exports BMP bytes without using browser toBlob encoding', async () => {
    const pixels = new Uint8ClampedArray([
      255, 0, 0, 255,
      0, 255, 0, 255,
      0, 0, 255, 255,
      255, 255, 255, 255,
    ]);
    const outputContext = createMockContext(pixels);
    const outputCanvas = createMockCanvas(0, 0, outputContext);
    const baseCanvas = createMockCanvas(2, 2);
    const overlayCanvas = createMockCanvas(2, 2);
    const unregister = registerCanvasScreenshotProvider(() => ({ baseCanvas, overlayCanvas }));
    mockCanvasCreation(outputCanvas);
    vi.mocked(invoke).mockResolvedValue('/tmp/out.bmp');

    try {
      await exportCanvasScreenshot('/tmp/out.bmp', 'bmp');
    } finally {
      unregister();
    }

    expect(outputCanvas.toBlob).not.toHaveBeenCalled();
    expect(invoke).toHaveBeenCalledWith('write_export_bytes', {
      path: '/tmp/out.bmp',
      bytes: Array.from(encodeBmp24(outputCanvas)),
    });
  });

  it('encodes a 24-bit BMP with padded bottom-up BGR rows', () => {
    const pixels = new Uint8ClampedArray([
      255, 0, 0, 255,
      0, 255, 0, 255,
      0, 0, 255, 255,
      255, 255, 255, 255,
    ]);
    const canvas = createMockCanvas(2, 2, createMockContext(pixels));

    const bmp = encodeBmp24(canvas);
    const view = new DataView(bmp.buffer);

    expect(String.fromCharCode(bmp[0], bmp[1])).toBe('BM');
    expect(view.getUint32(2, true)).toBe(70);
    expect(view.getUint32(10, true)).toBe(54);
    expect(view.getInt32(18, true)).toBe(2);
    expect(view.getInt32(22, true)).toBe(2);
    expect(view.getUint16(28, true)).toBe(24);
    expect(Array.from(bmp.slice(54, 70))).toEqual([
      255, 0, 0,
      255, 255, 255,
      0, 0,
      0, 0, 255,
      0, 255, 0,
      0, 0,
    ]);
  });
});
