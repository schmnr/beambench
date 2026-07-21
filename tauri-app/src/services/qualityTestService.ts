import { invoke } from '@tauri-apps/api/core';
import { open, save } from '@tauri-apps/plugin-dialog';

import i18n from '../i18n';
import type { PreviewData } from '../types/preview';
import type { CutEntry, Project } from '../types/project';
import type {
  MaterialTestRecipe,
  QualityTestError,
  QualityTestRequest,
  QualityTestWarning,
} from '../types/machine';
import type { JobProgress } from '../types/machine';
import type { MaterialApplyWarning } from './materialService';

export interface QualityTestPreviewResponse {
  preview: PreviewData;
  warnings: QualityTestWarning[];
}

export interface QualityTestExportResponse {
  path: string;
  warnings: QualityTestWarning[];
}

export interface QualityTestCanvasResponse {
  project: Project;
  warnings: QualityTestWarning[];
  createdObjectIds: string[];
  createdLayerIds: string[];
}

/**
 * IPC wrappers for the M3 quality-test commands.
 *
 * All four commands route through a transient pipeline that never touches the active project,
 * undo stack, or shared plan cache. `frame` and `start` hard-reject on bounds / Z support
 * violations; `preview` and `exportGcode` succeed with warnings attached.
 */
export const qualityTestService = {
  async preview(request: QualityTestRequest): Promise<QualityTestPreviewResponse> {
    return invoke<QualityTestPreviewResponse>('quality_test_preview', { request });
  },

  async exportGcode(request: QualityTestRequest): Promise<QualityTestExportResponse | null> {
    const selected = await save({
      title: i18n.t('file_dialogs.save_quality_test_gcode_title'),
      defaultPath: 'quality-test.gcode',
      filters: [{ name: i18n.t('file_dialogs.filter_gcode'), extensions: ['gcode', 'nc', 'ngc'] }],
    });
    if (selected === null) return null;
    return invoke<QualityTestExportResponse>('quality_test_export_gcode', {
      request,
      path: selected,
    });
  },

  async frame(request: QualityTestRequest): Promise<JobProgress> {
    return invoke<JobProgress>('quality_test_frame', { request });
  },

  async start(request: QualityTestRequest): Promise<JobProgress> {
    return invoke<JobProgress>('quality_test_start', { request });
  },

  async createMaterialOnCanvas(request: QualityTestRequest): Promise<QualityTestCanvasResponse> {
    return invoke<QualityTestCanvasResponse>('quality_test_create_material_on_canvas', { request });
  },

  async exportRecipes(recipes: MaterialTestRecipe[]): Promise<string | null> {
    const selected = await save({
      title: i18n.t('file_dialogs.export_material_test_title'),
      defaultPath: 'material-test-recipes.json',
      filters: [{ name: i18n.t('file_dialogs.filter_material_test_recipes'), extensions: ['json'] }],
    });
    if (selected === null) return null;
    await invoke<void>('export_material_test_recipes', {
      path: selected,
      recipes,
    });
    return selected;
  },

  async importRecipes(): Promise<MaterialTestRecipe[] | null> {
    const selected = await open({
      title: i18n.t('file_dialogs.import_material_test_title'),
      filters: [{ name: i18n.t('file_dialogs.filter_material_test_recipes'), extensions: ['json'] }],
      multiple: false,
    });
    if (selected === null || Array.isArray(selected)) return null;
    return invoke<MaterialTestRecipe[]>('import_material_test_recipes', { path: selected });
  },

  /**
   * Apply a material preset to a caller-supplied seed entry without touching project state.
   * Returns the updated entry plus any warnings. Used by the M3 dialogs to derive dialog defaults
   * (speed range, power range, line interval) from a preset without committing changes to the
   * active project's layer.
   */
  async applyMaterialPresetToSeed(
    presetId: string,
    seed: CutEntry,
  ): Promise<{ entry: CutEntry; warnings: MaterialApplyWarning[] }> {
    // Backend returns Rust tuple `(CutEntry, Vec<MaterialApplyWarning>)` → JSON array.
    const result = await invoke<[CutEntry, MaterialApplyWarning[]]>(
      'apply_material_preset_to_seed',
      { presetId, seed },
    );
    return { entry: result[0], warnings: result[1] };
  },
};

/**
 * Build a default `CutEntry` for use as the seed input to `applyMaterialPresetToSeed`.
 *
 * Mirrors Rust `CutEntry::new`: image/fill operations get a populated `raster_settings` (with
 * sensible defaults), all other operations get `vector_settings`. The backend's pure helper only
 * mutates whichever bag is `Some(...)`, so a stub seed with `raster_settings: null` for a fill
 * operation would silently drop the preset's interval/DPI/scan-angle fields. Real entry IDs are
 * backend-allocated; the stub uses a fresh UUID so the Rust `CutEntryId` deserializer accepts it.
 */
