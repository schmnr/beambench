import type { Point2D, ProjectObject, ObjectData, Bounds, Transform2D, TransformLocks, Workspace } from '../../types/project';
import type { ViewportParams } from '../ViewportTransform';
import type { ToolOverlay } from '../CanvasRenderer';

export interface CanvasMouseEvent {
  /** Screen-space position (CSS pixels relative to canvas) */
  screenX: number;
  screenY: number;
  /** World-space position (mm) */
  worldX: number;
  worldY: number;
  /** Snapped world-space position (if snap enabled) */
  snappedX: number;
  snappedY: number;
  button: number;
  shiftKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
}

export interface ToolContext {
  vp: ViewportParams;
  workspace?: Workspace;
  objects: ProjectObject[];
  selectedObjectIds: string[];
  selectedLayerId: string | null;
  layers: { id: string; enabled: boolean; visible?: boolean; operation?: string }[];
  transformLocks?: TransformLocks;
  snapEnabled: boolean;
  snapToObjects: boolean;
  gridSpacingMm: number;

  // Callbacks for tool actions
  selectObjects: (ids: string[]) => void;
  toggleObjectSelection: (id: string) => void;
  addObject: (name: string, layerId: string, objectData: ObjectData, bounds: Bounds) => Promise<ProjectObject | null>;
  updateObject: (id: string, updates: {
    name?: string;
    visible?: boolean;
    locked?: boolean;
    layer_id?: string;
    transform?: Transform2D;
    bounds?: Bounds;
  }) => Promise<boolean>;
  rotateObjects: (objectIds: string[], degrees: number, pivot?: { x: number; y: number }) => Promise<void>;
  shearObjects: (objectIds: string[], shearX: number, shearY: number, pivot?: { x: number; y: number }) => Promise<void>;
  updateObjectBoundsBatch: (entries: { id: string; bounds: Bounds }[]) => Promise<void>;
  setCursorWorldPos: (pos: Point2D | null) => void;
  setStatusMessage: (msg: string) => void;
  requestRender: () => void;
}

export interface CanvasTool {
  name: string;
  onMouseDown(e: CanvasMouseEvent, ctx: ToolContext): void;
  onMouseMove(e: CanvasMouseEvent, ctx: ToolContext): void;
  onMouseUp(e: CanvasMouseEvent, ctx: ToolContext): void;
  onDoubleClick?(e: CanvasMouseEvent, ctx: ToolContext): void;
  onKeyDown?(e: KeyboardEvent, ctx: ToolContext): void;
  getCursor(ctx: ToolContext): string;
  getOverlay(): ToolOverlay;
  reset(): void;
}
