import type { Point2D, Bounds } from '../types/project';
import type { DirectionMode } from '../types/plan';
import type {
  PreviewData,
  PreviewStats,
  RasterPreviewBitmap,
  RasterRunExtent,
} from '../types/preview';

// Default rapid traverse speed for travel time estimation (mm/min).
// Must match RAPID_SPEED_MM_MIN in stats.rs so that playback timing
// agrees with the authoritative Duration stat shown in the preview window.
export const DEFAULT_RAPID_SPEED_MM_MIN = 10000;

// --- Segment types: vector, raster, travel, and frame are structurally distinct ---

export interface VectorSegment {
  type: 'vector';
  layerIndex: number;
  layerColor: string;
  points: Point2D[];
  closed: boolean;
  startTime: number;
  endTime: number;
  speedMmMin: number;
  powerPercent: number;
}

export interface RasterSegment {
  type: 'raster';
  layerIndex: number;
  layerColor: string;
  preferRunPreview?: boolean;
  bounds: Bounds;
  lineCount: number;
  lineIntervalMm: number;
  overscanMm: number;
  speedMmMin: number;
  directionMode: DirectionMode;
  startTime: number;
  endTime: number;
  outlines?: { points: Point2D[]; closed: boolean }[];
  scanAxis?: 'horizontal' | 'vertical';
  scanAngleDeg?: number;
  scanOrigin?: Point2D;
  /** Processed preview bitmap, rendered via drawImage. */
  previewBitmap?: RasterPreviewBitmap;
  /** Min corner of the preview bitmap in pre-rotation, pre-transpose local space. */
  localOriginMm?: Point2D;
  localWidthMm?: number;
  localHeightMm?: number;
  /** Per-planner-run extents, used for overscan markers and playback progress masking. */
  runExtents?: RasterRunExtent[];
  /** One full envelope per non-empty scanline, used for overscan markers. */
  scanlineExtents?: RasterRunExtent[];
  /** Finer-grained extents used for overscan visualization. */
  overscanRunExtents?: RasterRunExtent[];
  /** Backend sequence index, used as a stable key into PreviewBitmapCache. */
  sequence?: number;
  avgPowerNormalized?: number;
  endPoint?: Point2D;
}

export interface TravelSegment {
  type: 'travel';
  from: Point2D;
  to: Point2D;
  startTime: number;
  endTime: number;
}

export interface FrameSegment {
  type: 'frame';
  path: Point2D[];
  startTime: number;
  endTime: number;
  speedMmMin: number;
}

export type AnimationSegment = VectorSegment | RasterSegment | TravelSegment | FrameSegment;

export interface AnimationTimeline {
  segments: AnimationSegment[];
  /** Approximate playback duration including travel and frame. NOT authoritative — use stats for display. */
  playbackDuration: number;
  /** Backend-computed stats (authoritative for display). */
  stats: PreviewStats;
  /** Job bounds — NOT machine bed bounds. */
  jobBounds: Bounds;
}

export function polylineLength(points: Point2D[]): number {
  let len = 0;
  for (let i = 1; i < points.length; i++) {
    const dx = points[i].x - points[i - 1].x;
    const dy = points[i].y - points[i - 1].y;
    len += Math.sqrt(dx * dx + dy * dy);
  }
  return len;
}

function pointDistance(a: Point2D, b: Point2D): number {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  return Math.sqrt(dx * dx + dy * dy);
}

/**
 * Estimate raster segment duration using the same motion model as the planner.
 *
 * Feed distance = each full row envelope plus per-line overscan.
 * Internal laser-off gaps remain part of the feed sweep.
 * Accounts for scan axis orientation and overscan-expanded bounds.
 */
function estimateRasterDuration(
  region: {
    bounds: { min: Point2D; max: Point2D };
    line_count: number;
    overscan_mm: number;
    fill_density: number;
    scan_axis?: string;
  },
  speedMmMin: number,
): number {
  const isVertical = region.scan_axis === 'vertical';
  // Bounds are in un-transposed world coords; scan width is along the scan direction
  const scanWidth = isVertical
    ? region.bounds.max.y - region.bounds.min.y
    : region.bounds.max.x - region.bounds.min.x;
  const os = region.overscan_mm;
  // Bounds already include overscan expansion; burn width is the inner portion
  const burnWidth = Math.max(0, scanWidth - 2 * os);
  const n = region.line_count;
  const feedPerLine = burnWidth + 2 * os;
  return (n * feedPerLine / speedMmMin) * 60;
}

