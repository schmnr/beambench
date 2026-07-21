import { describe, it, expect } from 'vitest';
import {
  effectiveDpi,
  effectiveLineIntervalMm,
} from '../rasterSettings';
import type { RasterSettings } from '../project';
import { makeRasterSettings } from '../../test-utils/projectFixtures';

type LegacyRasterSettings = Omit<RasterSettings, 'line_interval_mm'> & {
  line_interval_mm?: number;
};

function baseRs(partial: Partial<RasterSettings> = {}): RasterSettings {
  return makeRasterSettings(partial);
}

describe('effectiveDpi', () => {
  it('prefers line_interval_mm when set and positive', () => {
    const rs = baseRs({ dpi: 100, line_interval_mm: 0.05 });
    expect(effectiveDpi(rs)).toBe(508);
  });

  it('falls back to legacy dpi when line_interval_mm is missing', () => {
    const rs: LegacyRasterSettings = { ...baseRs({ dpi: 300 }) };
    delete rs.line_interval_mm;
    expect(effectiveDpi(rs as unknown as RasterSettings)).toBe(300);
  });

  it('uses legacy dpi when line_interval_mm is zero', () => {
    const rs = baseRs({ dpi: 400, line_interval_mm: 0 });
    expect(effectiveDpi(rs)).toBe(400);
  });

  it('defaults to 254 when nothing is set', () => {
    const rs: LegacyRasterSettings = { ...baseRs({ dpi: 0 }) };
    delete rs.line_interval_mm;
    expect(effectiveDpi(rs as unknown as RasterSettings)).toBe(254);
  });

  it('defaults to 254 when null/undefined', () => {
    expect(effectiveDpi(null)).toBe(254);
    expect(effectiveDpi(undefined)).toBe(254);
  });
});

describe('effectiveLineIntervalMm', () => {
  it('prefers line_interval_mm when set', () => {
    const rs = baseRs({ dpi: 100, line_interval_mm: 0.08 });
    expect(effectiveLineIntervalMm(rs)).toBeCloseTo(0.08, 9);
  });

  it('falls back to legacy dpi derivation', () => {
    const rs: LegacyRasterSettings = { ...baseRs({ dpi: 254 }) };
    delete rs.line_interval_mm;
    expect(effectiveLineIntervalMm(rs as unknown as RasterSettings)).toBeCloseTo(0.1, 4);
  });

  it('defaults to 0.1 when nothing is set', () => {
    const rs: LegacyRasterSettings = { ...baseRs({ dpi: 0 }) };
    delete rs.line_interval_mm;
    expect(effectiveLineIntervalMm(rs as unknown as RasterSettings)).toBeCloseTo(0.1, 9);
  });

  it('defaults to 0.1 when null/undefined', () => {
    expect(effectiveLineIntervalMm(null)).toBeCloseTo(0.1, 9);
    expect(effectiveLineIntervalMm(undefined)).toBeCloseTo(0.1, 9);
  });
});
