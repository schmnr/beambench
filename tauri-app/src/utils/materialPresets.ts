import type { MaterialPreset } from '../types/material';
import type { CutEntry, CutEntryTemplate } from '../types/project';
import { defaultRasterSettings, defaultVectorSettings } from '../types/cutEntryDefaults';
import { effectiveDpi, effectiveLineIntervalMm } from '../types/rasterSettings';

/** Keep the legacy summary alongside an ordered recipe without storing entry IDs. */
export function withMaterialEntries(preset: MaterialPreset, entries: Array<CutEntry | CutEntryTemplate>): MaterialPreset {
  const templates = entries.map((entry) => {
    const clone = structuredClone(entry);
    if ('id' in clone) delete (clone as Partial<CutEntry>).id;
    return clone as CutEntryTemplate;
  });
  const first = templates[0];
  if (!first) return { ...preset, entries: [] };
  const rs = first.raster_settings;
  const image = first.operation === 'image';
  return {
    ...preset,
    entries: templates,
    operation: first.operation,
    speed_mm_min: first.speed_mm_min,
    power_percent: first.power_percent,
    passes: first.vector_settings?.passes ?? rs?.passes ?? 1,
    dpi: image ? effectiveDpi(rs) : (rs?.dpi ?? null),
    raster_mode: image ? (rs?.mode ?? null) : null,
    line_interval_mm: image ? effectiveLineIntervalMm(rs) : (rs?.line_interval_mm ?? null),
    scan_angle: rs?.scan_angle ?? null,
    bidirectional: rs?.bidirectional ?? null,
    overscan_mm: rs?.overscan_mm ?? null,
    flood_fill: rs?.flood_fill ?? null,
    angle_passes: rs?.angle_passes ?? null,
    angle_increment_deg: rs?.angle_increment_deg ?? null,
    pass_through: image ? (rs?.pass_through ?? null) : null,
    halftone_cells_per_inch: image ? (rs?.halftone_cells_per_inch ?? null) : null,
    halftone_angle_deg: image ? (rs?.halftone_angle_deg ?? null) : null,
    newsprint_angle_deg: image ? (rs?.newsprint_angle_deg ?? null) : null,
    newsprint_frequency: image ? (rs?.newsprint_frequency ?? null) : null,
    invert: image ? (rs?.invert ?? null) : null,
    dot_width_correction_mm: image ? (rs?.dot_width_correction_mm ?? null) : null,
    ramp_length_mm: image ? (rs?.ramp_length_mm ?? null) : null,
  };
}

/** Basic library edits target the first entry, retaining advanced fields and the rest of the stack. */
export function updateMaterialPrimary(preset: MaterialPreset): MaterialPreset {
  const first = preset.entries?.[0];
  if (!first) return preset; // Legacy JSON presets retain partial-update semantics.
  const raster = ['fill', 'image', 'offset_fill'].includes(preset.operation);
  const vector = !['fill', 'image', 'tool'].includes(preset.operation);
  const entry: CutEntryTemplate = {
    ...first,
    operation: preset.operation,
    speed_mm_min: preset.speed_mm_min,
    power_percent: preset.power_percent,
    raster_settings: raster ? { ...(first.raster_settings ?? defaultRasterSettings()), passes: preset.passes } : null,
    vector_settings: vector ? { ...(first.vector_settings ?? defaultVectorSettings()), passes: preset.passes } : null,
  };
  return withMaterialEntries(preset, [entry, ...preset.entries!.slice(1)]);
}

export function newMaterialEntry(): CutEntryTemplate {
  return {
    operation: 'line', speed_mm_min: 1000, power_percent: 50,
    raster_settings: null, vector_settings: defaultVectorSettings(),
    air_assist: false, power_min_percent: 0, z_offset_mm: 0,
    gcode_prefix: '', gcode_suffix: '', output_enabled: true,
  };
}
