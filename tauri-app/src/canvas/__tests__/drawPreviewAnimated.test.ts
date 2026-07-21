import { describe, it, expect, vi } from 'vitest';
import { drawAnimatedPreview, interpolatePolyline, interpolateRasterRuns, computeRasterHeadPos } from '../drawPreviewAnimated';
import type { Point2D } from '../../types/project';
import type { RasterRunExtent } from '../../types/preview';
import type { AnimationTimeline } from '../previewTimeline';
import type { PreviewBitmapCache } from '../previewBitmapCache';

describe('interpolatePolyline', () => {
  it('returns first point at fraction 0', () => {
    const pts: Point2D[] = [{ x: 0, y: 0 }, { x: 10, y: 0 }];
    const result = interpolatePolyline(pts, 0);
    expect(result.partialPoints).toHaveLength(1);
    expect(result.headPos).toEqual({ x: 0, y: 0 });
  });

  it('returns all points at fraction 1', () => {
    const pts: Point2D[] = [{ x: 0, y: 0 }, { x: 10, y: 0 }, { x: 20, y: 0 }];
    const result = interpolatePolyline(pts, 1);
    expect(result.partialPoints).toHaveLength(3);
    expect(result.headPos).toEqual({ x: 20, y: 0 });
  });

  it('returns midpoint at fraction 0.5 of a 2-point line', () => {
    const pts: Point2D[] = [{ x: 0, y: 0 }, { x: 10, y: 0 }];
    const result = interpolatePolyline(pts, 0.5);
    expect(result.partialPoints).toHaveLength(2);
    expect(result.headPos.x).toBeCloseTo(5, 5);
    expect(result.headPos.y).toBeCloseTo(0, 5);
  });

  it('handles fraction > 1 as complete', () => {
    const pts: Point2D[] = [{ x: 0, y: 0 }, { x: 10, y: 0 }];
    const result = interpolatePolyline(pts, 1.5);
    expect(result.partialPoints).toHaveLength(2);
    expect(result.headPos).toEqual({ x: 10, y: 0 });
  });

  it('handles fraction < 0 as start', () => {
    const pts: Point2D[] = [{ x: 5, y: 5 }, { x: 15, y: 5 }];
    const result = interpolatePolyline(pts, -0.5);
    expect(result.partialPoints).toHaveLength(1);
    expect(result.headPos).toEqual({ x: 5, y: 5 });
  });

  it('handles empty points array', () => {
    const result = interpolatePolyline([], 0.5);
    expect(result.partialPoints).toHaveLength(0);
    expect(result.headPos).toEqual({ x: 0, y: 0 });
  });

  it('handles single point', () => {
    const result = interpolatePolyline([{ x: 3, y: 4 }], 0.5);
    expect(result.partialPoints).toHaveLength(1);
    expect(result.headPos).toEqual({ x: 3, y: 4 });
  });

  it('interpolates correctly across multi-segment polyline', () => {
    // Three points: (0,0) -> (10,0) -> (10,10). Total length = 20
    const pts: Point2D[] = [{ x: 0, y: 0 }, { x: 10, y: 0 }, { x: 10, y: 10 }];

    // 25% = 5mm along first segment
    const r25 = interpolatePolyline(pts, 0.25);
    expect(r25.headPos.x).toBeCloseTo(5, 5);
    expect(r25.headPos.y).toBeCloseTo(0, 5);
    expect(r25.partialPoints).toHaveLength(2); // start + cutoff

    // 75% = 15mm -> past first segment (10mm), 5mm into second
    const r75 = interpolatePolyline(pts, 0.75);
    expect(r75.headPos.x).toBeCloseTo(10, 5);
    expect(r75.headPos.y).toBeCloseTo(5, 5);
    expect(r75.partialPoints).toHaveLength(3); // start + corner + cutoff
  });

  it('handles zero-length segments', () => {
    const pts: Point2D[] = [{ x: 5, y: 5 }, { x: 5, y: 5 }];
    const result = interpolatePolyline(pts, 0.5);
    // Zero-length line: fraction of 0 total = start
    expect(result.partialPoints.length).toBeGreaterThanOrEqual(1);
  });
});