export function makeSeedCutEntry(
  operation: 'image' | 'line' | 'fill' | 'cut' | 'score' | 'offset_fill',
): CutEntry {
  const usesRaster = operation === 'image' || operation === 'fill' || operation === 'offset_fill';
  const usesVector =
    operation === 'line' ||
    operation === 'cut' ||
    operation === 'score' ||
    operation === 'offset_fill';
  return {
    id: crypto.randomUUID(),
    operation,
    speed_mm_min: 1000,
    power_percent: 50,
    raster_settings: usesRaster ? defaultRasterSettings() : null,
    vector_settings: usesVector ? defaultVectorSettings() : null,
    air_assist: false,
    power_min_percent: 0,
    z_offset_mm: 0,
    gcode_prefix: '',
    gcode_suffix: '',
    output_enabled: true,
  };
}

/** Mirrors Rust `RasterSettings::default()` so apply_material_to_entry has a bag to mutate. */
function defaultRasterSettings(): NonNullable<CutEntry['raster_settings']> {
  return {
    dpi: 254,
    mode: 'floyd_steinberg',
    scan_angle: 0,
    bidirectional: true,
    overscan_mm: 2.5,
    passes: 1,
    line_interval_mm: 0.1,
    crosshatch: false,
    flood_fill: false,
    angle_passes: 1,
    angle_increment_deg: 90,
    pass_through: false,
    halftone_cells_per_inch: 10,
    halftone_angle_deg: 0,
    newsprint_angle_deg: 45,
    newsprint_frequency: 10,
    invert: false,
    dot_width_correction_mm: 0,
    ramp_length_mm: 0,
  };
}

function defaultVectorSettings(): NonNullable<CutEntry['vector_settings']> {
  return {
    passes: 1,
    perforation_enabled: false,
    perforation_on_ms: 10,
    perforation_off_ms: 10,
    kerf_offset_mm: 0,
    tab_count: 0,
    tab_width_mm: 3,
    offset_overlap_mm: 0,
    offset_outward: false,
    offset_fill_grouping_mode: 'all_shapes_at_once',
  };
}

/** Format a `QualityTestWarning` for display in the dialog warning banner. */
export function formatQualityTestWarning(w: QualityTestWarning): string {
  switch (w.kind) {
    case 'bounds_exceeded':
      return `Generated geometry (${w.bbox_w_mm.toFixed(1)}×${w.bbox_h_mm.toFixed(1)} mm) exceeds bed (${w.bed_w_mm.toFixed(1)}×${w.bed_h_mm.toFixed(1)} mm).`;
    case 'font_fallback':
      return `Font "${w.requested_family}" not found — using bundled fallback for labels.`;
    default:
      return 'Unknown quality-test warning.';
  }
}

function stringifyUnknownError(e: unknown): string {
  if (typeof e === 'string') return e;
  if (e instanceof Error) return e.message;
  try {
    return JSON.stringify(e);
  } catch {
    return String(e);
  }
}

/** Format a `QualityTestError` for display in the dialog. */
export function formatQualityTestError(e: QualityTestError | string | unknown): string {
  if (typeof e === 'string') return e;
  if (e === null || typeof e !== 'object' || !('kind' in e)) {
    return stringifyUnknownError(e);
  }
  const err = e as QualityTestError;
  switch (err.kind) {
    case 'bounds_exceeded':
      return `Cannot run: generated geometry (${err.bbox_w_mm.toFixed(1)}×${err.bbox_h_mm.toFixed(1)} mm) exceeds bed (${err.bed_w_mm.toFixed(1)}×${err.bed_h_mm.toFixed(1)} mm).`;
    case 'z_support_required':
      return 'Active machine profile does not advertise Z support. Enable "Supports Z moves" in the profile settings.';
    case 'material_height_required':
      return 'Absolute Z mode requires a material height. Set a material thickness on the project before framing or starting.';
    case 'unsupported_z_backend':
      return 'Focus Test Z output is only supported for GRBL sessions.';
    case 'no_active_machine_profile':
      return 'No active machine profile selected.';
    case 'job_in_progress':
      return 'A job is already active. Cancel it or wait for it to finish before starting another quality test.';
    case 'internal': {
      const detail = err.message;
      return detail ? `Internal error: ${detail}` : 'Internal error while running quality test.';
    }
    default:
      return stringifyUnknownError(e);
  }
}
