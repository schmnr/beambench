/**
 * Shared cache for processed-raster preview bitmaps + per-region burned-mask
 * offscreen canvases used by animated playback. Used by both the main
 * CanvasRenderer (static preview on the workspace canvas) and
 * PreviewWindow.tsx (animated preview in a separate dialog). Each rendering
 * surface owns its own instance and clears it whenever `previewData`
 * changes, so there's no cross-plan stale state.
 */

export interface CachedPreviewBitmap {
  /** Blob URL backing the HTMLImageElement; revoked on clear(). */
  blobUrl: string;
  /** HTMLImageElement (possibly still loading). */
  img: HTMLImageElement;
}

export class PreviewBitmapCache {
  private bitmapCache: Map<number, CachedPreviewBitmap> = new Map();
  private burnedMaskCache: Map<number, HTMLCanvasElement> = new Map();
  private tintedBurnedMaskCache: Map<number, HTMLCanvasElement> = new Map();
  private onLoadCallback: (() => void) | null = null;

  /** Register a callback invoked whenever an in-flight bitmap finishes
   *  decoding. Owners should use this to re-trigger render. */
  setOnLoad(cb: (() => void) | null): void {
    this.onLoadCallback = cb;
  }

  /**
   * Ensure an HTMLImageElement is available for this sequence/png pair.
   * - On cache hit: returns the cached element (may still be loading —
   *   in that case its natural size is 0 and caller should skip draw).
   * - On cache miss: creates a blob URL + Image, inserts into the map,
   *   and triggers `onLoadCallback` when decoding completes.
   *
   * Returns null only if the image is still loading AND has no previous
   * cached entry.
   */
  ensurePreviewBitmap(
    sequence: number,
    pngBytes: number[] | Uint8Array,
  ): HTMLImageElement | null {
    const existing = this.bitmapCache.get(sequence);
    if (existing) {
      // If still loading (naturalWidth 0), caller should skip draw;
      // return the element so callers can check .complete.
      return existing.img;
    }

    // Rust Vec<u8> -> number[] across Tauri -> convert to Uint8Array
    // for Blob construction.
    const bytes = pngBytes instanceof Uint8Array ? Uint8Array.from(pngBytes) : new Uint8Array(pngBytes);
    const blob = new Blob([bytes], { type: 'image/png' });
    const blobUrl = URL.createObjectURL(blob);

    const img = new Image();
    const entry: CachedPreviewBitmap = { blobUrl, img };
    img.onload = () => {
      this.onLoadCallback?.();
    };
    img.onerror = () => {
      // Drop failed entries so a later decode attempt can retry.
      this.bitmapCache.delete(sequence);
      URL.revokeObjectURL(blobUrl);
    };
    img.src = blobUrl;
    this.bitmapCache.set(sequence, entry);
    return img;
  }

  /**
   * Ensure a per-region burned-mask offscreen canvas exists. Allocated
   * lazily on first access and reused across animation frames. Sized to
   * match the region's bitmap so per-run source rectangles can be copied
   * verbatim into it.
   */
  ensureBurnedMask(
    sequence: number,
    widthPx: number,
    heightPx: number,
  ): HTMLCanvasElement {
    const existing = this.burnedMaskCache.get(sequence);
    if (existing && existing.width === widthPx && existing.height === heightPx) {
      return existing;
    }
    const canvas = document.createElement('canvas');
    canvas.width = Math.max(1, widthPx);
    canvas.height = Math.max(1, heightPx);
    this.burnedMaskCache.set(sequence, canvas);
    return canvas;
  }

  /** Drop a single burned-mask (used on region completion). */
  clearBurnedMask(sequence: number): void {
    this.burnedMaskCache.delete(sequence);
    this.tintedBurnedMaskCache.delete(sequence);
  }

  ensureTintedBurnedMask(
    sequence: number,
    widthPx: number,
    heightPx: number,
  ): HTMLCanvasElement {
    const existing = this.tintedBurnedMaskCache.get(sequence);
    if (existing && existing.width === widthPx && existing.height === heightPx) {
      return existing;
    }
    const canvas = document.createElement('canvas');
    canvas.width = Math.max(1, widthPx);
    canvas.height = Math.max(1, heightPx);
    this.tintedBurnedMaskCache.set(sequence, canvas);
    return canvas;
  }

  /** Revoke all blob URLs and drop all cached bitmaps + burned masks.
   *  Call on plan rebuild and on renderer/window teardown. */
  clear(): void {
    for (const entry of this.bitmapCache.values()) {
      URL.revokeObjectURL(entry.blobUrl);
    }
    this.bitmapCache.clear();
    this.burnedMaskCache.clear();
    this.tintedBurnedMaskCache.clear();
  }
}
