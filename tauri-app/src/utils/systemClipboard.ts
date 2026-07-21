import { invoke } from '@tauri-apps/api/core';
import type { Point2D } from '../types/project';
import { useProjectStore } from '../stores/projectStore';
import { useUiStore } from '../stores/uiStore';

const SUPPORTED_ARTWORK_MIME_TYPES = new Set([
  'image/png',
  'image/jpeg',
  'image/jpg',
  'image/gif',
  'image/bmp',
  'image/webp',
  'image/tiff',
  'image/tif',
  'image/tga',
  'image/x-tga',
  'image/svg+xml',
]);

interface ClipboardArtworkBlob {
  blob: Blob;
  filename: string;
  mediaType: string;
}

interface ClipboardArtworkPayload {
  dataBase64: string;
  filename: string;
  mediaType: string;
}

export function isSupportedClipboardArtworkType(type: string | undefined | null): boolean {
  return Boolean(type && SUPPORTED_ARTWORK_MIME_TYPES.has(type.toLowerCase()));
}

function clipboardExtensionForType(type: string): string {
  switch (type.toLowerCase()) {
    case 'image/jpeg':
    case 'image/jpg':
      return 'jpg';
    case 'image/gif':
      return 'gif';
    case 'image/bmp':
      return 'bmp';
    case 'image/webp':
      return 'webp';
    case 'image/tiff':
    case 'image/tif':
      return 'tiff';
    case 'image/tga':
    case 'image/x-tga':
      return 'tga';
    case 'image/svg+xml':
      return 'svg';
    default:
      return 'png';
  }
}

function defaultClipboardFilename(mediaType: string, index: number): string {
  const suffix = index > 0 ? ` ${index + 1}` : '';
  return `Clipboard Artwork${suffix}.${clipboardExtensionForType(mediaType)}`;
}

function mediaTypeFromFilename(filename: string): string | null {
  const ext = filename.split('.').pop()?.toLowerCase();
  switch (ext) {
    case 'png':
      return 'image/png';
    case 'jpg':
    case 'jpeg':
      return 'image/jpeg';
    case 'gif':
      return 'image/gif';
    case 'bmp':
      return 'image/bmp';
    case 'webp':
      return 'image/webp';
    case 'tif':
    case 'tiff':
      return 'image/tiff';
    case 'tga':
      return 'image/x-tga';
    case 'svg':
      return 'image/svg+xml';
    default:
      return null;
  }
}

function looksLikeSvg(text: string): boolean {
  const trimmed = text.trimStart();
  return trimmed.startsWith('<svg') || (trimmed.startsWith('<?xml') && trimmed.includes('<svg'));
}

function fileToArtworkBlob(file: File, index: number): ClipboardArtworkBlob | null {
  const mediaType = file.type || mediaTypeFromFilename(file.name);
  if (!mediaType || !isSupportedClipboardArtworkType(mediaType)) return null;
  return {
    blob: file,
    filename: file.name || defaultClipboardFilename(mediaType, index),
    mediaType,
  };
}

export function getClipboardArtworkBlobs(event: ClipboardEvent): ClipboardArtworkBlob[] {
  const data = event.clipboardData;
  if (!data) return [];

  const blobs: ClipboardArtworkBlob[] = [];
  const items = Array.from(data.items ?? []);
  for (const item of items) {
    if (item.kind !== 'file' || !isSupportedClipboardArtworkType(item.type)) continue;
    const file = item.getAsFile();
    if (!file) continue;
    const artwork = fileToArtworkBlob(file, blobs.length);
    if (artwork) blobs.push(artwork);
  }

  if (blobs.length === 0) {
    const files = Array.from(data.files ?? []);
    for (const file of files) {
      const artwork = fileToArtworkBlob(file, blobs.length);
      if (artwork) blobs.push(artwork);
    }
  }

  if (blobs.length === 0) {
    const svgText = data.getData('image/svg+xml') || data.getData('text/plain');
    if (svgText && looksLikeSvg(svgText)) {
      blobs.push({
        blob: new Blob([svgText], { type: 'image/svg+xml' }),
        filename: defaultClipboardFilename('image/svg+xml', 0),
        mediaType: 'image/svg+xml',
      });
    }
  }

  return blobs;
}

export async function getNavigatorClipboardArtworkBlobs(): Promise<ClipboardArtworkBlob[]> {
  if (!navigator.clipboard || typeof navigator.clipboard.read !== 'function') {
    return [];
  }

  const clipboardItems = await navigator.clipboard.read();
  const blobs: ClipboardArtworkBlob[] = [];
  for (const item of clipboardItems) {
    const mediaType = item.types.find((type) => isSupportedClipboardArtworkType(type));
    if (!mediaType) continue;
    const blob = await item.getType(mediaType);
    blobs.push({
      blob,
      filename: defaultClipboardFilename(mediaType, blobs.length),
      mediaType,
    });
  }
  return blobs;
}

export function getSystemPasteWorldPosition(): Point2D {
  const ui = useUiStore.getState();
  return ui.cursorWorldPos ?? ui.viewportOffset;
}

export async function blobToBase64(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(reader.error ?? new Error('Failed to read clipboard artwork'));
    reader.onload = () => {
      const result = reader.result;
      if (typeof result !== 'string') {
        reject(new Error('Failed to read clipboard artwork'));
        return;
      }
      const comma = result.indexOf(',');
      resolve(comma >= 0 ? result.slice(comma + 1) : result);
    };
    reader.readAsDataURL(blob);
  });
}

export async function pasteClipboardArtworkBlobs(
  blobs: ClipboardArtworkBlob[],
  drop: Point2D | null = getSystemPasteWorldPosition(),
): Promise<boolean> {
  if (blobs.length === 0) return false;

  for (let index = 0; index < blobs.length; index += 1) {
    const item = blobs[index];
    const dataBase64 = await blobToBase64(item.blob);
    const itemDrop = drop ? { x: drop.x + index * 5, y: drop.y + index * 5 } : null;
    await useProjectStore.getState().importClipboardArtwork({
      dataBase64,
      filename: item.filename,
      mediaType: item.mediaType,
    }, itemDrop);
  }
  return true;
}

export async function pasteClipboardArtworkPayload(
  artwork: ClipboardArtworkPayload | null | undefined,
  drop: Point2D | null = getSystemPasteWorldPosition(),
): Promise<boolean> {
  if (!artwork) return false;
  await useProjectStore.getState().importClipboardArtwork(artwork, drop);
  return true;
}

export async function pasteClipboardArtworkFromEvent(event: ClipboardEvent): Promise<boolean> {
  const blobs = getClipboardArtworkBlobs(event);
  if (blobs.length === 0) return false;
  event.preventDefault();
  return pasteClipboardArtworkBlobs(blobs);
}

export async function pasteClipboardArtworkFromNative(): Promise<boolean> {
  let artwork: ClipboardArtworkPayload | null;
  try {
    artwork = await invoke<ClipboardArtworkPayload | null>('read_clipboard_artwork');
  } catch {
    return false;
  }
  return pasteClipboardArtworkPayload(artwork);
}

export async function pasteClipboardArtworkFromNavigator(): Promise<boolean> {
  let blobs: ClipboardArtworkBlob[];
  try {
    blobs = await getNavigatorClipboardArtworkBlobs();
  } catch {
    return false;
  }
  return pasteClipboardArtworkBlobs(blobs);
}

export async function pasteClipboardArtworkFromSystem(): Promise<boolean> {
  return (await pasteClipboardArtworkFromNative()) || (await pasteClipboardArtworkFromNavigator());
}
