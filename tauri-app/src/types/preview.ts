// --- Preview types (mirrors beambench-preview/src/types.rs) ---

import type { Bounds, Point2D } from './project';
import type { DirectionMode, PowerMode, ScanAxis } from './plan';

export interface PreviewData {
  plan_id: string;
  revision_hash: string;
  bounds: Bounds;
  layers: PreviewLayer[];
  travel_moves: TravelMove[];
  frame: PreviewFrame | null;
  stats: PreviewStats;
  warnings: string[];
  failed_entries: string[];
}

export interface PreviewLayer {
  layer_id: string;
  vector_paths: VectorPreview[];
  raster_regions: RasterPreview[];
}

export interface VectorPreview {
  points: Point2D[];
  closed: boolean;
  power_percent: number;
  speed_mm_min: number;
  sequence: number;
}

export interface FillOutline {
  points: Point2D[];
  closed: boolean;
}

/** PNG-encoded grayscale preview bitmap. `png_bytes` is a number[] because
 *  Rust `Vec<u8>` serializes to a JSON array across the Tauri boundary
 *  (matching the existing pattern in persistenceService.ts and
 *  projectStore.ts); the PreviewBitmapCache converts it to Uint8Array
 *  internally before constructing the Blob. */
export interface RasterPreviewBitmap {
  width_px: number;
  height_px: number;
  png_bytes: number[] | Uint8Array;
}

/** One planner run (not per-sub-run). Used by overscan marker rendering
 *  and by animated-preview playback for per-run progress masking. */
export interface RasterRunExtent {
  y_mm: number;
  start_x_mm: number;
  end_x_mm: number;
  /** "left_to_right" or "right_to_left" — matches Rust ScanDirection. */
  direction: 'left_to_right' | 'right_to_left';
}

export interface RasterPreview {
  bounds: Bounds;
  line_count: number;
  line_interval_mm: number;
  direction_mode: DirectionMode;
  power_mode: PowerMode;
  speed_mm_min: number;
  fill_density: number;
  scan_angle_deg: number;
  scan_origin: Point2D;
  overscan_mm: number;
  outlines: FillOutline[];
  scan_axis: ScanAxis;
  sequence: number;
  /** Backend-computed exact duration in seconds. */
  duration_secs: number;
  avg_power_normalized: number;
  /** Physical head position after the last run completes (world-space). */
  end_point?: Point2D;
  /** Processed-raster preview bitmap, rendered via drawImage. */
  preview_bitmap?: RasterPreviewBitmap;
  /** Min corner of the bitmap in pre-rotation, pre-transpose local space. */
  local_origin_mm: Point2D;
  /** Width of the bitmap in local-space mm. */
  local_width_mm: number;
  /** Height of the bitmap in local-space mm. */
  local_height_mm: number;
  /** Per-planner-run burn extents for playback progress. */
  run_extents: RasterRunExtent[];
  /** One full envelope per non-empty scanline, used for overscan markers. */
  scanline_extents?: RasterRunExtent[];
  /** Finer-grained burn extents, currently retained for future use and
   *  as a fallback when row-level run extents are unavailable. */
  overscan_run_extents: RasterRunExtent[];
}

export interface TravelMove {
  from: Point2D;
  to: Point2D;
  sequence: number;
}

export interface PreviewFrame {
  path: Point2D[];
  power_percent: number;
  speed_mm_min: number;
}

export interface PreviewStats {
  total_distance_mm: number;
  travel_distance_mm: number;
  burn_distance_mm: number;
  estimated_duration_secs: number;
  segment_count: number;
  raster_line_count: number;
}