export function buildTimeline(
  data: PreviewData,
  layerColors: string[],
  rapidSpeedMmMin: number = DEFAULT_RAPID_SPEED_MM_MIN,
  skipTravelTime: boolean = false,
  layerOperations?: Record<string, string>,
): AnimationTimeline {
  const segments: AnimationSegment[] = [];
  let cursor = 0;

  // --- Frame segment (runs first, before cuts) ---
  if (data.frame && data.frame.path.length >= 2) {
    const frameLen = polylineLength(data.frame.path);
    const speed = data.frame.speed_mm_min > 0 ? data.frame.speed_mm_min : rapidSpeedMmMin;
    const duration = speed > 0 ? (frameLen / speed) * 60 : 0;

    segments.push({
      type: 'frame',
      path: data.frame.path,
      startTime: cursor,
      endTime: cursor + duration,
      speedMmMin: speed,
    });
    cursor += duration;
  }

  // --- Collect all segments with their execution sequence ---
  // The backend assigns a sequence number to each segment during distillation,
  // preserving the true execution order from the planner.
  type OrderedItem =
    | { kind: 'travel'; seq: number; data: typeof data.travel_moves[0] }
    | { kind: 'vector'; seq: number; layerIndex: number; color: string; data: typeof data.layers[0]['vector_paths'][0] }
    | { kind: 'raster'; seq: number; layerIndex: number; color: string; data: typeof data.layers[0]['raster_regions'][0] };

  const ordered: OrderedItem[] = [];

  for (const move of data.travel_moves) {
    ordered.push({ kind: 'travel', seq: move.sequence ?? 0, data: move });
  }

  for (let li = 0; li < data.layers.length; li++) {
    const layer = data.layers[li];
    const color = layerColors[li % layerColors.length] ?? '#ffffff';

    for (const vec of layer.vector_paths) {
      ordered.push({ kind: 'vector', seq: vec.sequence ?? 0, layerIndex: li, color, data: vec });
    }

    for (const raster of layer.raster_regions) {
      ordered.push({ kind: 'raster', seq: raster.sequence ?? 0, layerIndex: li, color, data: raster });
    }
  }

  // Sort by execution sequence to reconstruct true interleaved order
  ordered.sort((a, b) => a.seq - b.seq);

  // --- Build timeline segments in execution order ---
  for (const item of ordered) {
    if (item.kind === 'travel') {
      const dist = pointDistance(item.data.from, item.data.to);
      const duration = skipTravelTime ? 0 : (rapidSpeedMmMin > 0 ? (dist / rapidSpeedMmMin) * 60 : 0);
      segments.push({
        type: 'travel',
        from: item.data.from,
        to: item.data.to,
        startTime: cursor,
        endTime: cursor + duration,
      });
      cursor += duration;
    } else if (item.kind === 'vector') {
      const len = polylineLength(item.data.points);
      const speed = item.data.speed_mm_min;
      const duration = speed > 0 ? (len / speed) * 60 : 0;
      segments.push({
        type: 'vector',
        layerIndex: item.layerIndex,
        layerColor: item.color,
        points: item.data.points,
        closed: item.data.closed,
        startTime: cursor,
        endTime: cursor + duration,
        speedMmMin: speed,
        powerPercent: item.data.power_percent,
      });
      cursor += duration;
    } else {
      const speed = item.data.speed_mm_min;
      // Prefer backend-computed exact duration; fall back to estimate for old data.
      const duration = item.data.duration_secs != null && item.data.duration_secs > 0
        ? item.data.duration_secs
        : (speed > 0 ? estimateRasterDuration(item.data, speed) : 0);
      const layerOperation = layerOperations?.[data.layers[item.layerIndex]?.layer_id];
      const hasFillOutlines = !!item.data.outlines && item.data.outlines.length > 0;
      segments.push({
        type: 'raster',
        layerIndex: item.layerIndex,
        layerColor: item.color,
        preferRunPreview: hasFillOutlines && (
          layerOperation === 'fill' || layerOperation === 'offset_fill'
        ),
        bounds: item.data.bounds,
        lineCount: item.data.line_count,
        lineIntervalMm: item.data.line_interval_mm,
        overscanMm: item.data.overscan_mm ?? 0,
        speedMmMin: speed,
        directionMode: item.data.direction_mode,
        startTime: cursor,
        endTime: cursor + duration,
        outlines: item.data.outlines,
        scanAxis: item.data.scan_axis,
        scanAngleDeg: item.data.scan_angle_deg,
        scanOrigin: item.data.scan_origin,
        previewBitmap: item.data.preview_bitmap,
        localOriginMm: item.data.local_origin_mm,
        localWidthMm: item.data.local_width_mm,
        localHeightMm: item.data.local_height_mm,
        runExtents: item.data.run_extents,
        scanlineExtents: item.data.scanline_extents,
        overscanRunExtents: item.data.overscan_run_extents,
        sequence: item.data.sequence,
        avgPowerNormalized: item.data.avg_power_normalized,
        endPoint: item.data.end_point,
      });
      cursor += duration;
    }
  }

  return {
    segments,
    playbackDuration: cursor,
    stats: data.stats,
    jobBounds: data.bounds,
  };
}
