import type { Bounds, Point2D, ProjectObject } from './project';

export interface NodeId {
  subpath_idx: number;
  command_idx: number;
}

export type HandleType = 'in' | 'out';
export type NodeSelectionTarget =
  | { kind: 'node'; nodeId: NodeId }
  | { kind: 'handle'; nodeId: NodeId; handleType: HandleType };

export interface NodeBatchUpdate {
  node_id: NodeId;
  x: number;
  y: number;
  handle_type?: HandleType | null;
}
export type OffsetDirection = 'inward' | 'outward' | 'both';
export type OffsetCornerStyle = 'miter' | 'round' | 'bevel';

/** One flattened offset-preview polyline, world coords, drawn as a dashed ghost. */
export interface OffsetPreviewPath {
  points: Point2D[];
  closed: boolean;
}

/** Result of a non-mutating offset preview. `source_all_open` is true only when
 *  the whole selection is open paths (the only case that yields preview paths). */
export interface OffsetPreview {
  paths: OffsetPreviewPath[];
  source_all_open: boolean;
}
export type GridSpacingMode = 'centerToCenter' | 'edgeToEdge';
export type GridArraySizingMode = 'count' | 'total';
export type StartPointMode = 'set' | 'set_and_reverse' | 'reset';
export type BooleanAssistantOperation = 'union' | 'subtract' | 'intersection' | 'weld' | 'exclude';

export interface BooleanAssistantSourcePreview {
  id: string;
  name: string;
  bounds: Bounds;
  pathData: string;
}

export interface BooleanAssistantPreview {
  operation: BooleanAssistantOperation;
  result: ProjectObject;
  sources: BooleanAssistantSourcePreview[];
}

export interface CopyAlongPathOptions {
  count: number;
  rotateCopies: boolean;
  scaleCopies: boolean;
  finalScalePercent: number;
}

export type NodeType = 'smooth' | 'corner';

export interface PathNode {
  id: NodeId;
  position: Point2D;
  handle_in: Point2D | null;
  handle_out: Point2D | null;
  node_type: NodeType;
}

export interface EditablePath {
  nodes: PathNode[];
  closed: boolean;
}

export interface NormalizedVector {
  polylines: { points: Point2D[]; closed: boolean }[];
  layer_id: string;
  source_object_name: string;
}