function makeMockContext() {
  return {
    save: vi.fn(),
    restore: vi.fn(),
    beginPath: vi.fn(),
    moveTo: vi.fn(),
    lineTo: vi.fn(),
    closePath: vi.fn(),
    clip: vi.fn(),
    stroke: vi.fn(),
    fill: vi.fn(),
    arc: vi.fn(),
    translate: vi.fn(),
    rotate: vi.fn(),
    scale: vi.fn(),
    transform: vi.fn(),
    drawImage: vi.fn(),
    strokeRect: vi.fn(),
    fillRect: vi.fn(),
    clearRect: vi.fn(),
    setLineDash: vi.fn(),
    globalAlpha: 1,
    strokeStyle: '',
    fillStyle: '',
    lineWidth: 1,
    lineCap: 'butt',
    imageSmoothingEnabled: false,
  } as unknown as CanvasRenderingContext2D;
}

// --- interpolateRasterRuns tests ---

function makeRun(y: number, startX: number, endX: number, _power = 0.5): RasterRunExtent {
  return { y_mm: y, start_x_mm: startX, end_x_mm: endX, direction: 'left_to_right' };
}

describe('interpolateRasterRuns', () => {
  it('returns empty visibleRuns for empty array', () => {
    const result = interpolateRasterRuns([], 0.5);
    expect(result.visibleRuns).toHaveLength(0);
    expect(result.headPos).toEqual({ x: 0, y: 0 });
  });

  it('returns empty visibleRuns at progress 0', () => {
    const runs = [makeRun(5, 0, 10)];
    const result = interpolateRasterRuns(runs, 0);
    expect(result.visibleRuns).toHaveLength(0);
    expect(result.headPos).toEqual({ x: 0, y: 0 });
  });

  it('returns all runs at progress 1 with head at last strip end', () => {
    const runs = [makeRun(5, 0, 10), makeRun(6, 0, 10)];
    const result = interpolateRasterRuns(runs, 1);
    expect(result.visibleRuns).toHaveLength(2);
    // Head at end of last strip: x=end_x_mm=10, y=y_mm=6
    expect(result.headPos).toEqual({ x: 10, y: 6 });
  });

  it('single strip: partial at fractional progress with head at cutoff', () => {
    const runs = [makeRun(5, 0, 20)];
    const result = interpolateRasterRuns(runs, 0.5);
    expect(result.visibleRuns).toHaveLength(1);
    expect(result.visibleRuns[0].end_x_mm).toBeCloseTo(10, 5);
    // Head tracks cutoff: x=10, y=5
    expect(result.headPos.x).toBeCloseTo(10, 5);
    expect(result.headPos.y).toBe(5);
  });

  it('equal-length runs: progress 0.5 is at midpoint of total distance', () => {
    // Two 10mm runs: total = 20mm. At 50% = 10mm, we finish strip 0.
    const runs = [makeRun(5, 0, 10), makeRun(6, 0, 10)];
    const result = interpolateRasterRuns(runs, 0.5);
    expect(result.visibleRuns).toHaveLength(1);
    expect(result.visibleRuns[0].end_x_mm).toBeCloseTo(10, 5);
    // Head at end of strip 0: x=10, y=5
    expect(result.headPos).toEqual({ x: 10, y: 5 });
  });

  it('unequal-length runs: distance-weighted progress', () => {
    // Strip 0: 10mm, Strip 1: 90mm. Total = 100mm.
    // At progress 0.5, target = 50mm. Strip 0 consumed (10mm), 40mm into strip 1.
    const runs = [makeRun(5, 0, 10), makeRun(6, 0, 90)];
    const result = interpolateRasterRuns(runs, 0.5);
    expect(result.visibleRuns).toHaveLength(2);
    expect(result.visibleRuns[0]).toEqual(runs[0]);
    expect(result.visibleRuns[1].end_x_mm).toBeCloseTo(40, 5);
    // Head at cutoff of strip 1: x=40, y=6
    expect(result.headPos.x).toBeCloseTo(40, 5);
    expect(result.headPos.y).toBe(6);
  });

  it('progress slightly past 1 still returns all runs with head at end', () => {
    const runs = [makeRun(5, 0, 10)];
    const result = interpolateRasterRuns(runs, 1.5);
    expect(result.visibleRuns).toHaveLength(1);
    expect(result.headPos).toEqual({ x: 10, y: 5 });
  });

  it('negative progress returns empty runs', () => {
    const runs = [makeRun(5, 10, 20)];
    const result = interpolateRasterRuns(runs, -0.5);
    expect(result.visibleRuns).toHaveLength(0);
  });

  it('zero-length runs: returns all runs without crashing', () => {
    const runs = [makeRun(5, 10, 10), makeRun(6, 20, 20)];
    const result = interpolateRasterRuns(runs, 0.5);
    expect(result.visibleRuns).toHaveLength(2);
  });

  it('preserves strip power and y_mm in visible output', () => {
    const runs = [
      makeRun(1, 0, 5, 0.2),
      makeRun(2, 0, 5, 0.8),
    ];
    const result = interpolateRasterRuns(runs, 1);
    expect(result.visibleRuns[0].y_mm).toBe(1);
    expect(result.visibleRuns[1].y_mm).toBe(2);
  });

  it('partial strip preserves original start_x_mm and power', () => {
    const runs = [makeRun(5, 10, 30, 0.7)]; // 20mm strip starting at x=10
    const result = interpolateRasterRuns(runs, 0.5);
    expect(result.visibleRuns).toHaveLength(1);
    const partial = result.visibleRuns[0];
    expect(partial.start_x_mm).toBe(10);
    expect(partial.end_x_mm).toBeCloseTo(20, 5); // 10 + 20*0.5
    expect(partial.y_mm).toBe(5);
    // Head at cutoff point
    expect(result.headPos.x).toBeCloseTo(20, 5);
    expect(result.headPos.y).toBe(5);
  });

  it('many runs with different lengths weight correctly', () => {
    // 4 runs: 1mm, 2mm, 3mm, 4mm = 10mm total
    const runs = [
      makeRun(1, 0, 1),
      makeRun(2, 0, 2),
      makeRun(3, 0, 3),
      makeRun(4, 0, 4),
    ];
    // progress 0.3 → target = 3mm → strip0 (1mm) + strip1 (2mm) = 3mm exactly
    const result = interpolateRasterRuns(runs, 0.3);
    expect(result.visibleRuns).toHaveLength(2);
    expect(result.visibleRuns[1].end_x_mm).toBeCloseTo(2, 5);
  });

  it('does not draw overscan halo at time zero before any raster run is burned', () => {
    const ctx = makeMockContext();
    const timeline: AnimationTimeline = {
      segments: [
        {
          type: 'raster',
          layerIndex: 0,
          layerColor: '#ff0000',
          bounds: { min: { x: 0, y: 0 }, max: { x: 14, y: 20 } },
          lineCount: 1,
          lineIntervalMm: 1,
          overscanMm: 2,
          speedMmMin: 1000,
          directionMode: 'bidirectional',
          startTime: 0,
          endTime: 10,
          scanAxis: 'horizontal',
          outlines: [
            {
              points: [
                { x: 2, y: 0 },
                { x: 12, y: 0 },
                { x: 12, y: 20 },
                { x: 2, y: 20 },
              ],
              closed: true,
            },
          ],
          runExtents: [{ y_mm: 10, start_x_mm: 2, end_x_mm: 12, direction: 'left_to_right' }],
        },
      ],
      playbackDuration: 10,
      stats: {
        total_distance_mm: 10,
        estimated_duration_secs: 10,
        segment_count: 1,
        burn_distance_mm: 10,
        travel_distance_mm: 0,
        raster_line_count: 1,
      },
      jobBounds: { min: { x: 0, y: 0 }, max: { x: 14, y: 20 } },
    };
    const vp = { offset: { x: 7, y: 10 }, zoom: 10, canvasWidth: 400, canvasHeight: 300 };

    drawAnimatedPreview(
      ctx,
      timeline,
      0,
      vp,
      {
        showTravel: true,
        showBurnProgress: true,
        showOverscan: true,
        shadeByPower: false,
        invertView: false,
      },
      {} as PreviewBitmapCache,
    );

    expect((ctx as unknown as { fillRect: ReturnType<typeof vi.fn> }).fillRect).not.toHaveBeenCalled();
  });

  it('fill-preferred raster playback uses scanline strokes without a solid grayscale base tint', () => {
    const ctx = makeMockContext();
    const timeline: AnimationTimeline = {
      segments: [
        {
          type: 'raster',
          layerIndex: 0,
          layerColor: '#000000',
          preferRunPreview: true,
          bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } },
          lineCount: 2,
          lineIntervalMm: 1,
          overscanMm: 0,
          speedMmMin: 1000,
          directionMode: 'bidirectional',
          startTime: 0,
          endTime: 10,
          scanAxis: 'horizontal',
          outlines: [
            {
              points: [
                { x: 0, y: 0 },
                { x: 10, y: 0 },
                { x: 10, y: 10 },
                { x: 0, y: 10 },
              ],
              closed: true,
            },
          ],
          runExtents: [
            { y_mm: 2, start_x_mm: 1, end_x_mm: 9, direction: 'left_to_right' },
            { y_mm: 4, start_x_mm: 1, end_x_mm: 9, direction: 'left_to_right' },
          ],
        },
      ],
      playbackDuration: 10,
      stats: {
        total_distance_mm: 16,
        estimated_duration_secs: 10,
        segment_count: 1,
        burn_distance_mm: 16,
        travel_distance_mm: 0,
        raster_line_count: 2,
      },
      jobBounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } },
    };
    const vp = { offset: { x: 0, y: 0 }, zoom: 10, canvasWidth: 400, canvasHeight: 300 };

    drawAnimatedPreview(
      ctx,
      timeline,
      10,
      vp,
      {
        showTravel: true,
        showBurnProgress: false,
        showOverscan: true,
        shadeByPower: false,
        invertView: false,
      },
      {} as PreviewBitmapCache,
    );

    expect((ctx as unknown as { fill: ReturnType<typeof vi.fn> }).fill).not.toHaveBeenCalled();
    expect((ctx as unknown as { stroke: ReturnType<typeof vi.fn> }).stroke).toHaveBeenCalled();
  });

  it('fill-preferred raster playback uses tinted bitmap masks when preview bitmaps are available', () => {
    const ctx = makeMockContext();
    const burnedMask = document.createElement('canvas');
    burnedMask.width = 8;
    burnedMask.height = 8;
    const burnedMaskCtx = {
      clearRect: vi.fn(),
      drawImage: vi.fn(),
    };
    vi.spyOn(burnedMask, 'getContext').mockReturnValue(burnedMaskCtx as unknown as CanvasRenderingContext2D);
    const tintedMask = document.createElement('canvas');
    tintedMask.width = 8;
    tintedMask.height = 8;
    const tintedCtx = {
      clearRect: vi.fn(),
      drawImage: vi.fn(),
      fillRect: vi.fn(),
      globalCompositeOperation: 'source-over',
      fillStyle: '',
    };
    vi.spyOn(tintedMask, 'getContext').mockReturnValue(tintedCtx as unknown as CanvasRenderingContext2D);
    const bitmapCache = {
      ensurePreviewBitmap: vi.fn(() => ({ complete: true, naturalWidth: 8 } as HTMLImageElement)),
      ensureBurnedMask: vi.fn(() => burnedMask),
      ensureTintedBurnedMask: vi.fn(() => tintedMask),
    } as unknown as PreviewBitmapCache;

    const timeline: AnimationTimeline = {
      segments: [
        {
          type: 'raster',
          layerIndex: 0,
          layerColor: '#ff4444',
          preferRunPreview: true,
          bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } },
          lineCount: 2,
          lineIntervalMm: 1,
          overscanMm: 0,
          speedMmMin: 1000,
          directionMode: 'bidirectional',
          startTime: 0,
          endTime: 10,
          scanAxis: 'horizontal',
          previewBitmap: {
            width_px: 8,
            height_px: 8,
            png_bytes: [1, 2, 3],
          },
          localOriginMm: { x: 0, y: 0 },
          localWidthMm: 10,
          localHeightMm: 10,
          outlines: [
            {
              points: [
                { x: 0, y: 0 },
                { x: 10, y: 0 },
                { x: 10, y: 10 },
                { x: 0, y: 10 },
              ],
              closed: true,
            },
          ],
          runExtents: [
            { y_mm: 2, start_x_mm: 1, end_x_mm: 9, direction: 'left_to_right' },
            { y_mm: 4, start_x_mm: 1, end_x_mm: 9, direction: 'left_to_right' },
          ],
          sequence: 3,
        },
      ],
      playbackDuration: 10,
      stats: {
        total_distance_mm: 16,
        estimated_duration_secs: 10,
        segment_count: 1,
        burn_distance_mm: 16,
        travel_distance_mm: 0,
        raster_line_count: 2,
      },
      jobBounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } },
    };
    const vp = { offset: { x: 0, y: 0 }, zoom: 10, canvasWidth: 400, canvasHeight: 300 };

    drawAnimatedPreview(
      ctx,
      timeline,
      10,
      vp,
      {
        showTravel: true,
        showBurnProgress: false,
        showOverscan: true,
        shadeByPower: false,
        invertView: false,
      },
      bitmapCache,
    );

    expect(bitmapCache.ensureTintedBurnedMask).toHaveBeenCalledWith(3, 8, 8);
    expect(tintedCtx.fillRect).toHaveBeenCalled();
    expect((ctx as unknown as { drawImage: ReturnType<typeof vi.fn> }).drawImage).toHaveBeenCalledWith(
      tintedMask,
      0,
      0,
      10,
      10,
    );
  });

  it('suppresses overscan markers when showOverscan is false', () => {
    const ctx = makeMockContext();
    const timeline: AnimationTimeline = {
      segments: [
        {
          type: 'raster',
          layerIndex: 0,
          layerColor: '#ff0000',
          preferRunPreview: true,
          bounds: { min: { x: 0, y: 0 }, max: { x: 14, y: 20 } },
          lineCount: 1,
          lineIntervalMm: 1,
          overscanMm: 2,
          speedMmMin: 1000,
          directionMode: 'bidirectional',
          startTime: 0,
          endTime: 10,
          scanAxis: 'horizontal',
          outlines: [
            {
              points: [
                { x: 2, y: 0 },
                { x: 12, y: 0 },
                { x: 12, y: 20 },
                { x: 2, y: 20 },
              ],
              closed: true,
            },
          ],
          runExtents: [{ y_mm: 10, start_x_mm: 2, end_x_mm: 12, direction: 'left_to_right' }],
        },
      ],
      playbackDuration: 10,
      stats: {
        total_distance_mm: 10,
        estimated_duration_secs: 10,
        segment_count: 1,
        burn_distance_mm: 10,
        travel_distance_mm: 0,
        raster_line_count: 1,
      },
      jobBounds: { min: { x: 0, y: 0 }, max: { x: 14, y: 20 } },
    };
    const vp = { offset: { x: 7, y: 10 }, zoom: 10, canvasWidth: 400, canvasHeight: 300 };

    drawAnimatedPreview(
      ctx,
      timeline,
      10,
      vp,
      {
        showTravel: true,
        showBurnProgress: false,
        showOverscan: false,
        shadeByPower: false,
        invertView: false,
      },
      {} as PreviewBitmapCache,
    );

    expect((ctx as unknown as { fillRect: ReturnType<typeof vi.fn> }).fillRect).not.toHaveBeenCalled();
  });

  // --- Multi-run gap tests: head follows actual runs, not blank space ---

  it('multi-run scanline: head jumps over gap between runs', () => {
    // Two runs on the same scanline (y=10):
    //   Run A: x=0-40 (40mm), Run B: x=60-100 (40mm)
    // Total distance = 80mm.  Gap at x=40-60 is not traversed by head.
    const runs = [
      makeRun(10, 0, 40),   // Run A
      makeRun(10, 60, 100), // Run B
    ];

    // At progress 0.5 → 40mm consumed → just finished strip 0
    const r50 = interpolateRasterRuns(runs, 0.5);
    expect(r50.headPos.x).toBeCloseTo(40, 5);
    expect(r50.headPos.y).toBe(10);

    // At progress 0.625 → 50mm consumed → 10mm into strip 1 (starts at 60)
    // cutoff = 60 + (100-60)*(10/40) = 60 + 10 = 70
    const r625 = interpolateRasterRuns(runs, 0.625);
    expect(r625.headPos.x).toBeCloseTo(70, 5);
    expect(r625.headPos.y).toBe(10);

    // Head never visits x=41-59 (the blank gap)
  });

  it('multi-run: head does not pass through blank space at any progress', () => {
    // Two runs with 20mm gap
    const runs = [
      makeRun(5, 0, 10),   // 10mm
      makeRun(5, 30, 50),  // 20mm
    ];
    // Total = 30mm. At any progress, head.x should be either in [0,10] or [30,50].
    for (let p = 0.01; p <= 1.0; p += 0.01) {
      const r = interpolateRasterRuns(runs, p);
      const hx = r.headPos.x;
      const inRunA = hx >= -0.001 && hx <= 10.001;
      const inRunB = hx >= 29.999 && hx <= 50.001;
      expect(inRunA || inRunB).toBe(true);
    }
  });

  // --- Vertical scan: coordinate un-transposition ---

  it('vertical scan: headPos un-transposes y_mm to world-X', () => {
    // Vertical scan: y_mm = world-X position, start/end = world-Y extent
    const runs = [makeRun(15, 0, 20)]; // world-X=15, world-Y range 0-20
    const result = interpolateRasterRuns(runs, 0.5, true);
    // Head should be at world coords: x=y_mm=15, y=cutoff=10
    expect(result.headPos.x).toBe(15);
    expect(result.headPos.y).toBeCloseTo(10, 5);
  });

  it('vertical scan at completion: head at last strip end, un-transposed', () => {
    const runs = [makeRun(5, 0, 10), makeRun(8, 0, 10)];
    const result = interpolateRasterRuns(runs, 1, true);
    // Last strip: y_mm=8 → world-X=8, end_x_mm=10 → world-Y=10
    expect(result.headPos).toEqual({ x: 8, y: 10 });
  });

  it('rotated scan: head position is resolved in world space', () => {
    const runs = [makeRun(0, 0, 10)];
    const result = interpolateRasterRuns(runs, 1, {
      scanAxis: 'horizontal',
      scanAngleDeg: 45,
      scanOrigin: { x: 10, y: 10 },
    });
    expect(result.headPos.x).toBeCloseTo(10 + 10 * Math.cos(Math.PI / 4), 5);
    expect(result.headPos.y).toBeCloseTo(10 + 10 * Math.sin(Math.PI / 4), 5);
  });
});

