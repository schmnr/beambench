import { invoke } from '@tauri-apps/api/core';
import type {
  Project,
  Layer,
  CutEntry,
  CutEntryPatch,
  CutEntryTemplate,
  LayerBatchToggle,
  LayerPatch,
  ProjectObject,
  ObjectData,
  Bounds,
  Transform2D,
  RasterSettings,
  StartFromMode,
  AnchorPoint,
  TransformLocks,
  OperationType,
  AlignmentType,
  DistributionDirection,
  MoveTogetherAxis,
  SameSizeAxis,
  DockDirection,
  DockOptions,
  NestOptions,
  NestResult,
  ResizeSlotsOptions,
  FlipAxis,
  ProjectOptimization,
  ProjectOptimizationPatch,
} from '../types/project';
import { normalizeProjectRulerGuides } from '../utils/rulerGuides';

function primaryEntryOf(layer: Layer): CutEntry {
  const entries = layer.entries ?? [];
  const isToolLayer = layer.is_tool_layer;
  return entries[0] ?? {
    id: `${layer.id}:primary`,
    operation: isToolLayer ? 'tool' : 'line',
    speed_mm_min: isToolLayer ? 0 : 1000,
    power_percent: isToolLayer ? 0 : 50,
    raster_settings: null,
    vector_settings: null,
    air_assist: false,
    power_min_percent: 0,
    z_offset_mm: 0,
    gcode_prefix: '',
    gcode_suffix: '',
    output_enabled: !isToolLayer,
  };
}

function decorateLayer(layer: Layer): Layer {
  const entries = layer.entries ?? [];
  return {
    ...layer,
    entries: entries.length > 0 ? entries : [primaryEntryOf(layer)],
  };
}

function decorateProject(project: Project | null): Project | null {
  if (!project) return null;
  return normalizeProjectRulerGuides({
    ...project,
    layers: project.layers.map(decorateLayer),
  });
}

/** Convert top-level snake_case keys to camelCase for Tauri invoke. */
function snakeToCamel(obj: Record<string, unknown>): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(obj)) {
    if (value === undefined) continue;
    const camelKey = key.replace(/_([a-z])/g, (_, c: string) => c.toUpperCase());
    result[camelKey] = value;
  }
  return result;
}

type RawNestResult = NestResult & {
  target_container_id?: string;
  placed_object_ids?: string[];
  unplaced_object_ids?: string[];
  elapsed_ms?: number;
};

function normalizeNestResult(result: RawNestResult): NestResult {
  if (!result || typeof result !== 'object') {
    throw new Error('Nest Selected returned an invalid result.');
  }
  const placedObjectIds = result.placedObjectIds ?? result.placed_object_ids;
  const unplacedObjectIds = result.unplacedObjectIds ?? result.unplaced_object_ids;
  if (!Array.isArray(placedObjectIds) || !Array.isArray(unplacedObjectIds)) {
    throw new Error('Nest Selected returned an invalid result.');
  }
  return {
    targetContainerId: result.targetContainerId ?? result.target_container_id ?? '',
    placedObjectIds,
    unplacedObjectIds,
    utilization: result.utilization ?? 0,
    elapsedMs: result.elapsedMs ?? result.elapsed_ms ?? 0,
  };
}

export interface UndoState {
  can_undo: boolean;
  can_redo: boolean;
}

export interface AddObjectAtomicResult {
  object: ProjectObject;
  createdLayer?: Layer | null;
}

export type DrawOrderDirection = 'forward' | 'backward' | 'front' | 'back';

