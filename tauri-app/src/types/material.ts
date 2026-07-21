import type { OperationType } from './project';

/** Material preset types for the material library — matches Rust MaterialPreset. */

export interface MaterialPreset {
  id: string;
  name: string;
  material: string;
  thickness_mm: number;
  operation: OperationType;
  speed_mm_min: number;
  power_percent: number;
  passes: number;
  dpi?: number | null;
  raster_mode?: 'grayscale' | 'threshold' | 'floyd_steinberg' | 'ordered_dither' | 'stucki' | 'jarvis' | 'sierra' | 'atkinson' | 'halftone' | 'newsprint' | 'sketch' | null;
  line_interval_mm?: number | null;
  scan_angle?: number | null;
  bidirectional?: boolean | null;
  overscan_mm?: number | null;
  flood_fill?: boolean | null;
  angle_passes?: number | null;
  angle_increment_deg?: number | null;
  pass_through?: boolean | null;
  halftone_cells_per_inch?: number | null;
  halftone_angle_deg?: number | null;
  newsprint_angle_deg?: number | null;
  newsprint_frequency?: number | null;
  /** Layer-level invert (negative image). Image-only. */
  invert?: boolean | null;
  /** Layer-level dot-width correction in mm. Image-only. */
  dot_width_correction_mm?: number | null;
  /** Ramp length in mm. Image-only. */
  ramp_length_mm?: number | null;
  // backend `MaterialPreset.notes`/`category` are `String` with
  // `#[serde(default)]` — they always round-trip as present strings (possibly
  // empty), never absent. Frontend mirror must match.
  notes: string;
  category: string;
  device_profile?: string | null;
}
