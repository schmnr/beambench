import { describe, it, expect } from 'vitest';
import { buildTimeline, polylineLength } from '../previewTimeline';
import type { AnimationSegment } from '../previewTimeline';
import type { PreviewData } from '../../types/preview';

function makeVectorPreview(overrides = {}) {
  return {
    points: [{ x: 0, y: 0 }, { x: 10, y: 0 }],
    closed: false,
    power_percent: 50,
    speed_mm_min: 600,
    sequence: 1,
    ...overrides,
  };
}

function makeRasterPreview(overrides = {}) {
  return {
    bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } },
    line_count: 10,
    line_interval_mm: 0.1,
    direction_mode: 'bidirectional' as const,
    power_mode: 'binary' as const,
    speed_mm_min: 1200,
    fill_density: 1,
    scan_angle_deg: 0,
    scan_origin: { x: 0, y: 0 },
    overscan_mm: 0,
    outlines: [],
    scan_axis: 'horizontal' as const,
    sequence: 2,
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

function makeTravelMove(overrides = {}) {
  return {
    from: { x: 0, y: 0 },
    to: { x: 30, y: 40 },
    sequence: 0,
    ...overrides,
  };
}

function makePreviewData(override?: Partial<PreviewData>): PreviewData {
  return {
    plan_id: 'plan-1',
    revision_hash: 'abc123',
    bounds: { min: { x: 0, y: 0 }, max: { x: 100, y: 100 } },
    layers: [],
    travel_moves: [],
    frame: null,
    stats: {
      total_distance_mm: 0,
      travel_distance_mm: 0,
      burn_distance_mm: 0,
      estimated_duration_secs: 0,
      segment_count: 0,
      raster_line_count: 0,
    },
    warnings: [],
    ...override,
    failed_entries: override?.failed_entries ?? [],
  };
}

function segmentsOfType(segments: AnimationSegment[], type: string) {
  return segments.filter((s) => s.type === type);
}

describe('polylineLength', () => {
  it('returns 0 for empty or single-point arrays', () => {
    expect(polylineLength([])).toBe(0);
    expect(polylineLength([{ x: 5, y: 5 }])).toBe(0);
  });

  it('computes length for a simple horizontal line', () => {
    expect(polylineLength([{ x: 0, y: 0 }, { x: 10, y: 0 }])).toBe(10);
  });

  it('computes length for a multi-segment polyline', () => {
    const pts = [{ x: 0, y: 0 }, { x: 3, y: 4 }, { x: 6, y: 0 }];
    expect(polylineLength(pts)).toBe(10); // 5 + 5
  });
});

