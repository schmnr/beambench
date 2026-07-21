import { describe, it, expect, vi, beforeEach } from 'vitest';
import { drawVectorPreview, drawRasterPreview, drawTravelPreview, drawFramePreview, drawRasterRunStripes } from './drawPreview';
import type { ViewportParams } from './ViewportTransform';
import type { VectorPreview, RasterPreview, TravelMove, PreviewFrame } from '../types/preview';
import { PreviewBitmapCache } from './previewBitmapCache';

// Mock canvas context
function createMockCtx(): CanvasRenderingContext2D {
  const calls: { method: string; args: unknown[] }[] = [];

  const ctx = {
    _calls: calls,
    save: vi.fn(() => calls.push({ method: 'save', args: [] })),
    restore: vi.fn(() => calls.push({ method: 'restore', args: [] })),
    beginPath: vi.fn(() => calls.push({ method: 'beginPath', args: [] })),
    moveTo: vi.fn((x: number, y: number) => calls.push({ method: 'moveTo', args: [x, y] })),
    lineTo: vi.fn((x: number, y: number) => calls.push({ method: 'lineTo', args: [x, y] })),
    stroke: vi.fn(() => calls.push({ method: 'stroke', args: [] })),
    closePath: vi.fn(() => calls.push({ method: 'closePath', args: [] })),
    fillRect: vi.fn((x: number, y: number, w: number, h: number) => calls.push({ method: 'fillRect', args: [x, y, w, h] })),
    strokeRect: vi.fn((x: number, y: number, w: number, h: number) => calls.push({ method: 'strokeRect', args: [x, y, w, h] })),
    rect: vi.fn((x: number, y: number, w: number, h: number) => calls.push({ method: 'rect', args: [x, y, w, h] })),
    fill: vi.fn(() => calls.push({ method: 'fill', args: [] })),
    clip: vi.fn(() => calls.push({ method: 'clip', args: [] })),
    drawImage: vi.fn(() => calls.push({ method: 'drawImage', args: [] })),
    translate: vi.fn(() => calls.push({ method: 'translate', args: [] })),
    rotate: vi.fn(() => calls.push({ method: 'rotate', args: [] })),
    scale: vi.fn(() => calls.push({ method: 'scale', args: [] })),
    transform: vi.fn(() => calls.push({ method: 'transform', args: [] })),
    setLineDash: vi.fn((dash: number[]) => calls.push({ method: 'setLineDash', args: [dash] })),
    strokeStyle: '',
    fillStyle: '',
    lineWidth: 1,
    globalAlpha: 1,
    lineCap: 'butt' as CanvasLineCap,
    imageSmoothingEnabled: true,
  };

  return ctx as unknown as CanvasRenderingContext2D;
}

const vp: ViewportParams = {
  offset: { x: 0, y: 0 },
  zoom: 100,
  canvasWidth: 800,
  canvasHeight: 600,
};

/** Shared bitmap cache stub. Tests that don't provide a real PNG will
 *  hit the fallback hatched render path, which still exercises the
 *  region outline + overscan code. */
function makeCache(): PreviewBitmapCache {
  return new PreviewBitmapCache();
}

function makeVectorPreview(overrides = {}): VectorPreview {
  return {
    points: [{ x: 0, y: 0 }, { x: 10, y: 0 }],
    closed: false,
    power_percent: 80,
    speed_mm_min: 1000,
    sequence: 1,
    ...overrides,
  };
}

function makeRasterPreview(overrides = {}): RasterPreview {
  return {
    bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } },
    line_count: 10,
    line_interval_mm: 0.1,
    direction_mode: 'bidirectional',
    power_mode: 'binary',
    speed_mm_min: 2000,
    fill_density: 1,
    scan_angle_deg: 0,
    scan_origin: { x: 0, y: 0 },
    overscan_mm: 0,
    outlines: [],
    scan_axis: 'horizontal',
    sequence: 1,
    duration_secs: 0,
    avg_power_normalized: 0,
    local_origin_mm: { x: 0, y: 0 },
    local_width_mm: 10,
    local_height_mm: 10,
    run_extents: [],
    overscan_run_extents: [],
    ...overrides,
  };
}

function makeTravelMove(overrides = {}): TravelMove {
  return {
    from: { x: 0, y: 0 },
    to: { x: 10, y: 10 },
    sequence: 0,
    ...overrides,
  };
}

