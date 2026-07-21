import { describe, it, expect } from 'vitest';

// Locale bundles, discovered dynamically (adding/removing a locale affects this test).
const locales = (
  import.meta as ImportMeta & {
    glob?: (p: string, o: { eager: true; import: 'default' }) => Record<string, unknown>;
  }
).glob!('../*.json', { eager: true, import: 'default' });

/**
 * Field-label keys whose VALUE must stay unit-free. The unit ("mm" / "in" /
 * "mm/min" ...) is appended in code via lengthUnits.labelWithUnit /
 * speedUnits.speedUnitLabel so it can follow the display_unit setting.
 *
 * Keep this list in sync with the metric/imperial sweep. Standalone unit
 * strings (e.g. panels.machine.status.mm_per_min, panels.measurement.unit_mm)
 * and readouts that interpolate a value (grid_array.footprint,
 * camera_alignment.rmse) are intentionally NOT listed here.
 */
const UNIT_FREE_KEYS = [
  'dialog.offset.distance',
  'dialog.barcode.width', 'dialog.barcode.height',
  'dialog.grid_array.total_width', 'dialog.grid_array.total_height',
  'dialog.grid_array.h_spacing', 'dialog.grid_array.v_spacing',
  'dialog.grid_array.x_col_shift', 'dialog.grid_array.y_row_shift',
  'dialog.focus_test.z_min', 'dialog.focus_test.z_max', 'dialog.focus_test.speed',
  'dialog.focus_test.line_length', 'dialog.focus_test.step_spacing', 'dialog.focus_test.material_height',
  'dialog.resize_slots.old_thickness', 'dialog.resize_slots.new_thickness', 'dialog.resize_slots.tolerance',
  'dialog.circular_array.radius', 'dialog.circular_array.center_x', 'dialog.circular_array.center_y',
  'dialog.interval_test.min_interval', 'dialog.interval_test.max_interval', 'dialog.interval_test.speed',
  'dialog.interval_test.cell_w', 'dialog.interval_test.cell_h', 'dialog.interval_test.cell_spacing',
  'dialog.nest.min_spacing', 'dialog.dock.padding',
  'dialog.settings.grid_spacing', 'dialog.settings.nudge_step', 'dialog.settings.nudge_fine', 'dialog.settings.nudge_coarse',
  'dialog.adjust_image.line_interval',
  'dialog.material_test.base_interval', 'dialog.material_test.cell_width', 'dialog.material_test.cell_height',
  'dialog.material_test.cell_spacing', 'dialog.material_test.text_speed', 'dialog.material_test.border_speed',
  'dialog.material_test.center_x', 'dialog.material_test.center_y',
  'dialog.device_settings.bed_width', 'dialog.device_settings.bed_height', 'dialog.device_settings.max_speed',
  'dialog.device_settings.dot_width', 'dialog.device_settings.speed', 'dialog.device_settings.offset',
  'dialog.device_settings.width_mm', 'dialog.device_settings.height_mm',
  'dialog.machine_profile.width_mm', 'dialog.machine_profile.height_mm', 'dialog.machine_profile.z_feed',
  'dialog.machine_profile.dot_width', 'dialog.machine_profile.speed', 'dialog.machine_profile.offset',
  'panels.text_properties.size_mm',
  'panels.sub_layer_stack.z_offset_mm', 'panels.sub_layer_stack.line_interval_mm', 'panels.sub_layer_stack.overscan_mm',
  'panels.sub_layer_stack.kerf_offset_mm', 'panels.sub_layer_stack.offset_overlap_mm',
  'panels.layers.quick_edit.kerf_mm',
  'panels.move.step_mm', 'panels.move.feed_mm_min',
  'panels.machine.jog.step_size_mm', 'panels.machine.jog.feed_mm_min',
  'panels.machine.laser.tolerance_mm',
  'panels.machine.material_library.thickness_mm',
];

// A trailing unit token: a parenthetical group (ASCII or fullwidth) OR a bare
// distance token (mm/cm/in/inch + CJK + Cyrillic) optionally followed by /time.
const TRAILING_UNIT =
  /(?:[（(][^()（）]*[）)]|(?:\bmm\b|\bcm\b|\bin\b|\binch\b|毫米|公釐|公厘|英寸|英吋|мм|см)(?:\s*\/\s*\S+)?)\s*$/iu;

function getString(obj: unknown, path: string): string | undefined {
  const v = path.split('.').reduce<unknown>(
    (acc, k) => (acc && typeof acc === 'object' ? (acc as Record<string, unknown>)[k] : undefined),
    obj,
  );
  return typeof v === 'string' ? v : undefined;
}

describe('unit-label-guard', () => {
  for (const [p, bundle] of Object.entries(locales)) {
    const code = p.match(/\.\.\/(.+)\.json$/)?.[1] ?? p;
    it(`${code}: field-label keys carry no trailing unit token`, () => {
      const offenders = UNIT_FREE_KEYS.map((k) => [k, getString(bundle, k)] as const)
        .filter(([, v]) => v !== undefined && TRAILING_UNIT.test(v))
        .map(([k, v]) => `${k}: "${v}"`);
      expect(offenders, `Unit token left in ${code}`).toEqual([]);
    });
  }
});