// --- computeRasterHeadPos tests (legacy fallback for regions without tone runs) ---

const BOUNDS_10x20 = { min: { x: 0, y: 0 }, max: { x: 10, y: 20 } };

describe('computeRasterHeadPos (legacy, no tone runs)', () => {
  // --- Basic boundary tests ---

  it('returns scan start / advance start at progress 0', () => {
    const head = computeRasterHeadPos(BOUNDS_10x20, 10, 'unidirectional', 0, undefined);
    expect(head.x).toBeCloseTo(0, 5); // scanMin
    expect(head.y).toBeCloseTo(0, 5); // advMin (line 0)
  });

  it('returns scan end / advance end at progress 1', () => {
    // p=1, lc=10: lineProgress=10, currentLine=9 (last), withinLine=1
    // lineAdv = 0 + (9/9)*20 = 20
    // isLastLine: effectiveScanFrac=1, phaseFrac=1 → scanPos = 0+1*10 = 10
    const head = computeRasterHeadPos(BOUNDS_10x20, 10, 'unidirectional', 1, undefined);
    expect(head.x).toBeCloseTo(10, 5);
    expect(head.y).toBeCloseTo(20, 5);
  });

  it('clamps progress to 0..1', () => {
    const headNeg = computeRasterHeadPos(BOUNDS_10x20, 10, 'unidirectional', -0.5, undefined);
    expect(headNeg.x).toBeCloseTo(0, 5);
    expect(headNeg.y).toBeCloseTo(0, 5);

    const headOver = computeRasterHeadPos(BOUNDS_10x20, 10, 'unidirectional', 1.5, undefined);
    expect(headOver.x).toBeCloseTo(10, 5);
    expect(headOver.y).toBeCloseTo(20, 5);
  });

  it('handles zero lineCount gracefully', () => {
    // lc=0 → fallback to 1, lineAdv = advMin + advExtent*0.5 = 10
    const head = computeRasterHeadPos(BOUNDS_10x20, 0, 'unidirectional', 0.5, undefined);
    expect(head.x).toBeCloseTo(5, 5);
    expect(head.y).toBeCloseTo(10, 5);
  });

  // --- Stepped Y (advance axis) ---

  it('Y steps per line rather than interpolating smoothly', () => {
    // 4 lines at Y = 0, 6.667, 13.333, 20 (evenly spaced by 1/3 of 20)
    // p=0.5 → lineProgress=2, currentLine=2 → lineAdv = (2/3)*20 = 13.333
    const head = computeRasterHeadPos(BOUNDS_10x20, 4, 'unidirectional', 0.5, undefined);
    expect(head.y).toBeCloseTo(20 * 2 / 3, 3);
    expect(head.x).toBeCloseTo(0, 5); // withinLine=0 → start of line
  });

  it('Y is constant within a single line scan', () => {
    // p=0.125 → lineProgress=0.5 → line 0, frac 0.5 → Y stays at line 0
    const head1 = computeRasterHeadPos(BOUNDS_10x20, 4, 'bidirectional', 0.125, undefined);
    // p=0.24 → lineProgress=0.96 → line 0, frac 0.96 → still line 0
    const head2 = computeRasterHeadPos(BOUNDS_10x20, 4, 'bidirectional', 0.24, undefined);
    expect(head1.y).toBeCloseTo(0, 5);
    expect(head2.y).toBeCloseTo(0, 5);
  });

  // --- Bidirectional direction alternation ---

  it('bidirectional: even line sweeps LTR', () => {
    // 4 lines, p=0.125 → lineProgress=0.5, line 0 (even), frac 0.5
    const head = computeRasterHeadPos(BOUNDS_10x20, 4, 'bidirectional', 0.125, undefined);
    expect(head.x).toBeCloseTo(5, 5); // scanMin + 0.5 * 10
    expect(head.y).toBeCloseTo(0, 5); // line 0
  });

  it('bidirectional: odd line sweeps RTL', () => {
    // 4 lines, p=0.375 → lineProgress=1.5, line 1 (odd), frac 0.5
    // RTL: scanMax - 0.5 * scanExtent = 10 - 5 = 5
    // lineAdv = (1/3)*20 = 6.667
    const head = computeRasterHeadPos(BOUNDS_10x20, 4, 'bidirectional', 0.375, undefined);
    expect(head.x).toBeCloseTo(5, 5);
    expect(head.y).toBeCloseTo(20 / 3, 3);
  });

  // --- Unidirectional with speed: rapid return ---

  it('unidirectional without speed: no return phase modelled', () => {
    // speedMmMin=0 (default) → scanFrac=1, no return
    // p=0.375 → lineProgress=1.5, line 1, frac 0.5 → scan phase
    const head = computeRasterHeadPos(BOUNDS_10x20, 4, 'unidirectional', 0.375, undefined);
    expect(head.x).toBeCloseTo(5, 5); // mid-scan LTR
    expect(head.y).toBeCloseTo(20 / 3, 3); // line 1
  });

  it('unidirectional with speed: scan phase then return phase per line', () => {
    // scanExtent=10, speed=1000, rapid=10000
    // scanTime = 10/1000 = 0.01, returnTime = 10/10000 = 0.001
    // scanFrac = 0.01 / 0.011 ≈ 0.9091
    // 4 lines, p=0.25 → lineProgress=1.0, line 1, withinLine=0.0
    // withinLine (0) <= scanFrac → scan phase, phaseFrac=0 → scanMin=0
    const head0 = computeRasterHeadPos(BOUNDS_10x20, 4, 'unidirectional', 0.25, undefined, 1000);
    expect(head0.x).toBeCloseTo(0, 5); // start of line 1 scan
    expect(head0.y).toBeCloseTo(20 / 3, 3); // line 1

    // Now pick a progress well into a line that triggers return phase.
    // line 0 cycle: withinLine = frac * 4 - 0. With scanFrac≈0.9091,
    // return starts at withinLine > 0.9091.
    // p = 0.24 → lineProgress=0.96 → line 0, withinLine=0.96 > 0.9091 → return phase
    const headReturn = computeRasterHeadPos(BOUNDS_10x20, 4, 'unidirectional', 0.24, undefined, 1000);
    // During return: head should be between scanMax (10) and scanMin (0), moving back
    expect(headReturn.x).toBeLessThan(10);
    expect(headReturn.x).toBeGreaterThan(0);
    // Y should be transitioning from line 0 (0) toward line 1 (6.667)
    expect(headReturn.y).toBeGreaterThan(0);
    expect(headReturn.y).toBeLessThan(20 / 3);
  });

  it('unidirectional: last line has no return phase', () => {
    // With speed=1000, scanFrac≈0.9091 for non-last lines.
    // On last line (line 3 with lc=4), effectiveScanFrac=1 → full scan, no return.
    // p=0.99 → lineProgress=3.96, line 3 (last), withinLine=0.96
    // effectiveScanFrac=1, so withinLine (0.96) <= 1 → scan phase
    // phaseFrac = 0.96, scanPos = 0 + 0.96*10 = 9.6
    const head = computeRasterHeadPos(BOUNDS_10x20, 4, 'unidirectional', 0.99, undefined, 1000);
    expect(head.x).toBeCloseTo(9.6, 1);
    expect(head.y).toBeCloseTo(20, 5); // line 3 = advMax
  });

  // --- Vertical scan axis ---

  it('vertical scan: advance is X, scan is Y', () => {
    // Vertical, 4 lines, p=0.5 → lineProgress=2, line 2, withinLine=0
    // advPos = advMin + (2/3)*advExtent = 0 + (2/3)*10 = 3.333 (X)
    // scanPos = scanMin = 0 (Y)
    const head = computeRasterHeadPos(BOUNDS_10x20, 4, 'unidirectional', 0.5, 'vertical');
    expect(head.x).toBeCloseTo(10 * 2 / 3, 3);
    expect(head.y).toBeCloseTo(0, 5);
  });

  it('vertical bidirectional: odd line sweeps scanMax→scanMin', () => {
    // 4 lines, p=0.375 → lineProgress=1.5, line 1 (odd), frac 0.5
    // advPos = (1/3)*10 = 3.333 (X), scanPos = 20 - 0.5*20 = 10 (Y)
    const head = computeRasterHeadPos(BOUNDS_10x20, 4, 'bidirectional', 0.375, 'vertical');
    expect(head.y).toBeCloseTo(10, 5);
    expect(head.x).toBeCloseTo(10 / 3, 3);
  });

  // --- Y monotonicity for unidirectional (advance direction never reverses) ---

  it('advance axis is monotonically non-decreasing for unidirectional', () => {
    const positions: number[] = [];
    for (let p = 0; p <= 1; p += 0.01) {
      const head = computeRasterHeadPos(BOUNDS_10x20, 100, 'unidirectional', p, undefined, 1000);
      positions.push(head.y);
    }
    for (let i = 1; i < positions.length; i++) {
      expect(positions[i]).toBeGreaterThanOrEqual(positions[i - 1] - 0.001);
    }
  });

  // --- Overscan: head sweeps across full bounds (bounds already include overscan) ---

  it('head sweeps across full bounds width including overscan', () => {
    // Bounds {0,0}-{14,20} represent burn area (10mm) + 2mm overscan on each side.
    // Head should reach x=0 at scan start and x=14 at scan end.
    const osBounds = { min: { x: 0, y: 0 }, max: { x: 14, y: 20 } };
    // p=0.99 → last line, near end of scan
    const head = computeRasterHeadPos(osBounds, 4, 'bidirectional', 0.99, undefined);
    // Last line (line 3, odd): RTL, withinLine=0.96 → scanMax - 0.96*14 = 14-13.44 = 0.56
    expect(head.x).toBeGreaterThanOrEqual(0);
    expect(head.x).toBeLessThanOrEqual(14);
  });
});
