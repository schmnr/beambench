import type { AppSettings } from '../types/commands';

export type DisplayUnit = AppSettings['display_unit']; // 'mm' | 'inches'

const MM_PER_INCH = 25.4;

/** Convert an internal millimeter value to the user's display unit. */
export function mmToDisplay(mm: number, unit: DisplayUnit = 'mm'): number {
  return unit === 'inches' ? mm / MM_PER_INCH : mm;
}

/** Convert a value the user typed (in their display unit) back to millimeters. */
export function displayToMm(value: number, unit: DisplayUnit = 'mm'): number {
  return unit === 'inches' ? value * MM_PER_INCH : value;
}

/**
 * Round a display value to a sensible precision.
 * mm: 2 decimals, inches: 4 decimals (matches the existing
 * PropertiesToolbar / ModifiersToolbar behavior).
 */
export function roundDisplayLength(value: number, unit: DisplayUnit = 'mm'): number {
  if (!Number.isFinite(value)) return 0;
  const dp = unit === 'inches' ? 4 : 2;
  const f = 10 ** dp;
  return Math.round(value * f) / f;
}

/** Short unit label for a length field. */
export function lengthUnitLabel(unit: DisplayUnit = 'mm'): string {
  return unit === 'inches' ? 'in' : 'mm';
}

/** Append a unit suffix to a translated, unit-free label. */
export function labelWithUnit(label: string, unit: string): string {
  return `${label} (${unit})`;
}

/**
 * Friendly increment for a numeric stepper. Metric returns mmStep verbatim;
 * imperial returns inchStep (default 0.05", a comfortable inch increment).
 */
export function lengthStep(unit: DisplayUnit = 'mm', mmStep = 1, inchStep = 0.05): number {
  return unit === 'inches' ? inchStep : mmStep;
}
