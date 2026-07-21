import type { AppSettings } from '../types/commands';

export type DisplayUnit = AppSettings['display_unit'];
export type SpeedTimeUnit = NonNullable<AppSettings['speed_time_unit']>;

const MM_PER_INCH = 25.4;

export function normalizeSpeedTimeUnit(unit: AppSettings['speed_time_unit']): SpeedTimeUnit {
  return unit === 'seconds' ? 'seconds' : 'minutes';
}

export function speedUnitLabel(
  displayUnit: DisplayUnit = 'mm',
  speedTimeUnit: AppSettings['speed_time_unit'] = 'minutes',
): string {
  const distance = displayUnit === 'inches' ? 'in' : 'mm';
  const time = normalizeSpeedTimeUnit(speedTimeUnit) === 'seconds' ? 'sec' : 'min';
  return `${distance}/${time}`;
}

export function speedMmMinToDisplay(
  speedMmMin: number,
  displayUnit: DisplayUnit = 'mm',
  speedTimeUnit: AppSettings['speed_time_unit'] = 'minutes',
): number {
  const distancePerMinute = displayUnit === 'inches' ? speedMmMin / MM_PER_INCH : speedMmMin;
  return normalizeSpeedTimeUnit(speedTimeUnit) === 'seconds'
    ? distancePerMinute / 60
    : distancePerMinute;
}

export function displaySpeedToMmMin(
  displaySpeed: number,
  displayUnit: DisplayUnit = 'mm',
  speedTimeUnit: AppSettings['speed_time_unit'] = 'minutes',
): number {
  const distancePerMinute = normalizeSpeedTimeUnit(speedTimeUnit) === 'seconds'
    ? displaySpeed * 60
    : displaySpeed;
  return displayUnit === 'inches' ? distancePerMinute * MM_PER_INCH : distancePerMinute;
}

export function speedStepForUnit(
  displayUnit: DisplayUnit = 'mm',
  speedTimeUnit: AppSettings['speed_time_unit'] = 'minutes',
): number {
  if (displayUnit === 'inches') {
    return normalizeSpeedTimeUnit(speedTimeUnit) === 'seconds' ? 0.1 : 1;
  }
  return normalizeSpeedTimeUnit(speedTimeUnit) === 'seconds' ? 1 : 100;
}

export function formatSpeedForDisplay(
  speedMmMin: number,
  displayUnit: DisplayUnit = 'mm',
  speedTimeUnit: AppSettings['speed_time_unit'] = 'minutes',
): string {
  const displaySpeed = speedMmMinToDisplay(speedMmMin, displayUnit, speedTimeUnit);
  if (!Number.isFinite(displaySpeed)) return '0';
  if (displayUnit === 'inches') {
    return displaySpeed.toFixed(normalizeSpeedTimeUnit(speedTimeUnit) === 'seconds' ? 2 : 1).replace(/\.0+$/, '').replace(/(\.\d*[1-9])0+$/, '$1');
  }
  if (normalizeSpeedTimeUnit(speedTimeUnit) === 'seconds' && !Number.isInteger(displaySpeed)) {
    return displaySpeed.toFixed(1).replace(/\.0$/, '');
  }
  return String(Math.round(displaySpeed));
}

export function speedInputValue(
  speedMmMin: number,
  displayUnit: DisplayUnit = 'mm',
  speedTimeUnit: AppSettings['speed_time_unit'] = 'minutes',
): number {
  // Inputs intentionally use the same rounded precision as summaries; conversion back to mm/min is lossy after edits.
  return Number(formatSpeedForDisplay(speedMmMin, displayUnit, speedTimeUnit));
}
