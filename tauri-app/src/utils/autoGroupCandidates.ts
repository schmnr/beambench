import type { Project, ProjectObject } from '../types/project';

export interface AutoGroupCandidate {
  outerId: string;
  childIds: string[];
  objectIds: string[];
}

function isGuide(object: ProjectObject): boolean {
  return object.data.type === 'vector_path' && object.data.ruler_guide_axis != null;
}

function area(object: ProjectObject): number {
  return Math.max(0, object.bounds.max.x - object.bounds.min.x)
    * Math.max(0, object.bounds.max.y - object.bounds.min.y);
}

function containsBounds(outer: ProjectObject, child: ProjectObject): boolean {
  return (
    child.bounds.min.x >= outer.bounds.min.x &&
    child.bounds.max.x <= outer.bounds.max.x &&
    child.bounds.min.y >= outer.bounds.min.y &&
    child.bounds.max.y <= outer.bounds.max.y
  );
}

function isAutoGroupOuter(object: ProjectObject): boolean {
  if (!object.visible || object.locked || isGuide(object)) return false;
  if (object.data.type === 'vector_path') return object.data.closed;
  return object.data.type === 'shape' || object.data.type === 'star' || object.data.type === 'polygon';
}

function isAutoGroupChild(object: ProjectObject): boolean {
  return object.visible && !object.locked && !isGuide(object);
}

/**
 * Auto-Group candidates: closed vector-compatible selected outers
 * containing selected non-guide children. Each child belongs to the smallest
 * selected outer that contains it.
 */
export function findAutoGroupCandidates(
  project: Project | null,
  selectedObjectIds: string[],
): AutoGroupCandidate[] {
  if (!project || selectedObjectIds.length < 2) return [];
  const selectedSet = new Set(selectedObjectIds);
  const selected = project.objects.filter((object) => selectedSet.has(object.id));
  const outers = selected.filter(isAutoGroupOuter);
  const children = selected.filter(isAutoGroupChild);
  if (outers.length === 0 || children.length === 0) return [];

  const childAssignments = new Map<string, string[]>();
  for (const child of children) {
    const containingOuter = outers
      .filter((outer) => outer.id !== child.id && containsBounds(outer, child))
      .sort((a, b) => area(a) - area(b))[0];
    if (!containingOuter) continue;
    childAssignments.set(containingOuter.id, [...(childAssignments.get(containingOuter.id) ?? []), child.id]);
  }

  return outers
    .map((outer) => {
      const childIds = childAssignments.get(outer.id) ?? [];
      return {
        outerId: outer.id,
        childIds,
        objectIds: [outer.id, ...childIds],
      };
    })
    .filter((candidate) => candidate.childIds.length > 0);
}
