import { describe, expect, it } from 'vitest';
import type { RasterSegment } from '../previewTimeline';
import { computeRasterMotionProgress } from '../rasterPreviewMotion';

function makeRasterSegment(overrides: Partial<RasterSegment> = {}): RasterSegment {
  return {
    type: 'raster',
    layerIndex: 0,
    layerColor: '#000000',
    bounds: { min: { x: -1, y: 0 }, max: { x: 11, y: 3 } },
    lineCount: 4,
    lineIntervalMm: 1,
    overscanMm: 1,
    speedMmMin: 600,
    directionMode: 'bidirectional',
    startTime: 0,
    endTime: 10,
    scanAxis: 'horizontal',
    runExtents: [
      { y_mm: 0, start_x_mm: 0, end_x_mm: 10, direction: 'left_to_right' },
      { y_mm: 1, start_x_mm: 0, end_x_mm: 10, direction: 'right_to_left' },
      { y_mm: 2, start_x_mm: 0, end_x_mm: 10, direction: 'left_to_right' },
      { y_mm: 3, start_x_mm: 0, end_x_mm: 10, direction: 'right_to_left' },
    ],
    scanlineExtents: [
      { y_mm: 0, start_x_mm: 0, end_x_mm: 10, direction: 'left_to_right' },
      { y_mm: 1, start_x_mm: 0, end_x_mm: 10, direction: 'right_to_left' },
      { y_mm: 2, start_x_mm: 0, end_x_mm: 10, direction: 'left_to_right' },
      { y_mm: 3, start_x_mm: 0, end_x_mm: 10, direction: 'right_to_left' },
    ],
    ...overrides,
  };
}

describe('computeRasterMotionProgress', () => {
  it('does not expose future overscan rows on a dense raster', () => {
    const segment = makeRasterSegment();

    const early = computeRasterMotionProgress(segment, 0.1);
    expect(early).not.toBeNull();
    expect(early!.visibleOverscanRuns).toHaveLength(1);

    const middle = computeRasterMotionProgress(segment, 0.5);
    expect(middle).not.toBeNull();
    expect(middle!.visibleOverscanRuns.length).toBeGreaterThanOrEqual(2);
    expect(middle!.visibleOverscanRuns.length).toBeLessThanOrEqual(3);
  });

  it('includes overscan, internal gaps, and inter-row positioning as travel', () => {
    const segment = makeRasterSegment({
      lineCount: 2,
      bounds: { min: { x: -1, y: 0 }, max: { x: 11, y: 1 } },
      runExtents: [
        { y_mm: 0, start_x_mm: 0, end_x_mm: 2, direction: 'left_to_right' },
        { y_mm: 0, start_x_mm: 8, end_x_mm: 10, direction: 'left_to_right' },
        { y_mm: 1, start_x_mm: 0, end_x_mm: 10, direction: 'right_to_left' },
      ],
      scanlineExtents: [
        { y_mm: 0, start_x_mm: 0, end_x_mm: 10, direction: 'left_to_right' },
        { y_mm: 1, start_x_mm: 0, end_x_mm: 10, direction: 'right_to_left' },
      ],
    });

    const result = computeRasterMotionProgress(segment, 1);
    expect(result).not.toBeNull();
    // Row 1: lead-in, internal gap, lead-out. Row 2: lead-in + lead-out.
    // One additional trace positions between the two rows.
    expect(result!.travelTraces).toHaveLength(6);
    expect(result!.travelTraces).toContainEqual({
      from: { x: 2, y: 0 },
      to: { x: 8, y: 0 },
    });
  });

  it('holds burn progress steady while the head crosses a laser-off gap', () => {
    const segment = makeRasterSegment({
      lineCount: 1,
      bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 1 } },
      overscanMm: 0,
      runExtents: [
        { y_mm: 0, start_x_mm: 0, end_x_mm: 2, direction: 'left_to_right' },
        { y_mm: 0, start_x_mm: 8, end_x_mm: 10, direction: 'left_to_right' },
      ],
      scanlineExtents: [{ y_mm: 0, start_x_mm: 0, end_x_mm: 10, direction: 'left_to_right' }],
    });

    const result = computeRasterMotionProgress(segment, 0.5);
    expect(result).not.toBeNull();
    expect(result!.headPos).toEqual({ x: 5, y: 0 });
    expect(result!.visibleRuns).toEqual([
      { y_mm: 0, start_x_mm: 0, end_x_mm: 2, direction: 'left_to_right' },
    ]);
    expect(result!.lastCompleteIndex).toBe(0);
  });

  it('follows right-to-left row direction', () => {
    const segment = makeRasterSegment({
      lineCount: 1,
      bounds: { min: { x: -1, y: 0 }, max: { x: 11, y: 1 } },
      runExtents: [{ y_mm: 0, start_x_mm: 0, end_x_mm: 10, direction: 'right_to_left' }],
      scanlineExtents: [{ y_mm: 0, start_x_mm: 0, end_x_mm: 10, direction: 'right_to_left' }],
    });

    const result = computeRasterMotionProgress(segment, 0.25);
    expect(result).not.toBeNull();
    expect(result!.headPos.x).toBeCloseTo(8, 6);
    expect(result!.visibleRuns[0].start_x_mm).toBeCloseTo(8, 6);
    expect(result!.visibleRuns[0].end_x_mm).toBeCloseTo(10, 6);
  });

  it('returns no visible work or overscan at the exact start', () => {
    const result = computeRasterMotionProgress(makeRasterSegment(), 0);
    expect(result).not.toBeNull();
    expect(result!.visibleRuns).toEqual([]);
    expect(result!.visibleOverscanRuns).toEqual([]);
    expect(result!.travelTraces).toEqual([]);
  });

  it('keeps a completed row complete while repositioning to the next row', () => {
    const segment = makeRasterSegment({
      lineCount: 2,
      bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 100 } },
      overscanMm: 0,
      speedMmMin: 6000,
      runExtents: [
        { y_mm: 0, start_x_mm: 0, end_x_mm: 10, direction: 'left_to_right' },
        { y_mm: 100, start_x_mm: 0, end_x_mm: 10, direction: 'right_to_left' },
      ],
      scanlineExtents: [
        { y_mm: 0, start_x_mm: 0, end_x_mm: 10, direction: 'left_to_right' },
        { y_mm: 100, start_x_mm: 0, end_x_mm: 10, direction: 'right_to_left' },
      ],
    });

    // The long inter-row move dominates the timeline; 20% is after the first
    // feed sweep but before the second row begins.
    const result = computeRasterMotionProgress(segment, 0.2);
    expect(result).not.toBeNull();
    expect(result!.visibleOverscanRuns).toHaveLength(1);
    expect(result!.lastOverscanRowComplete).toBe(true);
  });
});
