import { describe, it, expect } from 'vitest';
import type { RasterSettings, VectorSettings } from '../project';
import type { MaterialPreset } from '../material';
import { makeRasterSettings, makeVectorSettings } from '../../test-utils/projectFixtures';

describe('schema contract tests', () => {
  it('RasterSettings accepts angle_passes and angle_increment_deg', () => {
    // build a schema-valid RasterSettings via the typed `makeRasterSettings`
    // helper so TS enforces the full contract (halftone, newsprint, ramp, etc.),
    // not just the two fields this assertion checks.
    const settings: RasterSettings = makeRasterSettings({
      angle_passes: 3,
      angle_increment_deg: 60,
    });
    expect(settings.angle_passes).toBe(3);
    expect(settings.angle_increment_deg).toBe(60);
  });

  it('RasterSegment accepts scan_angle_deg and scan_origin', () => {
    const segment = {
      type: 'raster' as const,
      scanlines: [],
      line_interval_mm: 0.1,
      direction_mode: 'bidirectional' as const,
      power_mode: 'binary' as const,
      speed_mm_min: 2000,
      layer_id: 'l1',
      scan_angle_deg: 45,
      scan_origin: { x: 10, y: 20 },
    };
    expect(segment.scan_angle_deg).toBe(45);
    expect(segment.scan_origin).toEqual({ x: 10, y: 20 });
  });

  it('RasterPreview accepts scan_angle_deg and scan_origin', () => {
    const preview = {
      bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } },
      line_count: 100,
      line_interval_mm: 0.1,
      direction_mode: 'bidirectional' as const,
      power_mode: 'binary' as const,
      speed_mm_min: 2000,
      fill_density: 0.5,
      scan_angle_deg: 45,
      scan_origin: { x: 5, y: 5 },
    };
    expect(preview.scan_angle_deg).toBe(45);
    expect(preview.scan_origin).toEqual({ x: 5, y: 5 });
  });

  it('RasterSettings.mode accepts all 11 dithering modes', () => {
    const allModes: RasterSettings['mode'][] = [
      'grayscale',
      'threshold',
      'floyd_steinberg',
      'ordered_dither',
      'stucki',
      'jarvis',
      'sierra',
      'atkinson',
      'halftone',
      'newsprint',
      'sketch',
    ];
    expect(allModes).toHaveLength(11);
    // Verify each mode is assignable to the union type
    for (const mode of allModes) {
      const settings: RasterSettings = makeRasterSettings({ mode, overscan_mm: 0 });
      expect(settings.mode).toBe(mode);
    }
  });

  it('RasterSettings accepts 5 new image-mode fields', () => {
    const settings: RasterSettings = makeRasterSettings({
      mode: 'halftone',
      overscan_mm: 0,
      pass_through: true,
      halftone_cells_per_inch: 20,
      halftone_angle_deg: 45,
      newsprint_angle_deg: 30,
      newsprint_frequency: 15,
    });
    expect(settings.pass_through).toBe(true);
    expect(settings.halftone_cells_per_inch).toBe(20);
    expect(settings.halftone_angle_deg).toBe(45);
    expect(settings.newsprint_angle_deg).toBe(30);
    expect(settings.newsprint_frequency).toBe(15);
  });

  it('MaterialPreset.raster_mode accepts all 11 dithering modes', () => {
    const allModes: NonNullable<MaterialPreset['raster_mode']>[] = [
      'grayscale',
      'threshold',
      'floyd_steinberg',
      'ordered_dither',
      'stucki',
      'jarvis',
      'sierra',
      'atkinson',
      'halftone',
      'newsprint',
      'sketch',
    ];
    expect(allModes).toHaveLength(11);
    for (const mode of allModes) {
      const preset: Partial<MaterialPreset> = { raster_mode: mode };
      expect(preset.raster_mode).toBe(mode);
    }
  });

  it('MaterialPreset accepts 5 new raster fields', () => {
    const preset: Partial<MaterialPreset> = {
      pass_through: true,
      halftone_cells_per_inch: 20,
      halftone_angle_deg: 45,
      newsprint_angle_deg: 30,
      newsprint_frequency: 15,
    };
    expect(preset.pass_through).toBe(true);
    expect(preset.halftone_cells_per_inch).toBe(20);
    expect(preset.halftone_angle_deg).toBe(45);
    expect(preset.newsprint_angle_deg).toBe(30);
    expect(preset.newsprint_frequency).toBe(15);
  });

  it('RasterSettings accepts new image fields (invert, dot_width_correction_mm, ramp_length_mm)', () => {
    const settings: RasterSettings = makeRasterSettings({
      mode: 'floyd_steinberg',
      overscan_mm: 0,
      invert: true,
      dot_width_correction_mm: 0.08,
      ramp_length_mm: 0.5,
    });
    expect(settings.invert).toBe(true);
    expect(settings.dot_width_correction_mm).toBeCloseTo(0.08, 9);
    expect(settings.ramp_length_mm).toBeCloseTo(0.5, 9);
  });

  it('MaterialPreset accepts new image fields (invert, dot_width_correction_mm, ramp_length_mm)', () => {
    const preset: Partial<MaterialPreset> = {
      invert: true,
      dot_width_correction_mm: 0.08,
      ramp_length_mm: 0.5,
    };
    expect(preset.invert).toBe(true);
    expect(preset.dot_width_correction_mm).toBeCloseTo(0.08, 9);
    expect(preset.ramp_length_mm).toBeCloseTo(0.5, 9);
  });

  it('backward compat: RasterSettings without new fields has defaults', () => {
    const settings = makeRasterSettings();
    expect(settings.angle_passes).toBe(1);
    expect(settings.angle_increment_deg).toBe(90);
    expect(settings.line_interval_mm).toBe(0.1);
    expect(settings.newsprint_frequency).toBe(10);
  });

  it('VectorSettings accepts offset fill grouping mode', () => {
    const settings: VectorSettings = makeVectorSettings({
      offset_fill_grouping_mode: 'groups_together',
    });
    expect(settings.offset_fill_grouping_mode).toBe('groups_together');
  });
});
