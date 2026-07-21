import type { Bounds, Project, ProjectObject, Workspace } from '../types/project';

export type RulerGuideAxis = 'horizontal' | 'vertical';

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}

export function buildRulerGuideGeometry(
  axis: RulerGuideAxis,
  valueMm: number,
  workspace: Workspace,
): { path_data: string; bounds: Bounds } {
  if (axis === 'vertical') {
    const x = clamp(valueMm, 0, workspace.bed_width_mm);
    return {
      path_data: `M ${x} 0 L ${x} ${workspace.bed_height_mm}`,
      bounds: {
        min: { x, y: 0 },
        max: { x, y: workspace.bed_height_mm },
      },
    };
  }

  const y = clamp(valueMm, 0, workspace.bed_height_mm);
  return {
    path_data: `M 0 ${y} L ${workspace.bed_width_mm} ${y}`,
    bounds: {
      min: { x: 0, y },
      max: { x: workspace.bed_width_mm, y },
    },
  };
}

export function normalizeRulerGuideObject(
  obj: ProjectObject,
  workspace: Workspace,
): ProjectObject {
  if (obj.data.type !== 'vector_path' || !obj.data.ruler_guide_axis) return obj;

  const axis = obj.data.ruler_guide_axis;
  const valueMm = axis === 'vertical' ? obj.bounds.min.x : obj.bounds.min.y;
  const geometry = buildRulerGuideGeometry(axis, valueMm, workspace);
  return {
    ...obj,
    bounds: geometry.bounds,
    data: {
      ...obj.data,
      path_data: geometry.path_data,
    },
  };
}

export function normalizeProjectRulerGuides(project: Project): Project {
  return {
    ...project,
    objects: project.objects.map((obj) => normalizeRulerGuideObject(obj, project.workspace)),
  };
}
