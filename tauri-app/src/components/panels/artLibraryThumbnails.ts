import { drawObject } from '../../canvas/drawObjects';
import { zoomToFitBounds, type ViewportParams } from '../../canvas/ViewportTransform';
import type {
  ArtLibraryItem,
  ArtLibrarySelectionSnapshot,
} from '../../types/artLibrary';
import type { Bounds, Layer, ProjectObject } from '../../types/project';

const THUMBNAIL_SIZE = 128;
const THUMBNAIL_PADDING = 12;

function base64ToBytes(data: string): Uint8Array {
  const binary = globalThis.atob(data);
  const out = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) {
    out[i] = binary.charCodeAt(i);
  }
  return out;
}

function bytesToBlobPart(bytes: Uint8Array): ArrayBuffer {
  const copy = new Uint8Array(bytes.byteLength);
  copy.set(bytes);
  return copy.buffer;
}

async function loadImageFromBlob(blob: Blob): Promise<HTMLImageElement | null> {
  if (typeof Image === 'undefined') return null;
  return await new Promise((resolve) => {
    const url = URL.createObjectURL(blob);
    const img = new Image();
    img.onload = () => {
      URL.revokeObjectURL(url);
      resolve(img);
    };
    img.onerror = () => {
      URL.revokeObjectURL(url);
      resolve(null);
    };
    img.src = url;
  });
}

function createCanvas(): HTMLCanvasElement | null {
  if (typeof document === 'undefined') return null;
  const canvas = document.createElement('canvas');
  canvas.width = THUMBNAIL_SIZE;
  canvas.height = THUMBNAIL_SIZE;
  return canvas;
}

function unionBounds(objects: ProjectObject[]): Bounds | null {
  if (objects.length === 0) return null;
  let minX = objects[0].bounds.min.x;
  let minY = objects[0].bounds.min.y;
  let maxX = objects[0].bounds.max.x;
  let maxY = objects[0].bounds.max.y;
  for (const object of objects.slice(1)) {
    minX = Math.min(minX, object.bounds.min.x);
    minY = Math.min(minY, object.bounds.min.y);
    maxX = Math.max(maxX, object.bounds.max.x);
    maxY = Math.max(maxY, object.bounds.max.y);
  }
  return { min: { x: minX, y: minY }, max: { x: maxX, y: maxY } };
}

async function renderExternalFileThumbnail(item: ArtLibraryItem): Promise<string | null> {
  const bytes = base64ToBytes(item.data);
  const mediaType = item.media_type || 'application/octet-stream';
  if (!mediaType.startsWith('image/') && mediaType !== 'image/svg+xml') {
    return null;
  }

  const canvas = createCanvas();
  const ctx = canvas?.getContext('2d');
  if (!canvas || !ctx) return null;
  ctx.clearRect(0, 0, THUMBNAIL_SIZE, THUMBNAIL_SIZE);
  const img = await loadImageFromBlob(new Blob([bytesToBlobPart(bytes)], { type: mediaType }));
  if (!img) return null;

  const scale = Math.min(
    (THUMBNAIL_SIZE - THUMBNAIL_PADDING * 2) / img.width,
    (THUMBNAIL_SIZE - THUMBNAIL_PADDING * 2) / img.height,
  );
  const width = Math.max(1, img.width * scale);
  const height = Math.max(1, img.height * scale);
  const x = (THUMBNAIL_SIZE - width) / 2;
  const y = (THUMBNAIL_SIZE - height) / 2;
  ctx.drawImage(img, x, y, width, height);
  return canvas.toDataURL('image/png').replace(/^data:image\/png;base64,/, '');
}

async function buildSnapshotImageCache(
  snapshot: ArtLibrarySelectionSnapshot,
): Promise<Map<string, HTMLImageElement | HTMLCanvasElement>> {
  const cache = new Map<string, HTMLImageElement | HTMLCanvasElement>();
  for (const asset of snapshot.assets) {
    if (!asset.media_type.startsWith('image/')) continue;
    const img = await loadImageFromBlob(new Blob([bytesToBlobPart(base64ToBytes(asset.data))], { type: asset.media_type }));
    if (img) {
      cache.set(asset.hash, img);
    }
  }
  return cache;
}

function resolveLayer(object: ProjectObject, layers: Layer[]): Layer {
  return (
    layers.find((layer) => layer.id === object.layer_id) ?? {
      id: object.layer_id,
      name: 'Line',
      entries: [],
      enabled: true,
      order_index: 0,
      color_tag: '#ffffff',
      visible: true,
      is_tool_layer: false,
    }
  );
}

async function renderSnapshotThumbnail(item: ArtLibraryItem): Promise<string | null> {
  const canvas = createCanvas();
  const ctx = canvas?.getContext('2d');
  if (!canvas || !ctx) return null;

  let snapshot: ArtLibrarySelectionSnapshot;
  try {
    snapshot = JSON.parse(new TextDecoder().decode(base64ToBytes(item.data))) as ArtLibrarySelectionSnapshot;
  } catch {
    return null;
  }
  const bounds = unionBounds(snapshot.objects);
  if (!bounds) return null;
  const imageCache = await buildSnapshotImageCache(snapshot);
  const vpBase = zoomToFitBounds(bounds, THUMBNAIL_SIZE, THUMBNAIL_SIZE, THUMBNAIL_PADDING);
  const vp: ViewportParams = {
    offset: vpBase.offset,
    zoom: vpBase.zoom,
    canvasWidth: THUMBNAIL_SIZE,
    canvasHeight: THUMBNAIL_SIZE,
  };
  ctx.clearRect(0, 0, THUMBNAIL_SIZE, THUMBNAIL_SIZE);
  for (const object of snapshot.objects) {
    drawObject(
      ctx,
      object,
      resolveLayer(object, snapshot.layer_templates),
      vp,
      imageCache,
      undefined,
      true,
      false,
    );
  }
  return canvas.toDataURL('image/png').replace(/^data:image\/png;base64,/, '');
}

export async function generateArtLibraryThumbnail(item: ArtLibraryItem): Promise<string | null> {
  if (item.kind === 'selection_snapshot') {
    return renderSnapshotThumbnail(item);
  }
  return renderExternalFileThumbnail(item);
}
