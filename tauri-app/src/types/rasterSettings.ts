// Shared helpers that mirror the Rust-side
// `RasterSettings::effective_dpi()` / `effective_line_interval_mm()`
// implementations in `crates/beambench-core/src/layer.rs`. All UI code
// that displays or persists the image-layer resolution should go
// through these so the line interval control is the canonical
// editable value, with the legacy `dpi` field used purely as a
// fallback for old projects.
//
// The two helpers always agree: if `line_interval_mm` is set and
// positive they both derive from it, otherwise they both fall back
// to the legacy `dpi` field, otherwise they both fall back to the
// project-wide default (254 dpi / 0.1 mm).

import type { RasterSettings } from './project';

const DEFAULT_DPI = 254;
const DEFAULT_LINE_INTERVAL_MM = 0.1;

/** Canonical resolution in DPI for an image layer. */
export function effectiveDpi(rs: RasterSettings | null | undefined): number {
  if (!rs) return DEFAULT_DPI;
  if (typeof rs.line_interval_mm === 'number' && rs.line_interval_mm > 0) {
    return Math.round(25.4 / rs.line_interval_mm);
  }
  if (typeof rs.dpi === 'number' && rs.dpi > 0) return rs.dpi;
  return DEFAULT_DPI;
}

/** Canonical line interval in mm for an image layer. */
export function effectiveLineIntervalMm(rs: RasterSettings | null | undefined): number {
  if (!rs) return DEFAULT_LINE_INTERVAL_MM;
  if (typeof rs.line_interval_mm === 'number' && rs.line_interval_mm > 0) {
    return rs.line_interval_mm;
  }
  if (typeof rs.dpi === 'number' && rs.dpi > 0) return 25.4 / rs.dpi;
  return DEFAULT_LINE_INTERVAL_MM;
}
