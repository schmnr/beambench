// --- Geometry (self-contained for frontend use) ---

export interface Point2D {
  x: number;
  y: number;
}

export interface Bounds {
  min: Point2D;
  max: Point2D;
}

export interface Transform2D {
  a: number;
  b: number;
  c: number;
  d: number;
  tx: number;
  ty: number;
}

// --- Layer types ---

export type OperationType = 'image' | 'line' | 'fill' | 'offset_fill' | 'score' | 'cut' | 'tool';
export type AlignmentType = 'left' | 'right' | 'top' | 'bottom' | 'centers_xy' | 'centers_v' | 'centers_h';
export type DistributionDirection = 'v_spaced' | 'v_centered' | 'h_spaced' | 'h_centered';
export type FlipAxis = 'horizontal' | 'vertical';
export type MoveTogetherAxis = 'horizontal' | 'vertical';
export type SameSizeAxis = 'width' | 'height';
export type DockDirection = 'left' | 'right' | 'up' | 'down';
export interface DockOptions {
  moveAsGroup: boolean;
  lockInnerObjects: boolean;
  paddingMm: number;
}
export interface NestOptions {
  paddingMm: number;
  allowRotation: boolean;
  allowMirror: boolean;
  lockInnerObjects: boolean;
  timeLimitMs: number;
  rotationStepDeg: number;
}
export interface NestResult {
  targetContainerId: string;
  placedObjectIds: string[];
  unplacedObjectIds: string[];
  utilization: number;
  elapsedMs: number;
}
export interface NestError {
  code:
    | 'no_container'
    | 'no_parts'
    | 'unplaced'
    | 'unsupported_geometry'
    | 'cap_exceeded'
    | 'timeout'
    | 'cancelled'
    | 'engine_error';
  message: string;
  unplacedObjectIds?: string[];
}
export interface ResizeSlotsOptions {
  currentThicknessMm: number;
  newThicknessMm: number;
  toleranceMm: number;
  adjustSlotDepth?: boolean;
  adjustSlotWidth?: boolean;
  adjustTabHeight?: boolean;
}
export type BarcodeType =
  | 'code128'
  | 'code39'
  | 'code93'
  | 'codabar'
  | 'standard_2_of_5'
  | 'ean13'
  | 'ean8'
  | 'upc_a'
  | 'qr_code'
  | 'data_matrix'
  | 'pdf417';
export type QrErrorCorrection = 'low' | 'medium' | 'quartile' | 'high';

export interface BarcodeOptions {
  show_text?: boolean;
  qr_error_correction?: QrErrorCorrection;
  data_matrix_force_square?: boolean;
}

export type RasterMode =
  | 'grayscale'
  | 'threshold'
  | 'floyd_steinberg'
  | 'ordered_dither'
  | 'stucki'
  | 'jarvis'
  | 'sierra'
  | 'atkinson'
  | 'halftone'
  | 'newsprint'
  | 'sketch';

export type OffsetFillGroupingMode =
  | 'all_shapes_at_once'
  | 'groups_together'
  | 'shapes_individually';

export interface RasterSettings {
  dpi: number;
  mode: RasterMode;
  scan_angle: number;
  bidirectional: boolean;
  overscan_mm: number;
  passes: number;
  line_interval_mm: number;
  crosshatch: boolean;
  flood_fill: boolean;
  angle_passes: number;
  angle_increment_deg: number;
  pass_through: boolean;
  halftone_cells_per_inch: number;
  halftone_angle_deg: number;
  newsprint_angle_deg: number;
  newsprint_frequency: number;
  /** Layer-level invert (negative image). Applied after per-object adjustments and before dithering. */
  invert: boolean;
  /** Layer-level dot-width correction in mm. Trims both ends of threshold runs. */
  dot_width_correction_mm: number;
  /** Ramp length in mm. Applied by emitter/preview as power ramp-in/ramp-out. */
  ramp_length_mm: number;
}

export interface VectorSettings {
  passes: number;
  perforation_enabled: boolean;
  perforation_on_ms: number;
  perforation_off_ms: number;
  kerf_offset_mm?: number;
  tab_count?: number;
  tab_width_mm?: number;
  offset_overlap_mm?: number;
  offset_outward?: boolean;
  offset_fill_grouping_mode?: OffsetFillGroupingMode;
}