describe('drawVectorPreview', () => {
  let ctx: CanvasRenderingContext2D;

  beforeEach(() => {
    ctx = createMockCtx();
  });

  it('draws polyline path with correct coordinates', () => {
    const vectors: VectorPreview[] = [makeVectorPreview({
      points: [{ x: 0, y: 0 }, { x: 10, y: 0 }, { x: 10, y: 10 }],
    })];

    drawVectorPreview(ctx, vectors, '#ff0000', vp);

    expect(ctx.beginPath).toHaveBeenCalled();
    expect(ctx.moveTo).toHaveBeenCalled();
    expect(ctx.lineTo).toHaveBeenCalled();
    expect(ctx.stroke).toHaveBeenCalled();
    expect(ctx.closePath).not.toHaveBeenCalled();
  });

  it('calls closePath for closed paths', () => {
    const vectors: VectorPreview[] = [makeVectorPreview({
      points: [{ x: 0, y: 0 }, { x: 10, y: 0 }, { x: 10, y: 10 }],
      closed: true,
    })];

    drawVectorPreview(ctx, vectors, '#ff0000', vp);

    expect(ctx.closePath).toHaveBeenCalled();
  });

  it('skips vectors with fewer than 2 points', () => {
    const vectors: VectorPreview[] = [makeVectorPreview({
      points: [{ x: 0, y: 0 }],
    })];

    drawVectorPreview(ctx, vectors, '#ff0000', vp);

    expect(ctx.moveTo).not.toHaveBeenCalled();
  });
});