describe('buildTimeline', () => {
  it('returns empty timeline for empty PreviewData', () => {
    const data = makePreviewData();
    const tl = buildTimeline(data, ['#ff0000']);

    expect(tl.segments).toHaveLength(0);
    expect(tl.playbackDuration).toBe(0);
  });

  it('creates a single vector segment with correct timing', () => {
    const data = makePreviewData({
      layers: [{
        layer_id: 'L1',
        vector_paths: [makeVectorPreview()],
        raster_regions: [],
      }],
      stats: {
        total_distance_mm: 10,
        travel_distance_mm: 0,
        burn_distance_mm: 10,
        estimated_duration_secs: 1,
        segment_count: 1,
        raster_line_count: 0,
      },
    });

    const tl = buildTimeline(data, ['#ff0000']);

    const vectors = segmentsOfType(tl.segments, 'vector');
    expect(vectors).toHaveLength(1);
    const seg = vectors[0];
    if (seg.type === 'vector') {
      expect(seg.points).toHaveLength(2);
      expect(seg.closed).toBe(false);
      expect(seg.powerPercent).toBe(50);
      expect(seg.layerColor).toBe('#ff0000');
      expect(seg.endTime - seg.startTime).toBeCloseTo(1, 5);
    }
    // Stats pass-through
    expect(tl.stats.estimated_duration_secs).toBe(1);
  });

  it('creates a raster segment with accurate duration model', () => {
    const data = makePreviewData({
      layers: [{
        layer_id: 'L1',
        vector_paths: [],
        raster_regions: [makeRasterPreview({
          bounds: { min: { x: 10, y: 10 }, max: { x: 30, y: 30 } },
          direction_mode: 'unidirectional',
          speed_mm_min: 1200,
          fill_density: 0.5,
        })],
      }],
    });

    const tl = buildTimeline(data, ['#0000ff']);

    const rasters = segmentsOfType(tl.segments, 'raster');
    expect(rasters).toHaveLength(1);
    const seg = rasters[0];
    if (seg.type === 'raster') {
      expect(seg.bounds.min).toEqual({ x: 10, y: 10 });
      expect(seg.lineCount).toBe(10);
      // The head sweeps each full 20mm row once, including internal S0 gaps.
      // feedSecs = (10*20/1200)*60 = 10.0
      expect(seg.endTime - seg.startTime).toBeCloseTo(10, 3);
    }
  });

  it('raster duration accounts for overscan and vertical scan axis', () => {
    const data = makePreviewData({
      layers: [{
        layer_id: 'L1',
        vector_paths: [],
        raster_regions: [makeRasterPreview({
          // Vertical scan: scan width is Y extent (bounds are un-transposed)
          bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 50 } },
          line_count: 5,
          direction_mode: 'bidirectional',
          speed_mm_min: 3000,
          fill_density: 1.0,
          overscan_mm: 5,
          scan_axis: 'vertical',
        })],
      }],
    });

    const tl = buildTimeline(data, ['#ff0000']);
    const seg = segmentsOfType(tl.segments, 'raster')[0];
    if (seg.type === 'raster') {
      // Vertical: scanWidth = 50 (Y extent), burnWidth = 50 - 2*5 = 40
      // feed per line = 40*1.0 + 2*5 = 50mm, rapid per line = 40*0 = 0
      // feedSecs = (5*50/3000)*60 = 5.0
      expect(seg.endTime - seg.startTime).toBeCloseTo(5, 3);
    }
  });

  it('passes direction_mode through to raster segments', () => {
    const data = makePreviewData({
      layers: [{
        layer_id: 'L1',
        vector_paths: [],
        raster_regions: [makeRasterPreview({
          direction_mode: 'unidirectional',
          speed_mm_min: 600,
          fill_density: 1.0,
          line_count: 5,
        })],
      }],
    });

    const tl = buildTimeline(data, ['#ff0000']);
    const seg = segmentsOfType(tl.segments, 'raster')[0];
    if (seg.type === 'raster') {
      expect(seg.directionMode).toBe('unidirectional');
    }

    // Also verify bidirectional
    const data2 = makePreviewData({
      layers: [{
        layer_id: 'L1',
        vector_paths: [],
        raster_regions: [makeRasterPreview({
          speed_mm_min: 600,
          fill_density: 1.0,
          line_count: 5,
        })],
      }],
    });

    const tl2 = buildTimeline(data2, ['#ff0000']);
    const seg2 = segmentsOfType(tl2.segments, 'raster')[0];
    if (seg2.type === 'raster') {
      expect(seg2.directionMode).toBe('bidirectional');
    }
  });

  it('handles mixed vector + raster with correct total burn duration', () => {
    const data = makePreviewData({
      layers: [{
        layer_id: 'L1',
        vector_paths: [makeVectorPreview({
          points: [{ x: 0, y: 0 }, { x: 6, y: 0 }],
          speed_mm_min: 360, // 1s
        })],
        raster_regions: [makeRasterPreview({
          speed_mm_min: 600, // 5*10=50mm, 50/600*60 = 5s
          fill_density: 1.0,
          line_count: 5,
        })],
      }],
    });

    const tl = buildTimeline(data, ['#ff0000']);

    const vectors = segmentsOfType(tl.segments, 'vector');
    const rasters = segmentsOfType(tl.segments, 'raster');
    expect(vectors).toHaveLength(1);
    expect(rasters).toHaveLength(1);

    // Total burn duration should be 1 + 5 = 6s (travel adds separately)
    const burnDuration = [...vectors, ...rasters].reduce(
      (acc, s) => acc + (s.endTime - s.startTime), 0,
    );
    expect(burnDuration).toBeCloseTo(6, 5);
  });

  it('includes travel moves as timed segments in the timeline', () => {
    const data = makePreviewData({
      travel_moves: [
        makeTravelMove(), // 50mm
      ],
      layers: [{
        layer_id: 'L1',
        vector_paths: [makeVectorPreview({
          points: [{ x: 30, y: 40 }, { x: 40, y: 40 }],
          power_percent: 100,
        })],
        raster_regions: [],
      }],
    });

    // Use rapid speed of 3000 mm/min: 50mm / 3000 * 60 = 1s
    const tl = buildTimeline(data, ['#ff0000'], 3000);

    const travels = segmentsOfType(tl.segments, 'travel');
    expect(travels.length).toBeGreaterThanOrEqual(1);

    // Travel segment should have non-zero duration
    const travelDuration = travels.reduce((acc, s) => acc + (s.endTime - s.startTime), 0);
    expect(travelDuration).toBeCloseTo(1, 5);

    // Total playback should include both travel and burn time
    expect(tl.playbackDuration).toBeGreaterThan(1);
  });

  it('skipTravelTime collapses travel to zero duration', () => {
    const data = makePreviewData({
      travel_moves: [
        makeTravelMove(), // 50mm
      ],
      layers: [{
        layer_id: 'L1',
        vector_paths: [makeVectorPreview({
          points: [{ x: 30, y: 40 }, { x: 40, y: 40 }],
          power_percent: 100,
        })],
        raster_regions: [],
      }],
    });

    // Without skip: travel takes time
    const tlWith = buildTimeline(data, ['#ff0000'], 3000);
    const travelWith = segmentsOfType(tlWith.segments, 'travel');
    expect(travelWith[0].endTime - travelWith[0].startTime).toBeGreaterThan(0);
    expect(tlWith.playbackDuration).toBeGreaterThan(1);

    // With skip: travel has zero duration
    const tlSkip = buildTimeline(data, ['#ff0000'], 3000, true);
    const travelSkip = segmentsOfType(tlSkip.segments, 'travel');
    expect(travelSkip).toHaveLength(1); // segment still exists
    expect(travelSkip[0].endTime - travelSkip[0].startTime).toBe(0);

    // Playback duration should only include burn time (1s)
    expect(tlSkip.playbackDuration).toBeCloseTo(1, 5);
  });

  it('uses backend duration_secs for raster when available', () => {
    const data = makePreviewData({
      layers: [{
        layer_id: 'L1',
        vector_paths: [],
        raster_regions: [makeRasterPreview({
          bounds: { min: { x: 0, y: 0 }, max: { x: 100, y: 100 } },
          line_count: 50,
          speed_mm_min: 3000,
          duration_secs: 42.5, // backend-computed exact duration
        })],
      }],
    });

    const tl = buildTimeline(data, ['#ff0000']);
    const rasters = segmentsOfType(tl.segments, 'raster');
    expect(rasters).toHaveLength(1);
    // Should use the backend duration, not the estimate
    expect(rasters[0].endTime - rasters[0].startTime).toBeCloseTo(42.5, 5);
  });

  it('includes frame as a timed segment at the start', () => {
    const data = makePreviewData({
      frame: {
        path: [{ x: 0, y: 0 }, { x: 100, y: 0 }], // 100mm
        power_percent: 5,
        speed_mm_min: 6000, // 100/6000*60 = 1s
      },
    });

    const tl = buildTimeline(data, []);

    const frames = segmentsOfType(tl.segments, 'frame');
    expect(frames).toHaveLength(1);
    expect(frames[0].startTime).toBe(0);
    expect(frames[0].endTime - frames[0].startTime).toBeCloseTo(1, 5);
    expect(tl.playbackDuration).toBeCloseTo(1, 5);
  });

  it('passes through jobBounds and stats', () => {
    const bounds = { min: { x: -5, y: -5 }, max: { x: 50, y: 50 } };
    const data = makePreviewData({ bounds });

    const tl = buildTimeline(data, []);
    expect(tl.jobBounds).toBe(bounds);
    expect(tl.stats).toBe(data.stats);
  });

  it('handles zero-speed segments gracefully', () => {
    const data = makePreviewData({
      layers: [{
        layer_id: 'L1',
        vector_paths: [makeVectorPreview({
          power_percent: 0,
          speed_mm_min: 0,
        })],
        raster_regions: [],
      }],
    });

    const tl = buildTimeline(data, ['#fff']);
    const vectors = segmentsOfType(tl.segments, 'vector');
    expect(vectors).toHaveLength(1);
    expect(vectors[0].endTime - vectors[0].startTime).toBe(0);
  });

  it('passes end_point through to raster segments', () => {
    const data = makePreviewData({
      layers: [{
        layer_id: 'L1',
        vector_paths: [],
        raster_regions: [makeRasterPreview({
          speed_mm_min: 600,
          fill_density: 1.0,
          end_point: { x: 0, y: 10 },
          line_count: 5,
        })],
      }],
    });

    const tl = buildTimeline(data, ['#ff0000']);
    const seg = segmentsOfType(tl.segments, 'raster')[0];
    if (seg.type === 'raster') {
      expect(seg.endPoint).toEqual({ x: 0, y: 10 });
    }
  });

  it('does not prefer run preview for image raster regions on fill-like layer families', () => {
    const data = makePreviewData({
      layers: [{
        layer_id: 'L1',
        vector_paths: [],
        raster_regions: [makeRasterPreview({
          outlines: [],
        })],
      }],
    });

    const tl = buildTimeline(data, ['#ff0000'], undefined, false, { L1: 'fill' });
    const seg = segmentsOfType(tl.segments, 'raster')[0];
    if (seg.type === 'raster') {
      expect(seg.preferRunPreview).toBe(false);
    }
  });

  it('prefers run preview only for outlined fill raster regions', () => {
    const data = makePreviewData({
      layers: [{
        layer_id: 'L1',
        vector_paths: [],
        raster_regions: [makeRasterPreview({
          outlines: [{
            closed: true,
            points: [
              { x: 0, y: 0 },
              { x: 10, y: 0 },
              { x: 10, y: 10 },
              { x: 0, y: 10 },
            ],
          }],
        })],
      }],
    });

    const tl = buildTimeline(data, ['#ff0000'], undefined, false, { L1: 'fill' });
    const seg = segmentsOfType(tl.segments, 'raster')[0];
    if (seg.type === 'raster') {
      expect(seg.preferRunPreview).toBe(true);
    }
  });

  it('endPoint is undefined when not provided', () => {
    const data = makePreviewData({
      layers: [{
        layer_id: 'L1',
        vector_paths: [],
        raster_regions: [makeRasterPreview({
          speed_mm_min: 600,
          fill_density: 1.0,
          line_count: 5,
        })],
      }],
    });

    const tl = buildTimeline(data, ['#ff0000']);
    const seg = segmentsOfType(tl.segments, 'raster')[0];
    if (seg.type === 'raster') {
      expect(seg.endPoint).toBeUndefined();
    }
  });

  it('carries scan_angle_deg and scan_origin through to RasterSegment', () => {
    const data = makePreviewData({
      layers: [{
        layer_id: 'L1',
        vector_paths: [],
        raster_regions: [makeRasterPreview({
          bounds: { min: { x: 0, y: 0 }, max: { x: 20, y: 20 } },
          speed_mm_min: 1200,
          scan_angle_deg: 45,
          scan_origin: { x: 10, y: 10 },
        })],
      }],
    });

    const tl = buildTimeline(data, ['#ff0000']);
    const seg = segmentsOfType(tl.segments, 'raster')[0];
    if (seg.type === 'raster') {
      expect(seg.scanAngleDeg).toBe(45);
      expect(seg.scanOrigin).toEqual({ x: 10, y: 10 });
    }
  });

  it('scanAngleDeg/scanOrigin carry the backend default values when omitted locally', () => {
    const data = makePreviewData({
      layers: [{
        layer_id: 'L1',
        vector_paths: [],
        raster_regions: [makeRasterPreview({
          speed_mm_min: 600,
          fill_density: 1.0,
          line_count: 5,
        })],
      }],
    });

    const tl = buildTimeline(data, ['#ff0000']);
    const seg = segmentsOfType(tl.segments, 'raster')[0];
    if (seg.type === 'raster') {
      expect(seg.scanAngleDeg).toBe(0);
      expect(seg.scanOrigin).toEqual({ x: 0, y: 0 });
    }
  });
});
