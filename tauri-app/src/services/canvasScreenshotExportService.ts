import { invoke } from '@tauri-apps/api/core';

export type CanvasScreenshotFormat = 'png' | 'jpg' | 'bmp';

export interface CanvasScreenshotSource {
  baseCanvas: HTMLCanvasElement;
  overlayCanvas: HTMLCanvasElement;
}

type CanvasScreenshotProvider = () => CanvasScreenshotSource | null;

let screenshotProvider: CanvasScreenshotProvider | null = null;

export function registerCanvasScreenshotProvider(provider: CanvasScreenshotProvider): () => void {
  screenshotProvider = provider;
  return () => {
    if (screenshotProvider === provider) {
      screenshotProvider = null;
    }
  };
}

function getScreenshotSource(): CanvasScreenshotSource {
  const source = screenshotProvider?.() ?? null;
  if (!source?.baseCanvas || !source.overlayCanvas) {
    throw new Error('Canvas is not ready for bitmap export');
  }
  if (source.baseCanvas.width <= 0 || source.baseCanvas.height <= 0) {
    throw new Error('Canvas has no pixels to export');
  }
  return source;
}

export function composeCanvasLayers(
  source: CanvasScreenshotSource,
  options: { flattenWhite?: boolean } = {},
): HTMLCanvasElement {
  const { baseCanvas, overlayCanvas } = source;
  const width = baseCanvas.width;
  const height = baseCanvas.height;
  const canvas = document.createElement('canvas');
  canvas.width = width;
  canvas.height = height;

  const ctx = canvas.getContext('2d');
  if (!ctx) {
    throw new Error('Unable to create bitmap export canvas');
  }

  if (options.flattenWhite) {
    ctx.save();
    ctx.fillStyle = '#ffffff';
    ctx.fillRect(0, 0, width, height);
    ctx.restore();
  }

  ctx.drawImage(baseCanvas, 0, 0, width, height);
  ctx.drawImage(overlayCanvas, 0, 0, width, height);
  return canvas;
}

function canvasToBlob(canvas: HTMLCanvasElement, type: string, quality?: number): Promise<Blob> {
  return new Promise((resolve, reject) => {
    canvas.toBlob((blob) => {
      if (!blob) {
        reject(new Error(`Failed to encode ${type} export`));
        return;
      }
      resolve(blob);
    }, type, quality);
  });
}

async function canvasToBytes(canvas: HTMLCanvasElement, type: string, quality?: number): Promise<Uint8Array> {
  const blob = await canvasToBlob(canvas, type, quality);
  return new Uint8Array(await blob.arrayBuffer());
}

export function encodeBmp24(canvas: HTMLCanvasElement): Uint8Array {
  const width = canvas.width;
  const height = canvas.height;
  if (width <= 0 || height <= 0) {
    throw new Error('BMP export requires a non-empty canvas');
  }

  const ctx = canvas.getContext('2d');
  if (!ctx) {
    throw new Error('Unable to read bitmap export pixels');
  }

  const { data } = ctx.getImageData(0, 0, width, height);
  const rowBytes = width * 3;
  const rowPadding = (4 - (rowBytes % 4)) % 4;
  const rowStride = rowBytes + rowPadding;
  const pixelDataSize = rowStride * height;
  const fileHeaderSize = 14;
  const dibHeaderSize = 40;
  const pixelOffset = fileHeaderSize + dibHeaderSize;
  const fileSize = pixelOffset + pixelDataSize;

  const bytes = new Uint8Array(fileSize);
  const view = new DataView(bytes.buffer);

  bytes[0] = 0x42; // B
  bytes[1] = 0x4d; // M
  view.setUint32(2, fileSize, true);
  view.setUint32(10, pixelOffset, true);

  view.setUint32(14, dibHeaderSize, true);
  view.setInt32(18, width, true);
  view.setInt32(22, height, true);
  view.setUint16(26, 1, true);
  view.setUint16(28, 24, true);
  view.setUint32(34, pixelDataSize, true);
  view.setInt32(38, 2835, true);
  view.setInt32(42, 2835, true);

  for (let y = 0; y < height; y += 1) {
    const srcY = height - 1 - y;
    let dstIndex = pixelOffset + y * rowStride;
    for (let x = 0; x < width; x += 1) {
      const srcIndex = (srcY * width + x) * 4;
      bytes[dstIndex] = data[srcIndex + 2];
      bytes[dstIndex + 1] = data[srcIndex + 1];
      bytes[dstIndex + 2] = data[srcIndex];
      dstIndex += 3;
    }
  }

  return bytes;
}

export async function exportCanvasScreenshot(
  path: string,
  format: CanvasScreenshotFormat,
): Promise<string> {
  const source = getScreenshotSource();
  const flattenWhite = format === 'jpg' || format === 'bmp';
  const canvas = composeCanvasLayers(source, { flattenWhite });
  const bytes = format === 'bmp'
    ? encodeBmp24(canvas)
    : await canvasToBytes(canvas, format === 'jpg' ? 'image/jpeg' : 'image/png', format === 'jpg' ? 0.92 : undefined);

  return invoke<string>('write_export_bytes', {
    path,
    bytes: Array.from(bytes),
  });
}