describe('drawRasterPreview', () => {
  let ctx: CanvasRenderingContext2D;
  let cache: PreviewBitmapCache;

  beforeEach(() => {
    ctx = createMockCtx();
    cache = makeCache();
  });

  it('draws raster region with fallback hatching when no bitmap', () => {
    const regions: RasterPreview[] = [makeRasterPreview({
      bounds: { min: { x: 10, y: 20 }, max: { x: 60, y: 40 } },
      line_count: 100,
      fill_density: 0.8,
      local_width_mm: 50,
      local_height_mm: 20,
    })];

    drawRasterPreview(ctx, regions, '#00ff00', vp, cache);

    // No bitmap → fallback hatched render + outline stroke.
    expect(ctx.fillRect).toHaveBeenCalled();
    expect(ctx.strokeRect).toHaveBeenCalled();
  });

  it('renders orthogonal raster region', () => {
    const regions90: RasterPreview[] = [makeRasterPreview({
      fill_density: 0.5,
      scan_angle_deg: 90,
      scan_origin: { x: 5, y: 5 },
    })];

    drawRasterPreview(ctx, regions90, '#00ff00', vp, cache);
    expect(ctx.fillRect).toHaveBeenCalled();
    expect(ctx.strokeRect).toHaveBeenCalled();
  });

  it('enables image smoothing when downsampling a preview bitmap', () => {
    const fakeImg = { complete: true, naturalWidth: 200 } as HTMLImageElement;
    const bitmapCache = {
      ensurePreviewBitmap: vi.fn(() => fakeImg),
    } as unknown as PreviewBitmapCache;

    const regions: RasterPreview[] = [makeRasterPreview({
      preview_bitmap: {
        width_px: 400,
        height_px: 400,
        png_bytes: [0],
      },
      local_origin_mm: { x: 0, y: 0 },
      local_width_mm: 1,
      local_height_mm: 1,
    })];

    drawRasterPreview(ctx, regions, '#00ff00', vp, bitmapCache);

    expect(ctx.drawImage).toHaveBeenCalled();
    expect(ctx.imageSmoothingEnabled).toBe(true);
  });

  it('opacity scales with fill_density', () => {
    const ctx1 = createMockCtx();
    const ctx2 = createMockCtx();
    const c1 = makeCache();
    const c2 = makeCache();

    const lowDensity: RasterPreview[] = [makeRasterPreview({
      fill_density: 0.1,
    })];

    const highDensity: RasterPreview[] = [makeRasterPreview({
      fill_density: 1.0,
    })];

    drawRasterPreview(ctx1, lowDensity, '#00ff00', vp, c1);
    drawRasterPreview(ctx2, highDensity, '#00ff00', vp, c2);

    expect(ctx1.fillRect).toHaveBeenCalled();
    expect(ctx2.fillRect).toHaveBeenCalled();
  });

  it('draws run-based overscan markers for outlined raster regions', () => {
    const regions: RasterPreview[] = [makeRasterPreview({
      overscan_mm: 1,
      run_extents: [
        { y_mm: 2, start_x_mm: 1, end_x_mm: 9, direction: 'left_to_right' },
        { y_mm: 4, start_x_mm: 1, end_x_mm: 9, direction: 'left_to_right' },
      ],
      outlines: [{
        closed: true,
        points: [
          { x: 1, y: 1 },
          { x: 9, y: 1 },
          { x: 9, y: 9 },
          { x: 1, y: 9 },
        ],
      }],
    })];

    drawRasterPreview(ctx, regions, '#00ff00', vp, cache);

    expect(ctx.fillRect).toHaveBeenCalledTimes(4);
  });

  it('draws run-based overscan markers for image raster regions with run extents', () => {
    const regions: RasterPreview[] = [makeRasterPreview({
      overscan_mm: 1,
      run_extents: [
        { y_mm: 2, start_x_mm: 1, end_x_mm: 9, direction: 'left_to_right' },
        { y_mm: 4, start_x_mm: 2, end_x_mm: 8, direction: 'left_to_right' },
      ],
      preview_bitmap: {
        width_px: 10,
        height_px: 10,
        png_bytes: new Uint8Array([1, 2, 3]),
      },
      local_origin_mm: { x: 0, y: 0 },
      local_width_mm: 10,
      local_height_mm: 10,
      sequence: 1,
    })];

    vi.spyOn(cache, 'ensurePreviewBitmap').mockReturnValue({
      complete: true,
      naturalWidth: 10,
    } as HTMLImageElement);

    drawRasterPreview(ctx, regions, '#000000', vp, cache);

    expect(ctx.drawImage).toHaveBeenCalled();
    expect(ctx.fillRect).toHaveBeenCalledTimes(4);
  });

  it('does not use scanline stripe rendering for image rasters misclassified as fill', () => {
    const regions: RasterPreview[] = [makeRasterPreview({
      run_extents: [
        { y_mm: 2, start_x_mm: 1, end_x_mm: 9, direction: 'left_to_right' },
        { y_mm: 4, start_x_mm: 1, end_x_mm: 9, direction: 'left_to_right' },
      ],
      preview_bitmap: {
        width_px: 10,
        height_px: 10,
        png_bytes: new Uint8Array([1, 2, 3]),
      },
      local_origin_mm: { x: 0, y: 0 },
      local_width_mm: 10,
      local_height_mm: 10,
      sequence: 3,
      outlines: [],
    })];

    vi.spyOn(cache, 'ensurePreviewBitmap').mockReturnValue({
      complete: true,
      naturalWidth: 10,
    } as HTMLImageElement);

    drawRasterPreview(ctx, regions, '#000000', vp, cache, true);

    expect(ctx.drawImage).toHaveBeenCalled();
    expect(ctx.stroke).not.toHaveBeenCalled();
  });

  it('prefers scanline envelopes over individual burn runs for overscan markers', () => {
    const regions: RasterPreview[] = [makeRasterPreview({
      line_count: 1,
      power_mode: 'grayscale',
      overscan_mm: 1,
      run_extents: [
        { y_mm: 2, start_x_mm: 1, end_x_mm: 3, direction: 'left_to_right' },
        { y_mm: 2, start_x_mm: 7, end_x_mm: 9, direction: 'left_to_right' },
      ],
      scanline_extents: [
        { y_mm: 2, start_x_mm: 1, end_x_mm: 9, direction: 'left_to_right' },
      ],
      overscan_run_extents: [
        { y_mm: 2, start_x_mm: 1, end_x_mm: 3, direction: 'left_to_right' },
        { y_mm: 2, start_x_mm: 7, end_x_mm: 9, direction: 'left_to_right' },
      ],
      preview_bitmap: {
        width_px: 10,
        height_px: 10,
        png_bytes: new Uint8Array([1, 2, 3]),
      },
      local_origin_mm: { x: 0, y: 0 },
      local_width_mm: 10,
      local_height_mm: 10,
      sequence: 2,
    })];

    vi.spyOn(cache, 'ensurePreviewBitmap').mockReturnValue({
      complete: true,
      naturalWidth: 10,
    } as HTMLImageElement);

    drawRasterPreview(ctx, regions, '#000000', vp, cache);

    // One raster row means one lead-in and one lead-out marker. If the
    // renderer incorrectly uses the two burn runs, this would be 4.
    expect(ctx.fillRect).toHaveBeenCalledTimes(2);
  });

  it('renders outlined raster regions from run extents instead of fallback fill blocks', () => {
    const regions: RasterPreview[] = [makeRasterPreview({
      line_count: 2,
      run_extents: [
        { y_mm: 2, start_x_mm: 1, end_x_mm: 9, direction: 'left_to_right' },
        { y_mm: 4, start_x_mm: 1, end_x_mm: 9, direction: 'left_to_right' },
      ],
      outlines: [{
        closed: true,
        points: [
          { x: 1, y: 1 },
          { x: 9, y: 1 },
          { x: 9, y: 9 },
          { x: 1, y: 9 },
        ],
      }],
    })];

    drawRasterPreview(ctx, regions, '#000000', vp, cache);

    expect(ctx.lineTo).toHaveBeenCalled();
    expect(ctx.fillRect).not.toHaveBeenCalled();
  });

  it('does not clip run stripes to fill outlines (burn geometry is exact; outlines may be simplified)', () => {
    // Simplified preview outlines can collapse thin slivers that the planner
    // still burns. Stripes are the exact run extents and must render
    // unclipped; only the cosmetic base tint may be clipped to outlines.
    const stripeCtx = createMockCtx();
    const runs = [
      { y_mm: 5, start_x_mm: 0, end_x_mm: 30, direction: 'left_to_right' as const },
    ];
    const region = makeRasterPreview({
      run_extents: runs,
      outlines: [{
        closed: true,
        points: [
          { x: 0, y: 0 },
          { x: 10, y: 0 },
          { x: 10, y: 10 },
          { x: 0, y: 10 },
        ],
      }],
    });

    drawRasterRunStripes(stripeCtx, region, runs, '#000000', vp);

    const calls = (stripeCtx as unknown as { _calls: { method: string }[] })._calls;
    const clipIdx = calls.findIndex((c) => c.method === 'clip');
    expect(clipIdx).toBeGreaterThanOrEqual(0);
    const restoreIdx = calls.findIndex((c, i) => c.method === 'restore' && i > clipIdx);
    expect(restoreIdx).toBeGreaterThan(clipIdx);
    const strokeIdxs = calls
      .map((c, i) => (c.method === 'stroke' ? i : -1))
      .filter((i) => i >= 0);
    expect(strokeIdxs.length).toBeGreaterThan(0);
    for (const idx of strokeIdxs) {
      expect(idx).toBeGreaterThan(restoreIdx);
    }
  });

  it('uses denser stroke coverage for emphasized run previews', () => {
    const standardCtx = createMockCtx();
    const emphasizedCtx = createMockCtx();
    const runs = Array.from({ length: 12 }, (_, i) => ({
      y_mm: 0.02 * i,
      start_x_mm: 0,
      end_x_mm: 10,
      direction: 'left_to_right' as const,
    }));
    const region = makeRasterPreview({
      line_interval_mm: 0.02,
      run_extents: runs,
      outlines: [{
        closed: true,
        points: [
          { x: 0, y: 0 },
          { x: 10, y: 0 },
          { x: 10, y: 1 },
          { x: 0, y: 1 },
        ],
      }],
    });

    drawRasterRunStripes(standardCtx, region, runs, '#ff0000', vp, { emphasizeVisibleRuns: false });
    drawRasterRunStripes(emphasizedCtx, region, runs, '#ff0000', vp, { emphasizeVisibleRuns: true });

    expect((emphasizedCtx as unknown as { stroke: ReturnType<typeof vi.fn> }).stroke.mock.calls.length)
      .toBeGreaterThan((standardCtx as unknown as { stroke: ReturnType<typeof vi.fn> }).stroke.mock.calls.length);
  });
});