/**
 * M4: clipboard payload shape for `paste_layer_entries` — mirrors `CutEntry` minus `id`.
 *
 * The backend mints fresh `CutEntryId`s for each entry on every paste, so a single clipboard
 * can be pasted onto N target layers without aliasing.
 */
export interface CutEntryTemplate {
  operation: OperationType;
  speed_mm_min: number;
  power_percent: number;
  raster_settings: RasterSettings | null;
  vector_settings: VectorSettings | null;
  air_assist: boolean;
  power_min_percent: number;
  z_offset_mm: number;
  gcode_prefix: string;
  gcode_suffix: string;
  output_enabled: boolean;
}

/**
 * M4: batch toggle mode for layer enabled/visible operations.
 * `OnlyThisOn` powers the row-menu "Disable/Hide all but this one" actions.
 */
export type LayerBatchToggle =
  | { kind: 'all_on' }
  | { kind: 'all_off' }
  | { kind: 'invert' }
  | { kind: 'only_this_on'; keep: string };

export interface CutEntry {
  id: string;
  operation: OperationType;
  speed_mm_min: number;
  power_percent: number;
  raster_settings: RasterSettings | null;
  vector_settings: VectorSettings | null;
  air_assist: boolean;
  power_min_percent: number;
  z_offset_mm: number;
  gcode_prefix: string;
  gcode_suffix: string;
  output_enabled: boolean;
}

export interface CutEntryPatch {
  id?: string;
  operation?: OperationType;
  speed_mm_min?: number;
  power_percent?: number;
  raster_settings?: RasterSettings | null;
  vector_settings?: VectorSettings | null;
  air_assist?: boolean;
  power_min_percent?: number;
  z_offset_mm?: number;
  gcode_prefix?: string;
  gcode_suffix?: string;
  output_enabled?: boolean;
}

export interface Layer {
  id: string;
  name: string;
  entries: CutEntry[];
  enabled: boolean;
  order_index: number;
  color_tag: string;
  visible: boolean;
  is_tool_layer: boolean;
}

export interface LayerPatch {
  name?: string;
  enabled?: boolean;
  visible?: boolean;
  color_tag?: string;
}

// --- Raster adjustments ---

// backend `RasterAdjustments` marks all fields non-optional with
// `#[serde(default)]` so they're always present in the serialized payload.
// The frontend mirror must match — do NOT mark fields optional or consumers
// will inject holes the backend never produces.
export interface RasterAdjustments {
  brightness: number;
  contrast: number;
  gamma: number;
  invert: boolean;
  threshold: number;
  saturation: number;
  sharpen: number;
  edge_enhance: boolean;
  enhance_radius: number;
  enhance_amount: number;
  enhance_denoise: number;
}

export interface GcodeLine {
  line_number: number;
  raw: string;
  command: string | null;
  params: Record<string, number>;
}

// --- Tab anchors ---

export interface TabAnchor {
  subpath_index: number;
  position: number;
}

// --- Variable text ---

export interface VariableTextSource {
  csvPath: string | null;
  csvData: string[][];
  fieldDefaults: Record<string, string>;
  current?: number;
  currentRow?: number;
  start?: number;
  end?: number;
  advanceBy?: number;
  autoAdvance?: boolean;
  totalCopies: number;
}

export type VariableTextMode =
  | 'normal'
  | 'serial_number'
  | 'date_time'
  | 'merge_csv'
  | 'cut_setting';

export interface VariableTextConfig {
  template: string;
  mode?: VariableTextMode | null;
  offset?: number | null;
  source: VariableTextSource;
}

// --- Array result ---

export interface ArrayResult {
  createdIds: string[];
  groupId: string | null;
}

// --- Object types ---

export type ShapeKind = 'rectangle' | 'ellipse';
export type TextAlignment = 'left' | 'center' | 'right';
export type TextAlignmentV = 'top' | 'middle' | 'bottom';
export type TextLayoutMode = 'straight' | 'bend' | 'path';
export type TextTransformStyle = 'none' | 'arch' | 'rise' | 'wave' | 'flag' | 'angle' | 'circle';
export type TextCirclePlacement = 'top_outside' | 'top_inside' | 'bottom_outside' | 'bottom_inside';
export type TextFontSource = 'system' | 'shx' | 'bundled_fallback';
export type GuideAxis = 'horizontal' | 'vertical';
export type ImageMaskPolarity = 'keep_inside' | 'keep_outside';

