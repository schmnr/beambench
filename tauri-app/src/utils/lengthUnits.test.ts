import { describe, it, expect } from 'vitest';
import {
  mmToDisplay, displayToMm, roundDisplayLength, lengthUnitLabel, lengthStep, labelWithUnit,
} from './lengthUnits';

describe('lengthUnits', () => {
  it('mmToDisplay leaves mm unchanged and converts inches', () => {
    expect(mmToDisplay(25.4, 'mm')).toBe(25.4);
    expect(mmToDisplay(25.4, 'inches')).toBeCloseTo(1, 10);
  });
  it('displayToMm inverts mmToDisplay', () => {
    expect(displayToMm(1, 'inches')).toBeCloseTo(25.4, 10);
    expect(displayToMm(10, 'mm')).toBe(10);
    expect(displayToMm(mmToDisplay(137.2, 'inches'), 'inches')).toBeCloseTo(137.2, 8);
  });
  it('roundDisplayLength: 2dp mm, 4dp inches (matches toolbars)', () => {
    expect(roundDisplayLength(1.23456, 'mm')).toBe(1.23);
    expect(roundDisplayLength(1.23456, 'inches')).toBe(1.2346);
  });
  it('lengthUnitLabel returns mm or in', () => {
    expect(lengthUnitLabel('mm')).toBe('mm');
    expect(lengthUnitLabel('inches')).toBe('in');
  });
  it('labelWithUnit appends parenthesized unit', () => {
    expect(labelWithUnit('Distance', 'mm')).toBe('Distance (mm)');
    expect(labelWithUnit('Feed', 'in/min')).toBe('Feed (in/min)');
  });
  it('lengthStep returns mm step metric, inch step imperial', () => {
    expect(lengthStep('mm', 0.5, 0.02)).toBe(0.5);
    expect(lengthStep('inches', 0.5, 0.02)).toBe(0.02);
    expect(lengthStep('inches', 1)).toBe(0.05);
  });
  it('defaults to mm when unit omitted', () => {
    expect(mmToDisplay(7)).toBe(7);
    expect(displayToMm(7)).toBe(7);
    expect(lengthUnitLabel()).toBe('mm');
  });
});
