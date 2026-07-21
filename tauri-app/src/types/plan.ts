// --- Execution Plan types (mirrors beambench-planner/src/plan.rs) ---

import type { Bounds, Point2D } from './project';

// --- Scanline types ---

export type ScanDirection = 'left_to_right' | 'right_to_left';
export type DirectionMode = 'bidirectional' | 'unidirectional';
export type PowerMode = 'binary' | 'grayscale';
export type ScanAxis = 'horizontal' | 'vertical';
export interface PlanPolyline {
  points: Point2D[];
  closed: boolean;
}

export interface ScanRun {
  start_x_mm: number;
  end_x_mm: number;
  power_values: number[];
}

export interface Scanline {
  y_mm: number;
  runs: ScanRun[];
  direction: ScanDirection;
}

// --- Segment types (tagged union via "type" field) ---

export interface TravelSegment {
  type: 'travel';
  start: Point2D;
  end: Point2D;
}

export interface VectorSegment {
  type: 'vector';
  cut_entry_id: string;
  polyline: Point2D[];
  closed: boolean;
  power_percent: number;
  speed_mm_min: number;
  layer_id: string;
  perforation_enabled: boolean;
  perforation_on_ms: number;
  perforation_off_ms: number;
  source_object_id?: string;
  source_subpath_index?: number;
}

export interface RasterSegment {
  type: 'raster';
  cut_entry_id: string;
  scanlines: Scanline[];
  line_interval_mm: number;
  direction_mode: DirectionMode;
  power_mode: PowerMode;
  speed_mm_min: number;
  layer_id: string;
  scan_angle_deg: number;
  scan_origin: Point2D;
  overscan_mm: number;
  outlines: PlanPolyline[];
  scan_axis: ScanAxis;
  power_max_percent: number;
  power_min_percent: number;
  dot_width_correction_mm: number;
  ramp_length_mm: number;
  x_pixel_mm: number;
}

export interface FrameSegment {
  type: 'frame';
  path: Point2D[];
  power_percent: number;
  speed_mm_min: number;
}

export interface OffsetFillSegment {
  type: 'offset_fill';
  layer_id: string;
  object_id: string;
  offset_mm: number;
  angle_deg: number;
}

export type PlanSegment =
  | TravelSegment
  | VectorSegment
  | RasterSegment
  | FrameSegment
  | OffsetFillSegment;

// --- Warning ---

export interface PlanWarning {
  message: string;
}

export type PlanEntryFailureReason =
  | {
      type: 'image_mask_skipped';
      object_id: string;
      message: string;
    }
  | {
      type: 'offset_fill_timed_out';
      iterations: number;
      elapsed_ms: number;
    }
  | {
      type: 'offset_fill_emergency_ceiling';
      iterations: number;
    };

export interface PlanEntryFailure {
  layer_id: string;
  cut_entry_id?: string | null;
  operation: 'image' | 'line' | 'fill' | 'score' | 'cut' | 'offset_fill';
  reason: PlanEntryFailureReason;
}

// --- Statistics ---

export interface PlanStats {
  distance_mm: number;
  duration_secs: number;
  segment_count: number;
  warning_count: number;
}

// --- Execution Plan ---

export interface ExecutionPlan {
  id: string;
  project_id: string;
  revision_hash: string;
  created_at: string;
  bounds: Bounds;
  total_distance_mm: number;
  estimated_duration_secs: number;
  segments: PlanSegment[];
  layer_order: string[];
  warnings: PlanWarning[];
  failed_entries: PlanEntryFailure[];
}