export interface ImageMaskRef {
  object_id: string;
  polarity: ImageMaskPolarity;
}

export type ObjectData =
  | { type: 'raster_image'; asset_key: string; original_width_px: number; original_height_px: number; adjustments?: RasterAdjustments; masks?: ImageMaskRef[] }
  | { type: 'vector_path'; path_data: string; closed: boolean; ruler_guide_axis?: GuideAxis | null }
  | { type: 'shape'; kind: ShapeKind; width: number; height: number; corner_radius: number }
  | { type: 'star'; points: number; bulge: number; ratio: number; dual_radius: boolean; ratio2: number | null; corner_radius: number; corner_radii: number[] }
  | { type: 'text'; content: string; font_family: string; font_size_mm: number; alignment: TextAlignment; alignment_v: TextAlignmentV; bold: boolean; italic: boolean; upper_case: boolean; welded: boolean; h_spacing: number; v_spacing: number; on_path: boolean; path_offset: number; distort: boolean; layout_mode: TextLayoutMode; rtl: boolean; bend_radius: number; transform_style: TextTransformStyle; transform_curve: number; circle_placement: TextCirclePlacement; max_width?: number | null; squeeze: boolean; ignore_empty_vars: boolean; resolved_font_source?: TextFontSource | null; resolved_font_key?: string | null; resolved_path_data?: string | null; missing_font: boolean; missing_glyphs?: string[]; guide_path_id?: string | null; variable_text?: VariableTextConfig }
  | { type: 'polygon'; sides: number; radius: number }
  | { type: 'barcode'; barcode_type: BarcodeType; data: string; width: number; height: number; options?: BarcodeOptions }
  | { type: 'group'; children: string[] }
  | { type: 'virtual_clone'; source_id: string };

export interface StartPointEdit {
  subpathIndex: number;
  originalStartCurrentIdx: number;
  reversed: boolean;
  vDisplay: number;
  normalized: boolean;
}

export interface ProjectObject {
  id: string;
  name: string;
  visible: boolean;
  locked: boolean;
  transform: Transform2D;
  bounds: Bounds;
  layer_id: string;
  z_index: number;
  data: ObjectData;
  lock_aspect_ratio: boolean;
  power_scale: number;
  priority: number;
  created_at: string;
  tabs?: TabAnchor[];
  start_point_edits?: StartPointEdit[];
}

// --- Workspace ---

export type WorkspaceOrigin = 'top_left' | 'bottom_left';

export interface Workspace {
  bed_width_mm: number;
  bed_height_mm: number;
  origin: WorkspaceOrigin;
}

// --- Asset types ---

export type AssetMediaType = 'png' | 'jpeg' | 'svg' | 'bmp' | 'gif' | 'tiff' | 'webp' | 'tga' | 'dxf' | 'ai' | 'pdf' | 'eps';

export interface Asset {
  id: string;
  original_filename: string;
  media_type: AssetMediaType;
  byte_size: number;
  width_px: number | null;
  height_px: number | null;
  source_path?: string | null;
}

// --- Job & transform types ---

export type StartFromMode = 'absolute_coords' | 'user_origin' | 'current_position';
export type AnchorPoint = 'top_left' | 'top_center' | 'top_right' | 'center_left' | 'center' | 'center_right' | 'bottom_left' | 'bottom_center' | 'bottom_right';
// backend `TransformLocks` has all fields non-optional `bool` with
// a Default that sets every lock enabled. The frontend mirror must match.
export interface TransformLocks {
  move_enabled: boolean;
  size_enabled: boolean;
  rotate_enabled: boolean;
  shear_enabled: boolean;
}

export interface CutSettings {
  operation: OperationType;
  speed_mm_min: number;
  power_percent: number;
  power_min_percent?: number;
  passes: number;
  air_assist?: boolean;
  z_offset_mm?: number;
}

export type FinishPosition = 'origin' | 'dont_move' | 'custom_xy';

export type DirectionOrder =
  | 'none'
  | 'top_down'
  | 'bottom_up'
  | 'left_right'
  | 'right_left';

// Stable sort criteria used by the optimization ordering pass. These are
// criteria inside one ordering pass, not separate optimizer passes.
export type OptimizationOrderKey = 'layer' | 'group' | 'priority';

