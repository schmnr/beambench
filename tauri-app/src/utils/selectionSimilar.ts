import type { Project, ProjectObject } from '../types/project';
import { normalizeSelectionMembersWithinIsolation } from './arrangementSelection';
import { computeVisualBoundsWorld } from '../canvas/alignment';

export type SimilarSelectionKind =
  | 'layer'
  | 'type'
  | 'size'
  | 'operation'
  | 'circle_diameter'
  | 'open_closed';

const nearlyEqual = (a: number, b: number) => Math.abs(a - b) <= Math.max(0.02, Math.max(Math.abs(a), Math.abs(b)) * 0.002);

function objectTypeKey(object: ProjectObject): string {
  if (object.data.type === 'shape') return `shape:${object.data.kind}`;
  return object.data.type;
}

function isCircle(object: ProjectObject): boolean {
  if (object.data.type !== 'shape' || object.data.kind !== 'ellipse') return false;
  return nearlyEqual(object.bounds.max.x - object.bounds.min.x, object.bounds.max.y - object.bounds.min.y);
}

function matches(project: Project, anchor: ProjectObject, candidate: ProjectObject, kind: SimilarSelectionKind): boolean {
  switch (kind) {
    case 'layer':
      return candidate.layer_id === anchor.layer_id;
    case 'type':
      return objectTypeKey(candidate) === objectTypeKey(anchor);
    case 'size': {
      const anchorBounds = computeVisualBoundsWorld(anchor, project.objects);
      const candidateBounds = computeVisualBoundsWorld(candidate, project.objects);
      const anchorWidth = anchorBounds.max.x - anchorBounds.min.x;
      const anchorHeight = anchorBounds.max.y - anchorBounds.min.y;
      const width = candidateBounds.max.x - candidateBounds.min.x;
      const height = candidateBounds.max.y - candidateBounds.min.y;
      return nearlyEqual(width, anchorWidth) && nearlyEqual(height, anchorHeight);
    }
    case 'operation': {
      const anchorOperation = project.layers.find((layer) => layer.id === anchor.layer_id)?.entries[0]?.operation;
      const candidateOperation = project.layers.find((layer) => layer.id === candidate.layer_id)?.entries[0]?.operation;
      return anchorOperation != null && candidateOperation === anchorOperation;
    }
    case 'circle_diameter': {
      if (!isCircle(anchor) || !isCircle(candidate)) return false;
      const anchorDiameter = anchor.bounds.max.x - anchor.bounds.min.x;
      const candidateDiameter = candidate.bounds.max.x - candidate.bounds.min.x;
      return nearlyEqual(candidateDiameter, anchorDiameter);
    }
    case 'open_closed':
      return anchor.data.type === 'vector_path'
        && candidate.data.type === 'vector_path'
        && candidate.data.closed === anchor.data.closed;
  }
}

export function selectSimilarObjectIds(
  project: Project,
  anchorId: string,
  kind: SimilarSelectionKind,
  isolationRootId: string | null = null,
): string[] {
  const anchor = project.objects.find((object) => object.id === anchorId);
  if (!anchor) return [];
  return normalizeSelectionMembersWithinIsolation(
    project,
    project.objects.filter((candidate) => matches(project, anchor, candidate, kind)).map((candidate) => candidate.id),
    isolationRootId,
  );
}
