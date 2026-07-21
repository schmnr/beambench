import type { Project, ProjectObject } from '../types/project';

function isToolLikeObject(project: Project, object: ProjectObject): boolean {
  const layer = project.layers.find((candidate) => candidate.id === object.layer_id);
  return Boolean(layer?.is_tool_layer)
    || (object.data.type === 'vector_path' && object.data.ruler_guide_axis != null);
}

function findParentGroupId(project: Project, objectId: string): string | null {
  for (const object of project.objects) {
    if (object.data.type === 'group' && object.data.children.includes(objectId)) {
      return object.id;
    }
  }
  return null;
}

export function topLevelArrangementObjectId(project: Project, objectId: string): string {
  let current = objectId;
  while (true) {
    const parentId = findParentGroupId(project, current);
    if (!parentId) return current;
    current = parentId;
  }
}

export function normalizeArrangementSelection(project: Project, objectIds: string[]): string[] {
  const seen = new Set<string>();
  const normalized: string[] = [];
  for (const objectId of objectIds) {
    const object = project.objects.find((candidate) => candidate.id === objectId);
    if (!object || isToolLikeObject(project, object)) continue;
    const promoted = topLevelArrangementObjectId(project, objectId);
    if (seen.has(promoted)) continue;
    seen.add(promoted);
    normalized.push(promoted);
  }
  return normalized;
}

/**
 * Selection-side normalization: validates the IDs exist, promotes group children
 * to their top-level parent, and dedupes. Unlike `normalizeArrangementSelection`,
 * this keeps tool-layer objects and ruler guides — they are valid selection targets
 * (they just don't participate in arrange/align operations).
 */
export function normalizeSelectionMembers(project: Project, objectIds: string[]): string[] {
  const seen = new Set<string>();
  const normalized: string[] = [];
  for (const objectId of objectIds) {
    const object = project.objects.find((candidate) => candidate.id === objectId);
    if (!object) continue;
    const promoted = topLevelArrangementObjectId(project, objectId);
    if (seen.has(promoted)) continue;
    seen.add(promoted);
    normalized.push(promoted);
  }
  return normalized;
}

function collectGroupDescendants(project: Project, objectId: string, output: string[], seen: Set<string>): void {
  const object = project.objects.find((candidate) => candidate.id === objectId);
  if (!object || object.data.type !== 'group') return;
  for (const childId of object.data.children) {
    if (seen.has(childId)) continue;
    seen.add(childId);
    output.push(childId);
    collectGroupDescendants(project, childId, output, seen);
  }
}

export function expandArrangementSelectionMembers(project: Project, objectIds: string[]): string[] {
  const expanded: string[] = [];
  const seen = new Set<string>();
  for (const rootId of normalizeArrangementSelection(project, objectIds)) {
    if (!seen.has(rootId)) {
      seen.add(rootId);
      expanded.push(rootId);
    }
    collectGroupDescendants(project, rootId, expanded, seen);
  }
  return expanded;
}

export function expandSelectionMembers(project: Project, objectIds: string[]): string[] {
  const expanded: string[] = [];
  const seen = new Set<string>();
  for (const rootId of normalizeSelectionMembers(project, objectIds)) {
    if (!seen.has(rootId)) {
      seen.add(rootId);
      expanded.push(rootId);
    }
    collectGroupDescendants(project, rootId, expanded, seen);
  }
  return expanded;
}

export function resolveArrangementAnchorId(project: Project, objectIds: string[]): string | null {
  const normalized = normalizeArrangementSelection(project, objectIds);
  return normalized.length > 0 ? normalized[normalized.length - 1] : null;
}