// Mirrors the Rust `beambench_core::ProjectOptimization` struct.
// Every field travels with the project file (on disk, via
// `Project.optimization`). Runtime-only state (`current_position`) lives
// on a separate `OptimizationRuntime` overlay server-side and never
// crosses this interface.
export interface ProjectOptimization {
  enabled: boolean;
  ordering: OptimizationOrderKey[];

  // Per-shape ordering
  inner_first: boolean;
  direction_order: DirectionOrder;

  // Travel
  reduce_travel: boolean;
  hide_backlash: boolean;
  reduce_direction_changes: boolean;

  // Per-cut start point
  choose_best_start: boolean;
  choose_corners: boolean;
  choose_best_direction: boolean;

  // Cleanup
  remove_overlapping: boolean;
  remove_overlap_tolerance_mm: number;

  // Output positioning
  start_point_x: number | null;
  start_point_y: number | null;
  finish_position: FinishPosition;
  finish_x: number | null;
  finish_y: number | null;
}

/// Patch shape for `project::set_optimization`. Every field is optional;
/// only fields present in the patch cross IPC. Matches the Rust
/// `beambench_core::ProjectOptimizationPatch` wire contract and
/// `#[serde(skip_serializing_if = "Option::is_none")]` rule — callers
/// should drop `undefined` keys before serializing.
///
/// Nullable fields use `FieldValue | null` at the optional level rather
/// than the outer-Some / inner-Option nesting Rust uses, because an
/// explicit `null` across JSON round-trips is the natural way to
/// express "set this nullable field to null" from TS.
export interface ProjectOptimizationPatch {
  enabled?: boolean;
  ordering?: OptimizationOrderKey[];
  inner_first?: boolean;
  direction_order?: DirectionOrder;
  reduce_travel?: boolean;
  hide_backlash?: boolean;
  reduce_direction_changes?: boolean;
  choose_best_start?: boolean;
  choose_corners?: boolean;
  choose_best_direction?: boolean;
  remove_overlapping?: boolean;
  remove_overlap_tolerance_mm?: number;
  start_point_x?: number | null;
  start_point_y?: number | null;
  finish_position?: FinishPosition;
  finish_x?: number | null;
  finish_y?: number | null;
}

/// Server-provided default that matches `ProjectOptimization::default()`
/// in beambench-core. Used when constructing a baseline for store
/// initialization before a project has been loaded.
export const DEFAULT_PROJECT_OPTIMIZATION: ProjectOptimization = {
  enabled: true,
  ordering: ['layer', 'priority'],
  inner_first: false,
  direction_order: 'none',
  reduce_travel: false,
  hide_backlash: false,
  reduce_direction_changes: false,
  choose_best_start: false,
  choose_corners: false,
  choose_best_direction: false,
  remove_overlapping: false,
  remove_overlap_tolerance_mm: 0.05,
  start_point_x: null,
  start_point_y: null,
  finish_position: 'origin',
  finish_x: null,
  finish_y: null,
};

// --- Machine Profile Snapshot ---

export interface MachineProfileSnapshot {
  profile_id: string;
  profile_name: string;
  bed_width_mm: number;
  bed_height_mm: number;
  max_speed_mm_min: number;
}

// --- Project ---

export interface ProjectMetadata {
  format_version: string;
  app_version: string;
  project_id: string;
  project_name: string;
  created_at: string;
  modified_at: string;
}

export interface Project {
  metadata: ProjectMetadata;
  workspace: Workspace;
  layers: Layer[];
  objects: ProjectObject[];
  assets: Asset[];
  machine_profile_id: string | null;
  machine_profile_snapshot: MachineProfileSnapshot | null;
  dirty?: boolean;
  notes: string;
  start_from: StartFromMode;
  job_origin: AnchorPoint;
  user_origin: [number, number] | null;
  transform_locks: TransformLocks;
  /// Persisted optimization settings. Mirrors the Rust
  /// `Project.optimization` field.
  optimization: ProjectOptimization;
  /// M3: material thickness in mm. Used by Focus Test in `AbsoluteWorkCoord` mode as the Z
  /// reference. Optional — Focus Test live actions are gated when this is `null` and absolute
  /// mode is selected.
  material_height_mm?: number | null;
}
