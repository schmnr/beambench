import { invoke } from '@tauri-apps/api/core';
import type { ArrayResult, Bounds, ImageMaskPolarity, Point2D, ProjectObject } from '../types/project';
import type {
  CopyAlongPathOptions,
  BooleanAssistantOperation,
  BooleanAssistantPreview,
  EditablePath,
  GridArraySizingMode,
  GridSpacingMode,
  HandleType,
  NodeBatchUpdate,
  NodeId,
  NormalizedVector,
  OffsetCornerStyle,
  OffsetDirection,
  OffsetPreview,
  StartPointMode,
} from '../types/vector';

export interface CutShapesApplyResult {
  createdObjectIds: string[];
  insideGroupId?: string | null;
  outsideGroupId?: string | null;
  cutterObjectId: string;
}

export const vectorService = {
  convertToPath: (objectId: string): Promise<ProjectObject> =>
    invoke('convert_to_path', { objectId }),

  booleanUnion: (objectIdA: string, objectIdB: string): Promise<ProjectObject> =>
    invoke('boolean_union', { objectIdA, objectIdB }),

  booleanSubtract: (objectIdA: string, objectIdB: string): Promise<ProjectObject> =>
    invoke('boolean_subtract', { objectIdA, objectIdB }),

  booleanExclude: (objectIdA: string, objectIdB: string): Promise<ProjectObject> =>
    invoke('boolean_exclude', { objectIdA, objectIdB }),

  booleanAssistantPreview: (
    objectIds: string[],
    operation: BooleanAssistantOperation,
  ): Promise<BooleanAssistantPreview> =>
    invoke('boolean_assistant_preview', { objectIds, operation }),

  groupObjects: (objectIds: string[]): Promise<ProjectObject> =>
    invoke('group_objects', { objectIds }),

  autoGroupObjects: (objectIds: string[]): Promise<ProjectObject[]> =>
    invoke('auto_group_objects', { objectIds }),

  ungroupObjects: (groupId: string): Promise<string[]> =>
    invoke('ungroup_objects', { groupId }),

  getEditablePath: (objectId: string): Promise<EditablePath[]> =>
    invoke('get_editable_path', { objectId }),

  updateNode: (
    objectId: string,
    subpathIdx: number,
    commandIdx: number,
    x: number,
    y: number,
    handleType?: HandleType,
  ): Promise<ProjectObject> =>
    invoke('update_node', { objectId, subpathIdx, commandIdx, x, y, handleType }),

  updateNodesBatch: (
    objectId: string,
    updates: NodeBatchUpdate[],
  ): Promise<ProjectObject> =>
    invoke('update_nodes_batch', { objectId, updates }),

  setNodeType: (
    objectId: string,
    subpathIdx: number,
    commandIdx: number,
    nodeType: 'smooth' | 'corner',
  ): Promise<ProjectObject> =>
    invoke('set_node_type', { objectId, subpathIdx, commandIdx, nodeType }),

  deleteNode: (objectId: string, subpathIdx: number, commandIdx: number): Promise<ProjectObject> =>
    invoke('delete_node', { objectId, subpathIdx, commandIdx }),

  deleteNodes: (objectId: string, nodeIds: NodeId[]): Promise<ProjectObject> =>
    invoke('delete_nodes', { objectId, nodeIds }),

  insertNode: (objectId: string, subpathIdx: number, commandIdx: number, t: number): Promise<ProjectObject> =>
    invoke('insert_node', { objectId, subpathIdx, commandIdx, t }),

  convertSegmentToLine: (objectId: string, subpathIdx: number, commandIdx: number): Promise<ProjectObject> =>
    invoke('convert_segment_to_line', { objectId, subpathIdx, commandIdx }),

  convertSegmentToCurve: (objectId: string, subpathIdx: number, commandIdx: number): Promise<ProjectObject> =>
    invoke('convert_segment_to_curve', { objectId, subpathIdx, commandIdx }),

  alignSegmentToAngle: (objectId: string, subpathIdx: number, commandIdx: number): Promise<ProjectObject> =>
    invoke('align_segment_to_angle', { objectId, subpathIdx, commandIdx }),

  trimSegmentToIntersection: (
    objectId: string,
    subpathIdx: number,
    commandIdx: number,
    clickX: number,
    clickY: number,
  ): Promise<ProjectObject> =>
    invoke('trim_segment_to_intersection', { objectId, subpathIdx, commandIdx, clickX, clickY }),

  extendEndpointToIntersection: (objectId: string, nodeId: NodeId): Promise<ProjectObject> =>
    invoke('extend_endpoint_to_intersection', {
      objectId,
      subpathIdx: nodeId.subpath_idx,
      commandIdx: nodeId.command_idx,
    }),

  joinSubpaths: (objectId: string, srcNodeId: NodeId, dstNodeId: NodeId): Promise<ProjectObject> =>
    invoke('join_subpaths', { objectId, srcNodeId, dstNodeId }),

  deleteSegment: (objectId: string, subpathIdx: number, commandIdx: number): Promise<ProjectObject> =>
    invoke('delete_segment_cmd', { objectId, subpathIdx, commandIdx }),

  breakPathAtNode: (objectId: string, subpathIdx: number, commandIdx: number): Promise<ProjectObject> =>
    invoke('break_path_at_node', { objectId, subpathIdx, commandIdx }),

  togglePathClosed: (objectId: string, subpathIdx: number): Promise<ProjectObject> =>
    invoke('toggle_path_closed', { objectId, subpathIdx }),

  scalePathToBounds: (
    objectId: string,
    newMinX: number,
    newMinY: number,
    newMaxX: number,
    newMaxY: number,
  ): Promise<ProjectObject> =>
    invoke('scale_path_to_bounds', { objectId, newMinX, newMinY, newMaxX, newMaxY }),

  meshDeformSelection: (
    objectIds: string[],
    sourceBounds: Bounds,
    handles: Point2D[],
    gridSize: number,
    perspective: boolean,
  ): Promise<ProjectObject[]> =>
    invoke('mesh_deform_selection', {
      objectIds,
      sourceBounds,
      handles,
      gridSize,
      perspective,
    }),

  normalizeForPlanner: (objectIds: string[]): Promise<NormalizedVector[]> =>
    invoke('normalize_for_planner', { objectIds }),

  booleanIntersection: (objectIdA: string, objectIdB: string): Promise<ProjectObject> =>
    invoke('boolean_intersection', { objectIdA, objectIdB }),

  booleanWeld: (objectIds: string[]): Promise<ProjectObject> =>
    invoke('boolean_weld', { objectIds }),

  offsetShapes: (
    objectIds: string[],
    distance: number,
    direction: OffsetDirection,
    cornerStyle?: OffsetCornerStyle,
    deleteOriginal?: boolean,
  ): Promise<ProjectObject[]> =>
    invoke('offset_shapes', {
      objectIds,
      distance,
      direction,
      cornerStyle: cornerStyle ?? 'miter',
      deleteOriginal: deleteOriginal ?? false,
    }),

  previewOffsetShapes: (
    objectIds: string[],
    distance: number,
    direction: OffsetDirection,
    cornerStyle?: OffsetCornerStyle,
  ): Promise<OffsetPreview> =>
    invoke('preview_offset_shapes', {
      objectIds,
      distance,
      direction,
      cornerStyle: cornerStyle ?? 'miter',
    }),

  closePath: (objectId: string): Promise<ProjectObject> =>
    invoke('close_path', { objectId }),

  closePathsWithTolerance: (paths: string[], tolerance: number): Promise<string[]> =>
    invoke('close_paths_with_tolerance', { paths, tolerance }),

  cutShapesApply: (objectIds: string[]): Promise<CutShapesApplyResult> =>
    invoke('cut_shapes_apply', { objectIds }),

  cutShapes: (objectIds: string[]): Promise<string[]> =>
    invoke('cut_shapes', { objectIds }),

  trimShape: (
    clickX: number,
    clickY: number,
    edgeThresholdMm: number,
    heal?: boolean,
  ): Promise<{ objects: ProjectObject[]; healFailed: boolean; openResult: boolean }> =>
    invoke('trim_shape', { clickX, clickY, edgeThresholdMm, heal }),

  previewTrimSegment: (
    clickX: number,
    clickY: number,
    edgeThresholdMm: number,
  ): Promise<{ segmentPoints: [number, number][] } | null> =>
    invoke('preview_trim_segment', { clickX, clickY, edgeThresholdMm }),

  closeAndJoin: (
    objectIds: string[],
    tolerance?: number,
  ): Promise<{ object: ProjectObject; fullyClosed: boolean }> =>
    invoke('close_and_join', { objectIds, tolerance: tolerance ?? 0.1 }),

  breakApart: (objectId: string): Promise<ProjectObject[]> =>
    invoke('break_apart', { objectId }),

  gridArray: (params: {
    objectIds: string[];
    rows: number;
    cols: number;
    sizingModeX?: GridArraySizingMode;
    sizingModeY?: GridArraySizingMode;
    totalWidthMm?: number;
    totalHeightMm?: number;
    hSpacingMm: number;
    vSpacingMm: number;
    spacingMode?: GridSpacingMode;
    mirrorAlternateCols?: boolean;
    mirrorAlternateRows?: boolean;
    xColShiftMm?: number;
    yRowShiftMm?: number;
    halfShift?: boolean;
    reverseH?: boolean;
    reverseV?: boolean;
    randomOrientation?: boolean;
    randomSeed?: number;
    groupResults?: boolean;
    createVirtual?: boolean;
    autoIncrementText?: boolean;
    textIncrement?: number;
  }): Promise<ArrayResult> =>
    invoke('grid_array', params),

  circularArray: (params: {
    objectIds: string[];
    count: number;
    radiusMm: number;
    rotateCopies?: boolean;
    centerX?: number;
    centerY?: number;
    centerObjectId?: string;
    startAngleDeg?: number;
    endAngleDeg?: number;
    groupResults?: boolean;
    createVirtual?: boolean;
    autoIncrementText?: boolean;
    textIncrement?: number;
  }): Promise<ArrayResult> =>
    invoke('circular_array', params),

  copyAlongPathBatch: (
    objectIds: string[],
    pathObjectId: string,
    options: CopyAlongPathOptions,
  ): Promise<ProjectObject[]> =>
    invoke('copy_along_path_batch', {
      objectIds,
      pathObjectId,
      count: options.count,
      rotate: options.rotateCopies,
      scaleCopies: options.scaleCopies,
      finalScalePercent: options.finalScalePercent,
    }),

  copyAlongPath: (
    objectId: string,
    pathObjectId: string,
    options: CopyAlongPathOptions,
  ): Promise<ProjectObject[]> =>
    invoke('copy_along_path', {
      objectId,
      pathObjectId,
      count: options.count,
      rotate: options.rotateCopies,
      scaleCopies: options.scaleCopies,
      finalScalePercent: options.finalScalePercent,
    }),

  unlinkVirtualClone: (objectId: string): Promise<ProjectObject> =>
    invoke('unlink_virtual_clone', { objectId }),

  rubberBandOutline: (objectIds: string[]): Promise<ProjectObject> =>
    invoke('rubber_band_outline', { objectIds }),

  applyPathToText: (textObjectId: string, pathObjectId: string): Promise<ProjectObject> =>
    invoke('apply_path_to_text', { textObjectId, pathObjectId }),

  cropImage: (imageObjectId: string, maskObjectId: string): Promise<ProjectObject> =>
    invoke('crop_image', { imageObjectId, maskObjectId }),

  applyMaskToImage: (imageObjectId: string, maskObjectId: string): Promise<ProjectObject> =>
    invoke('apply_mask_to_image', { imageObjectId, maskObjectId }),

  assignImageMask: (
    imageObjectId: string,
    maskObjectIds: string[],
    polarity: ImageMaskPolarity,
  ): Promise<ProjectObject> =>
    invoke('assign_image_mask', { imageObjectId, maskObjectIds, polarity }),

  setImageMaskPolarity: (
    imageObjectId: string,
    maskObjectId: string,
    polarity: ImageMaskPolarity,
  ): Promise<ProjectObject> =>
    invoke('set_image_mask_polarity', { imageObjectId, maskObjectId, polarity }),

  removeImageMask: (imageObjectId: string, maskObjectId?: string): Promise<ProjectObject> =>
    invoke('remove_image_mask', { imageObjectId, maskObjectId }),

  convertToBitmap: (objectId: string, dpi: number): Promise<ProjectObject> =>
    invoke('convert_to_bitmap', { objectId, dpi }),

  addTabs: (objectId: string, count: number, widthMm: number): Promise<ProjectObject> =>
    invoke('add_tabs', { objectId, count, widthMm }),

  placeTab: (objectId: string, worldX: number, worldY: number): Promise<ProjectObject> =>
    invoke('place_tab', { objectId, worldX, worldY }),

  removeTab: (objectId: string, worldX: number, worldY: number): Promise<ProjectObject> =>
    invoke('remove_tab', { objectId, worldX, worldY }),

  resolveTabMarkers: (objectId: string): Promise<{ subpathIndex: number; position: number; worldX: number; worldY: number }[]> =>
    invoke('resolve_tab_markers', { objectId }),

  applyRadius: (objectId: string, radiusMm: number): Promise<ProjectObject> =>
    invoke('apply_radius', { objectId, radiusMm }),

  getFilletCandidates: (objectId: string): Promise<{ subpathIndex: number; vertexIndex: number; x: number; y: number; alreadyFilleted: boolean }[]> =>
    invoke('get_fillet_candidates', { objectId }),

  applyCornerRadius: (objectId: string, subpathIndex: number, vertexIndex: number, radiusMm: number): Promise<ProjectObject> =>
    invoke('apply_corner_radius', { objectId, subpathIndex, vertexIndex, radiusMm }),

  setStartPoint: (objectId: string, x: number, y: number, mode?: StartPointMode): Promise<ProjectObject> =>
    invoke('set_start_point', { objectId, x, y, mode }),

  getPathVertices: (objectId: string): Promise<Array<{
    subpathIndex: number; vertexIndex: number;
    x: number; y: number;
    isStart: boolean; subpathClosed: boolean;
  }>> =>
    invoke('get_path_vertices', { objectId }),
};
