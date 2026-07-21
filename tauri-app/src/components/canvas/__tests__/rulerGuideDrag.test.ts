import { describe, expect, it } from 'vitest';
import { resolveRulerGuideDropValue } from '../rulerGuideDrag';
import type { Workspace } from '../../../types/project';

const workspace: Workspace = {
  bed_width_mm: 400,
  bed_height_mm: 300,
  origin: 'bottom_left',
};

describe('resolveRulerGuideDropValue', () => {
  it('keeps vertical guide drops based only on the X guide coordinate', () => {
    expect(resolveRulerGuideDropValue('vertical', 25, workspace)).toBe(25);
    expect(resolveRulerGuideDropValue('vertical', 400, workspace)).toBe(400);
  });

  it('keeps horizontal guide drops based only on the Y guide coordinate', () => {
    expect(resolveRulerGuideDropValue('horizontal', 75, workspace)).toBe(75);
    expect(resolveRulerGuideDropValue('horizontal', 300, workspace)).toBe(300);
  });

  it('rejects drops where the guide line itself is outside the bed', () => {
    expect(resolveRulerGuideDropValue('vertical', -1, workspace)).toBeNull();
    expect(resolveRulerGuideDropValue('vertical', 401, workspace)).toBeNull();
    expect(resolveRulerGuideDropValue('horizontal', -1, workspace)).toBeNull();
    expect(resolveRulerGuideDropValue('horizontal', 301, workspace)).toBeNull();
  });

  it('rejects non-finite guide coordinates', () => {
    expect(resolveRulerGuideDropValue('vertical', Number.NaN, workspace)).toBeNull();
    expect(resolveRulerGuideDropValue('horizontal', Number.POSITIVE_INFINITY, workspace)).toBeNull();
  });
});
