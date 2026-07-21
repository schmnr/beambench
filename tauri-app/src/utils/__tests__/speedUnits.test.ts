import { describe, expect, it } from 'vitest';
import {
  displaySpeedToMmMin,
  formatSpeedForDisplay,
  speedInputValue,
  speedMmMinToDisplay,
  speedUnitLabel,
} from '../speedUnits';

describe('speed unit conversions', () => {
  it('keeps mm/min values unchanged by default', () => {
    expect(speedUnitLabel('mm', 'minutes')).toBe('mm/min');
    expect(speedMmMinToDisplay(3000, 'mm', 'minutes')).toBe(3000);
    expect(displaySpeedToMmMin(3000, 'mm', 'minutes')).toBe(3000);
    expect(formatSpeedForDisplay(3000, 'mm', 'minutes')).toBe('3000');
  });

  it('converts mm/min to mm/sec and back', () => {
    expect(speedUnitLabel('mm', 'seconds')).toBe('mm/sec');
    expect(speedMmMinToDisplay(3000, 'mm', 'seconds')).toBe(50);
    expect(displaySpeedToMmMin(75, 'mm', 'seconds')).toBe(4500);
    expect(speedInputValue(4500, 'mm', 'seconds')).toBe(75);
  });

  it('combines inch display units with speed time units', () => {
    expect(speedUnitLabel('inches', 'seconds')).toBe('in/sec');
    expect(formatSpeedForDisplay(3048, 'inches', 'minutes')).toBe('120');
    expect(formatSpeedForDisplay(3048, 'inches', 'seconds')).toBe('2');
    expect(displaySpeedToMmMin(2, 'inches', 'seconds')).toBeCloseTo(3048);
  });
});
