/**
 * Frontend mirror of Rust `RasterSettings::default()` and `VectorSettings::default()`.
 *
 * **CANONICAL SOURCE:** `crates/beambench-core/src/layer.rs` — `CutEntry::defaults_for(operation)`
 * + `RasterSettings::default()` + `VectorSettings::default()`.
 *
 * These mirrors exist for inline UI fallbacks where the operation-switch path needs to
 * synchronously fabricate a missing settings bag (e.g., switching a Line layer to Fill needs a
 * raster_settings bag client-side before the next backend round-trip). The user-facing
 * **Reset to Defaults** action routes through `projectStore.resetCutEntryToDefaults` →
 * `reset_cut_entry_to_defaults` Tauri command, which uses the canonical Rust source — never
 * these mirrors.
 *
 * **Drift risk:** if you change a default value here without updating the Rust side (or vice
 * versa), the two will silently disagree. A backend
 * `default_settings_for_operation(op)` command could eventually replace these mirrors.
 */
import type { RasterSettings, VectorSettings } from './project';

export function defaultRasterSettings(): RasterSettings {
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

export function defaultVectorSettings(): VectorSettings {
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
