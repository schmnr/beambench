import type { Workspace } from '../../types/project';

export type RulerGuideAxis = 'horizontal' | 'vertical';

export function resolveRulerGuideDropValue(
  axis: RulerGuideAxis,
  valueMm: number,
  workspace: Workspace,
): number | null {
  if (!Number.isFinite(valueMm)) {
    return null;
  }

  const max = axis === 'vertical' ? workspace.bed_width_mm : workspace.bed_height_mm;
  if (valueMm < 0 || valueMm > max) {
    return null;
  }

  return Math.max(0, Math.min(max, valueMm));
}
