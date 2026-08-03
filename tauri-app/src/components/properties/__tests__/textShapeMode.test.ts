import { describe, expect, it } from 'vitest';
import {
  activeTextShape,
  circleCurveToDegrees,
  circleDegreesToCurve,
  circlePlacementFromParts,
  circlePlacementParts,
  patchForTextShape,
  signedTextShapeValue,
} from '../textShapeMode';

const base = {
  layout_mode: 'straight' as const,
  on_path: false,
  bend_radius: 0,
  transform_style: 'none' as const,
  transform_curve: 0,
  circle_placement: 'top_outside' as const,
};

describe('textShapeMode', () => {
  it('keeps layout and envelope modes mutually exclusive', () => {
    expect(patchForTextShape({ ...base, transform_style: 'wave' }, 'bend')).toMatchObject({
      layout_mode: 'bend',
      on_path: false,
      transform_style: 'none',
      bend_radius: 50,
    });
    expect(patchForTextShape({ ...base, layout_mode: 'path', on_path: true }, 'wave')).toMatchObject({
      layout_mode: 'straight',
      on_path: false,
      transform_style: 'wave',
      transform_curve: 50,
    });
    expect(patchForTextShape({ ...base, transform_curve: 50 }, 'circle')).toMatchObject({
      transform_style: 'circle',
      transform_curve: 0,
    });
  });

  it('reports the one active shape', () => {
    expect(activeTextShape({ ...base, layout_mode: 'bend' })).toBe('bend');
    expect(activeTextShape({ ...base, layout_mode: 'path', on_path: true })).toBe('path');
    expect(activeTextShape({ ...base, layout_mode: 'bend', transform_style: 'circle' })).toBe('circle');
  });

  it('encodes normal and reverse direction without zero-strength no-ops', () => {
    expect(signedTextShapeValue(0, false, 50)).toBe(50);
    expect(signedTextShapeValue(0, true, 50)).toBe(-50);
    expect(signedTextShapeValue(-25, false, 50)).toBe(25);
  });

  it('round-trips circle position and arc span controls', () => {
    expect(circlePlacementParts('bottom_inside')).toEqual({ vertical: 'bottom', side: 'inside' });
    expect(circlePlacementFromParts('top', 'inside')).toBe('top_inside');
    expect(circleCurveToDegrees(circleDegreesToCurve(270))).toBe(270);
  });
});