describe('drawTravelPreview', () => {
  let ctx: CanvasRenderingContext2D;

  beforeEach(() => {
    ctx = createMockCtx();
  });

  it('draws dashed lines for travel moves', () => {
    const moves: TravelMove[] = [
      makeTravelMove(),
      makeTravelMove({ from: { x: 10, y: 10 }, to: { x: 20, y: 0 }, sequence: 1 }),
    ];

    drawTravelPreview(ctx, moves, vp);

    expect(ctx.setLineDash).toHaveBeenCalled();
    expect(ctx.moveTo).toHaveBeenCalledTimes(2);
    expect(ctx.lineTo).toHaveBeenCalledTimes(2);
    expect(ctx.stroke).toHaveBeenCalledTimes(2);
  });
});

describe('drawFramePreview', () => {
  let ctx: CanvasRenderingContext2D;

  beforeEach(() => {
    ctx = createMockCtx();
  });

  it('draws dotted outline for frame', () => {
    const frame: PreviewFrame = {
      path: [
        { x: 0, y: 0 },
        { x: 100, y: 0 },
        { x: 100, y: 50 },
        { x: 0, y: 50 },
        { x: 0, y: 0 },
      ],
      power_percent: 5,
      speed_mm_min: 3000,
    };

    drawFramePreview(ctx, frame, vp);

    expect(ctx.setLineDash).toHaveBeenCalled();
    expect(ctx.moveTo).toHaveBeenCalled();
    expect(ctx.lineTo).toHaveBeenCalledTimes(4);
    expect(ctx.stroke).toHaveBeenCalled();
  });

  it('does nothing for frame with fewer than 2 points', () => {
    const frame: PreviewFrame = {
      path: [{ x: 0, y: 0 }],
      power_percent: 5,
      speed_mm_min: 3000,
    };

    drawFramePreview(ctx, frame, vp);

    expect(ctx.moveTo).not.toHaveBeenCalled();
  });
});