export const projectService = {
  async createProject(name: string): Promise<Project> {
    return decorateProject(await invoke<Project>('create_project', { name }))!;
  },

  async getProject(): Promise<Project | null> {
    return decorateProject(await invoke<Project | null>('get_project'));
  },

  async closeProject(): Promise<void> {
    return invoke<void>('close_project');
  },

  async getProjectLayers(): Promise<Layer[]> {
    return (await invoke<Layer[]>('get_project_layers')).map(decorateLayer);
  },

  async addLayer(name: string, operation: OperationType): Promise<Layer> {
    return decorateLayer(await invoke<Layer>('add_layer', { name, operation }));
  },

  async updateLayer(
    layerId: string,
    updates: LayerPatch,
  ): Promise<Layer> {
    return decorateLayer(await invoke<Layer>('update_layer', {
      layerId,
      patch: updates,
    }));
  },

  async removeLayer(layerId: string): Promise<void> {
    return invoke<void>('remove_layer', { layerId });
  },

  async reorderLayer(layerId: string, newIndex: number): Promise<Layer[]> {
    return (await invoke<Layer[]>('reorder_layer', { layerId, newIndex })).map(decorateLayer);
  },

  async getProjectObjects(): Promise<ProjectObject[]> {
    return invoke<ProjectObject[]>('get_project_objects');
  },

  async addObject(
    name: string,
    layerId: string,
    objectData: ObjectData,
    bounds: Bounds,
  ): Promise<ProjectObject> {
    return invoke<ProjectObject>('add_object', {
      name,
      layerId,
      objectData,
      bounds,
    });
  },

  async addObjectAtomic(
    name: string,
    layerId: string,
    objectData: ObjectData,
    bounds: Bounds,
    createLayer?: {
      name: string;
      operation: OperationType;
      color_tag?: string;
      /**
       * Optional field-level overrides applied to the minted first entry.
       * Clients never supply a `CutEntry` or `CutEntryId` — the backend
       * mints the entry (and its id) via `Layer::new_single_entry`.
       */
      entry_patch?: CutEntryPatch;
    },
  ): Promise<AddObjectAtomicResult> {
    const result = await invoke<AddObjectAtomicResult>('add_object_atomic', {
      name,
      layerId,
      objectData,
      bounds,
      createLayerName: createLayer?.name,
      createLayerColorTag: createLayer?.color_tag,
      createLayerOperation: createLayer?.operation,
      createLayerEntryPatch: createLayer?.entry_patch,
    });
    return {
      ...result,
      createdLayer: result.createdLayer ? decorateLayer(result.createdLayer) : result.createdLayer,
    };
  },

  async addCutEntry(layerId: string, afterEntryId?: string | null): Promise<CutEntry> {
    return invoke<CutEntry>('add_cut_entry', {
      layerId,
      afterEntryId: afterEntryId ?? null,
    });
  },

  async removeCutEntry(layerId: string, entryId: string): Promise<void> {
    return invoke<void>('remove_cut_entry', { layerId, entryId });
  },

  async reorderCutEntry(layerId: string, entryId: string, newIndex: number): Promise<Layer> {
    return decorateLayer(await invoke<Layer>('reorder_cut_entry', { layerId, entryId, newIndex }));
  },

  async updateCutEntry(layerId: string, entryId: string, patch: CutEntryPatch): Promise<CutEntry> {
    return invoke<CutEntry>('update_cut_entry', {
      layerId,
      entryId,
      patch,
    });
  },

  async updateObject(
    objectId: string,
    updates: {
      name?: string;
      visible?: boolean;
      locked?: boolean;
      layer_id?: string;
      transform?: Transform2D;
      bounds?: Bounds;
      lock_aspect_ratio?: boolean;
      power_scale?: number;
      priority?: number;
    },
  ): Promise<ProjectObject> {
    return invoke<ProjectObject>('update_object', {
      objectId,
      ...snakeToCamel(updates as Record<string, unknown>),
    });
  },

  async updateObjectData(objectId: string, data: ObjectData): Promise<ProjectObject> {
    return invoke<ProjectObject>('update_object_data', { objectId, data });
  },

  async advanceAutoVariableText(): Promise<ProjectObject[]> {
    return invoke<ProjectObject[]>('advance_auto_variable_text');
  },

  // atomic apply for the Adjust Image dialog.
  async applyAdjustImageDialog(
    objectId: string,
    adjustments: unknown,
    layerId: string,
    rasterSettings: RasterSettings,
  ): Promise<{ object: ProjectObject; layer: Layer }> {
    return invoke<{ object: ProjectObject; layer: Layer }>('apply_adjust_image_dialog', {
      objectId,
      adjustments,
      layerId,
      rasterSettings,
    });
  },

  async resizeShapeObject(objectId: string, bounds: Bounds): Promise<ProjectObject> {
    return invoke<ProjectObject>('resize_shape_object', { objectId, bounds });
  },

  async setTextGuidePath(textId: string, guidePathId: string | null): Promise<ProjectObject> {
    return invoke<ProjectObject>('set_text_guide_path', { textId, guidePathId });
  },

  async removeObject(objectId: string): Promise<void> {
    return invoke<void>('remove_object', { objectId });
  },

  async removeObjects(objectIds: string[]): Promise<number> {
    return invoke<number>('remove_objects', { objectIds });
  },

  async nudgeObjects(objectIds: string[], dx: number, dy: number): Promise<void> {
    return invoke<void>('nudge_objects', { objectIds, dx, dy });
  },

  async duplicateObject(objectId: string): Promise<ProjectObject> {
    return invoke<ProjectObject>('duplicate_object', { objectId });
  },

  async duplicateObjects(objectIds: string[]): Promise<ProjectObject[]> {
    return invoke<ProjectObject[]>('duplicate_objects', { objectIds });
  },

  async duplicateObjectInPlace(objectId: string): Promise<ProjectObject> {
    return invoke<ProjectObject>('duplicate_object_in_place', { objectId });
  },

  async duplicateObjectsInPlace(objectIds: string[]): Promise<ProjectObject[]> {
    return invoke<ProjectObject[]>('duplicate_objects_in_place', { objectIds });
  },

  async pasteObjects(objects: ProjectObject[], inPlace: boolean): Promise<ProjectObject[]> {
    return invoke<ProjectObject[]>('paste_objects', { objects, inPlace });
  },

  async alignObjects(
    objectIds: string[],
    alignmentType: AlignmentType,
    anchorObjectId?: string | null,
  ): Promise<ProjectObject[]> {
    return invoke<ProjectObject[]>('align_objects', { objectIds, alignmentType, anchorObjectId: anchorObjectId ?? null });
  },

  async distributeObjects(objectIds: string[], direction: DistributionDirection): Promise<ProjectObject[]> {
    return invoke<ProjectObject[]>('distribute_objects', { objectIds, direction });
  },

  async moveObjectsTogether(
    objectIds: string[],
    axis: MoveTogetherAxis,
    anchorObjectId: string,
  ): Promise<ProjectObject[]> {
    return invoke<ProjectObject[]>('move_objects_together', { objectIds, axis, anchorObjectId });
  },

  async dockObjects(
    objectIds: string[],
    direction: DockDirection,
    options: DockOptions,
  ): Promise<ProjectObject[]> {
    return invoke<ProjectObject[]>('dock_objects', { objectIds, direction, options });
  },

  async nestSelected(objectIds: string[], options: NestOptions): Promise<NestResult> {
    return normalizeNestResult(await invoke<RawNestResult>('nest_selected', { selectedIds: objectIds, options }));
  },

  async mirrorAcrossLine(objectIds: string[], axisObjectId: string): Promise<ProjectObject[]> {
    return invoke<ProjectObject[]>('mirror_across_line', { objectIds, axisObjectId });
  },

  async makeSameSize(
    objectIds: string[],
    anchorObjectId: string,
    axis: SameSizeAxis,
    preserveAspect: boolean,
  ): Promise<ProjectObject[]> {
    return invoke<ProjectObject[]>('make_same_size', {
      objectIds,
      anchorObjectId,
      axis,
      preserveAspect,
    });
  },

  async resizeSlots(objectIds: string[], options: ResizeSlotsOptions): Promise<ProjectObject[]> {
    return invoke<ProjectObject[]>('resize_slots', { objectIds, options });
  },

  async bindMachineProfile(): Promise<Project> {
    return invoke<Project>('bind_machine_profile');
  },

  async getUndoState(): Promise<UndoState> {
    return invoke<UndoState>('get_undo_state');
  },

  async undoProject(): Promise<Project> {
    return invoke<Project>('undo_project');
  },

  async redoProject(): Promise<Project> {
    return invoke<Project>('redo_project');
  },

  async replaceProject(project: Project): Promise<void> {
    return invoke<void>('replace_project', { project });
  },

  async setLayerVisible(layerId: string, visible: boolean): Promise<boolean> {
    return invoke<boolean>('set_layer_visible', { layerId, visible });
  },

  async setLayerAirAssist(layerId: string, airAssist: boolean): Promise<boolean> {
    return invoke<boolean>('set_layer_air_assist', { layerId, airAssist });
  },

  async pushDrawOrder(objectId: string, direction: DrawOrderDirection): Promise<void> {
    return invoke<void>('push_draw_order', { objectId, direction });
  },

  async lockObjects(objectIds: string[]): Promise<void> {
    return invoke<void>('lock_objects', { objectIds });
  },

  async unlockObjects(objectIds: string[]): Promise<void> {
    return invoke<void>('unlock_objects', { objectIds });
  },

  async flipObjects(
    objectIds: string[],
    axis: FlipAxis,
    pivot?: { x: number; y: number },
  ): Promise<void> {
    const payload: {
      objectIds: string[];
      horizontal: boolean;
      pivotX?: number;
      pivotY?: number;
    } = {
      objectIds,
      horizontal: axis === 'horizontal',
    };
    if (pivot) {
      payload.pivotX = pivot.x;
      payload.pivotY = pivot.y;
    }
    return invoke<void>('flip_objects', payload);
  },

  async rotateObjects(
    objectIds: string[],
    degrees: number,
    pivot?: { x: number; y: number },
  ): Promise<void> {
    return invoke<void>('rotate_objects', {
      objectIds,
      degrees,
      pivotX: pivot?.x,
      pivotY: pivot?.y,
    });
  },

  async rotateObjectsAndBakeActivePath(
    objectIds: string[],
    degrees: number,
    pivot: { x: number; y: number } | undefined,
    activeObjectId: string,
  ): Promise<ProjectObject> {
    return invoke<ProjectObject>('rotate_objects_and_bake_active_path', {
      objectIds,
      degrees,
      pivotX: pivot?.x,
      pivotY: pivot?.y,
      activeObjectId,
    });
  },

  async shearObjects(
    objectIds: string[],
    shearX: number,
    shearY: number,
    pivot?: { x: number; y: number },
  ): Promise<void> {
    await invoke('shear_objects', {
      objectIds,
      shearX,
      shearY,
      pivotX: pivot?.x,
      pivotY: pivot?.y,
    });
  },

  async updateObjectBoundsBatch(entries: { id: string; bounds: Bounds }[]): Promise<void> {
    await invoke('update_object_bounds_batch', { entries });
  },

  async moveObjectsTo(objectIds: string[], x: number, y: number): Promise<void> {
    return invoke<void>('move_objects_to', { objectIds, x, y });
  },

  async setStartFrom(mode: StartFromMode): Promise<void> {
    await invoke('set_start_from', { mode });
  },

  async setJobOrigin(anchor: AnchorPoint): Promise<void> {
    await invoke('set_job_origin', { anchor });
  },

  async setUserOrigin(x: number, y: number): Promise<void> {
    await invoke('set_user_origin', { x, y });
  },

  /**
   * Merge a partial `ProjectOptimizationPatch` onto `project.optimization`.
   *
   * Only fields present on `patch` cross IPC — undefined keys are
   * dropped, matching the Rust `#[serde(skip_serializing_if = "Option::is_none")]`
   * contract. The server-side merge via `ProjectOptimization::apply_patch`
   * is where the actual field-by-field change decision happens (no-op
   * short-circuit, undo snapshot, plan cache invalidation).
   */
  async setOptimization(patch: ProjectOptimizationPatch): Promise<ProjectOptimization> {
    const filtered: Record<string, unknown> = {};
    for (const [key, value] of Object.entries(patch)) {
      if (value !== undefined) filtered[key] = value;
    }
    return invoke<ProjectOptimization>('set_optimization', { patch: filtered });
  },

  async updateProjectNotes(notes: string): Promise<void> {
    await invoke('update_project_notes', { notes });
  },

  /** M3: set the project's material thickness (used as Focus Test absolute-Z reference). */
  async setMaterialHeight(value: number | null): Promise<void> {
    await invoke('set_material_height', { value });
  },

  // ---------------------------------------------------------------------------
  // M4 — Layer/Cut Workflow Polish
  // ---------------------------------------------------------------------------

  /** M4: replace a layer's `entries[]` from a clipboard template (Copy/Paste settings). */
  async pasteLayerEntries(layerId: string, entries: CutEntryTemplate[]): Promise<Layer> {
    return invoke<Layer>('paste_layer_entries', { layerId, entries });
  },

  /** M4: reset a single cut entry to built-in defaults for its operation. Preserves entry id. */
  async resetCutEntryToDefaults(layerId: string, entryId: string): Promise<CutEntry> {
    return invoke<CutEntry>('reset_cut_entry_to_defaults', { layerId, entryId });
  },

  /** M4: batch toggle Layer.enabled across all layers. */
  async setAllLayersEnabled(mode: LayerBatchToggle): Promise<Layer[]> {
    return invoke<Layer[]>('set_all_layers_enabled', { mode });
  },

  /** M4: batch toggle Layer.visible across all layers. */
  async setAllLayersVisible(mode: LayerBatchToggle): Promise<Layer[]> {
    return invoke<Layer[]>('set_all_layers_visible', { mode });
  },

  /** M4: re-stamp every layer's order_index per the cut-strength heuristic. */
  async sortLayersCutLast(): Promise<Layer[]> {
    return invoke<Layer[]>('sort_layers_cut_last');
  },

  async setTransformLocks(locks: TransformLocks): Promise<void> {
    await invoke('set_transform_locks', { locks });
  },

  async setObjectsVisible(objectIds: string[], visible: boolean): Promise<void> {
    await invoke('set_objects_visible', { objectIds, visible });
  },

  async reassignLayer(objectIds: string[], layerId: string): Promise<void> {
    return invoke<void>('reassign_layer', { objectIds, targetLayerId: layerId });
  },

  async selectOpenShapes(): Promise<string[]> {
    return invoke<string[]>('select_open_shapes');
  },

  async selectOpenShapesSetToFill(): Promise<string[]> {
    return invoke<string[]>('select_open_shapes_set_to_fill');
  },

  async deleteDuplicates(objectIds: string[]): Promise<string[]> {
    return invoke<string[]>('delete_duplicates', { objectIds });
  },

  async countDuplicates(objectIds: string[]): Promise<number> {
    return invoke<number>('count_duplicates', { objectIds });
  },

  async autoJoinShapes(objectIds: string[], tolerance: number): Promise<string[]> {
    return invoke<string[]>('auto_join_shapes', { objectIds, tolerance });
  },

  async optimizeShapes(objectIds: string[], tolerance = 0.1): Promise<string[]> {
    return invoke<string[]>('optimize_shapes', { objectIds, tolerance });
  },

  async selectAllInLayer(layerId: string): Promise<string[]> {
    return invoke<string[]>('select_all_in_layer', { layerId });
  },

  async selectContainedShapes(objectId: string): Promise<string[]> {
    return invoke<string[]>('select_contained_shapes', { objectId });
  },

  async selectShapesSmallerThanSelected(objectIds: string[]): Promise<string[]> {
    return invoke<string[]>('select_shapes_smaller_than_selected', { objectIds });
  },

  async countOpenPathsWithTolerance(
    objectIds: string[],
    tolerance: number,
    mode: 'move_ends_together' | 'join_with_line',
  ): Promise<{ openShapesFound: number; shapesClosed: number; remainingOpen: number; objectIds: string[] }> {
    return invoke('count_open_paths_with_tolerance', { objectIds, tolerance, mode });
  },

  async closeSelectedPathsWithTolerance(
    objectIds: string[],
    tolerance: number,
    mode: 'move_ends_together' | 'join_with_line',
  ): Promise<{ openShapesFound: number; shapesClosed: number; remainingOpen: number; objectIds: string[] }> {
    return invoke('close_selected_paths_with_tolerance', { objectIds, tolerance, mode });
  },
};
