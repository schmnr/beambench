import { create } from 'zustand';
import type {
  Layer,
  CutEntry,
  CutEntryPatch,
  CutEntryTemplate,
  LayerBatchToggle,
  LayerPatch,
  Project,
  ProjectObject,
  ObjectData,
  Bounds,
  Point2D,
  Transform2D,
  StartFromMode,
  AnchorPoint,
  TransformLocks,
  FlipAxis,
  MoveTogetherAxis,
  SameSizeAxis,
  DockDirection,
  DockOptions,
  ResizeSlotsOptions,
  ProjectOptimizationPatch,
  ImageMaskPolarity,
  AlignmentType,
  DistributionDirection,
} from '../types/project';
import type {
  CopyAlongPathOptions,
  GridArraySizingMode,
  GridSpacingMode,
  OffsetCornerStyle,
  OffsetDirection,
} from '../types/vector';
import { projectService, type DrawOrderDirection } from '../services/projectService';
import { importService } from '../services/importService';
import { persistenceService } from '../services/persistenceService';
import { previewService } from '../services/previewService';
import { sessionJobOptions } from '../types/jobOptions';
import { vectorService } from '../services/vectorService';
import { PALETTE_COLORS } from '../constants/palette';
import { usePreviewStore } from './previewStore';
import { useNotificationStore } from './notificationStore';
import i18n from '../i18n';
import { wrapBackendError } from '../i18n/errors';
import { useUiStore } from './uiStore';
import { useUndoStore } from './undoStore';
import {
  resolveDestinationLayer,
  AUTO_LAYER_ID,
  type ResolveOutput,
  type NeedsBackendCreate,
} from './layerFamilyResolver';
import { objectContentKind } from '../commands/selectionContext';
import { buildRulerGuideGeometry, normalizeProjectRulerGuides } from '../utils/rulerGuides';
import { expandArrangementSelectionMembers, expandSelectionMembers, normalizeArrangementSelection, normalizeSelectionMembers, resolveArrangementAnchorId } from '../utils/arrangementSelection';
import { findAutoGroupCandidates } from '../utils/autoGroupCandidates';
import { isTransformLocked, notifyObjectLocked, notifyTransformLocked } from '../utils/transformLocks';
import { parsePathData, computePathBBox, mapPathCoordToBounds } from '../canvas/drawObjects';
import { applyAroundCenter, getCombinedBounds, resolveCloneForGeometry } from '../canvas/alignment';

const invalidatePreview = () => usePreviewStore.getState().invalidate();
const notifyError = (msg: string) => useNotificationStore.getState().push(wrapBackendError(msg), 'error');
const NIL_UUID = '00000000-0000-0000-0000-000000000000';
const TOOL1_COLOR = PALETTE_COLORS.find((entry) => entry.name === 'Tool 1')?.hex ?? '#DA0B3F';

function boundsCenter(bounds: Bounds): Point2D {
  return {
    x: (bounds.min.x + bounds.max.x) / 2,
    y: (bounds.min.y + bounds.max.y) / 2,
  };
}

function mergeSelectionAddOrder(previousIds: string[], nextIds: string[]): string[] {
  if (nextIds.length === 0) return [];
  const nextSet = new Set(nextIds);
  const ordered = previousIds.filter((id) => nextSet.has(id));
  const seen = new Set(ordered);
  for (const id of nextIds) {
    if (seen.has(id)) continue;
    seen.add(id);
    ordered.push(id);
  }
  return ordered;
}

function orderBatchForDrawOrderAnchor(objectIds: string[], objects: ProjectObject[]): string[] {
  const drawOrder = new Map(objects.map((object, index) => [object.id, index]));
  return [...objectIds].sort((a, b) => (drawOrder.get(b) ?? -1) - (drawOrder.get(a) ?? -1));
}

function primaryEntryOf(layer: Layer): CutEntry {
  const entries = layer.entries ?? [];
  const entry = entries[0];
  if (entry) return entry;
  const isToolLayer = layer.is_tool_layer;
  return {
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

export function decorateProject(project: Project | null): Project | null {
  if (!project) return null;
  return normalizeProjectRulerGuides({
    ...project,
    layers: project.layers.map(decorateLayer),
  });
}

function revokeCachedAssetUrls(assetCache: Map<string, string>): void {
  for (const url of assetCache.values()) {
    if (typeof URL.revokeObjectURL === 'function') {
      URL.revokeObjectURL(url);
    }
  }
}

function dropCachedAsset(
  assetCache: Map<string, string>,
  assetLoadErrors: Map<string, string>,
  assetId: string | null,
): { assetCache: Map<string, string>; assetLoadErrors: Map<string, string> } {
  if (!assetId) {
    return { assetCache, assetLoadErrors };
  }

  const nextCache = new Map(assetCache);
  const url = nextCache.get(assetId);
  if (url && typeof URL.revokeObjectURL === 'function') {
    URL.revokeObjectURL(url);
  }
  nextCache.delete(assetId);

  const nextErrors = new Map(assetLoadErrors);
  nextErrors.delete(assetId);

  return { assetCache: nextCache, assetLoadErrors: nextErrors };
}

/** Scan project objects for missing fonts and push a warning notification. */
function notifyMissingFonts(project: Project): void {
  const missingFonts = new Set<string>();
  for (const obj of project.objects) {
    if (obj.data?.type === 'text' && obj.data.missing_font) {
      missingFonts.add(obj.data.font_family);
    }
  }
  if (missingFonts.size > 0) {
    const names = [...missingFonts].join(', ');
    useNotificationStore
      .getState()
      .push(i18n.t('notifications.missing_fonts', { names }), 'warning');
  }
}
const refreshUndo = async () => useUndoStore.getState().refresh();
const clearUndo = () => useUndoStore.getState().clear();

function canArrangeObjects(project: Project, objectIds: string[]): boolean {
  const selectedObjects = project.objects.filter((object) => objectIds.includes(object.id));
  if (selectedObjects.some((object) => object.locked)) {
    notifyObjectLocked();
    return false;
  }
  if (isTransformLocked(project.transform_locks, 'position')) {
    notifyTransformLocked('position');
    return false;
  }
  return true;
}

function canPositionObjects(project: Project): boolean {
  if (isTransformLocked(project.transform_locks, 'position')) {
    notifyTransformLocked('position');
    return false;
  }
  return true;
}

function canScaleObjects(project: Project, objectIds: string[]): boolean {
  const selectedObjects = project.objects.filter((object) => objectIds.includes(object.id));
  if (selectedObjects.some((object) => object.locked)) {
    notifyObjectLocked();
    return false;
  }
  if (isTransformLocked(project.transform_locks, 'scale')) {
    notifyTransformLocked('scale');
    return false;
  }
  return true;
}

function canCopyAlongPathObjects(
  project: Project,
  objectIds: string[],
  pathObjectId: string,
  scaleCopies: boolean,
): boolean {
  const involvedIds = [...new Set([...objectIds, pathObjectId])];
  const involvedObjects = project.objects.filter((object) => involvedIds.includes(object.id));
  if (involvedObjects.some((object) => object.locked)) {
    notifyObjectLocked();
    return false;
  }
  if (isTransformLocked(project.transform_locks, 'position')) {
    notifyTransformLocked('position');
    return false;
  }
  if (scaleCopies && isTransformLocked(project.transform_locks, 'scale')) {
    notifyTransformLocked('scale');
    return false;
  }
  return true;
}

function topLevelCreatedSelectionIds(objects: ProjectObject[]): string[] {
  const childIds = new Set<string>();
  for (const object of objects) {
    if (object.data?.type === 'group') {
      for (const childId of object.data.children) {
        childIds.add(childId);
      }
    }
  }
  return objects
    .map((object) => object.id)
    .filter((objectId) => !childIds.has(objectId));
}

function pointLiesOnLine(
  point: { x: number; y: number },
  start: { x: number; y: number },
  end: { x: number; y: number },
): boolean {
  const dx = end.x - start.x;
  const dy = end.y - start.y;
  const length = Math.hypot(dx, dy);
  if (length <= 1e-9) return false;
  const cross = Math.abs((point.x - start.x) * dy - (point.y - start.y) * dx);
  return cross <= Math.max(1e-9, length * 1e-6);
}

function singleStraightSegmentEnd(
  commands: ReturnType<typeof parsePathData>,
): { x: number; y: number } | null {
  const start = commands[0];
  const draw = commands[1];
  if (commands.length !== 2 || start?.type !== 'M' || !draw) return null;
  if (draw.type === 'L') {
    return { x: draw.x, y: draw.y };
  }
  if (
    draw.type === 'C' &&
    draw.x1 != null &&
    draw.y1 != null &&
    draw.x2 != null &&
    draw.y2 != null &&
    pointLiesOnLine({ x: draw.x1, y: draw.y1 }, start, draw) &&
    pointLiesOnLine({ x: draw.x2, y: draw.y2 }, start, draw)
  ) {
    return { x: draw.x, y: draw.y };
  }
  return null;
}

function isEligibleMirrorAxis(project: Project, objectId: string): boolean {
  const candidate = project.objects.find((object) => object.id === objectId);
  if (!candidate) return false;
  const resolved = resolveCloneForGeometry(candidate, project.objects);
  if (resolved.data.type !== 'vector_path' || resolved.data.ruler_guide_axis != null || resolved.data.closed) {
    return false;
  }
  const commands = parsePathData(resolved.data.path_data);
  const startCommand = commands[0];
  const endCommand = singleStraightSegmentEnd(commands);
  if (startCommand?.type !== 'M' || !endCommand) {
    return false;
  }
  const bbox = computePathBBox(commands);
  const boundsWidth = resolved.bounds.max.x - resolved.bounds.min.x;
  const boundsHeight = resolved.bounds.max.y - resolved.bounds.min.y;
  const center = {
    x: (resolved.bounds.min.x + resolved.bounds.max.x) / 2,
    y: (resolved.bounds.min.y + resolved.bounds.max.y) / 2,
  };
  const mapWorld = (x: number, y: number) => applyAroundCenter(
    resolved.transform,
    mapPathCoordToBounds(x, y, bbox, resolved.bounds.min.x, resolved.bounds.min.y, boundsWidth, boundsHeight),
    center,
  );
  const start = mapWorld(startCommand.x, startCommand.y);
  const end = mapWorld(endCommand.x, endCommand.y);
  const dx = end.x - start.x;
  const dy = end.y - start.y;
  return Math.hypot(dx, dy) > 1e-9;
}

function computeMirrorAcrossLineSelection(
  project: Project,
  objectIds: string[],
): { objectIds: string[]; sourceIds: string[]; axisObjectId: string | null } {
  const sourceIds = normalizeArrangementSelection(project, objectIds);
  const axisObjectId = [...objectIds]
    .reverse()
    .find((objectId) => isEligibleMirrorAxis(project, objectId)) ?? null;
  if (!axisObjectId) {
    return { objectIds: sourceIds, sourceIds, axisObjectId: null };
  }
  const commandIds = sourceIds.includes(axisObjectId) ? sourceIds : [...sourceIds, axisObjectId];
  return {
    objectIds: commandIds,
    sourceIds: commandIds.filter((objectId) => objectId !== axisObjectId),
    axisObjectId,
  };
}

/**
 * Execute a `NeedsBackendCreate` request from the layer-family
 * resolver: addLayer + updateLayer(color_tag + speed/power/name
 * inheritance). Returns the new layer with the updated fields
 * pre-applied so callers can splice it into local state without a
 * round-trip refresh.
 */
async function createFamilySiblingLayer(req: NeedsBackendCreate) {
  const created = decorateLayer(await projectService.addLayer(req.suggestedName, req.operation));
  const colorSynced = decorateLayer(await projectService.updateLayer(created.id, { color_tag: req.colorTag }));
  if (colorSynced.is_tool_layer) {
    return colorSynced;
  }
  let entry = primaryEntryOf(colorSynced);
  if (req.copyFrom) {
    const sourceEntry = primaryEntryOf(req.copyFrom);
    entry = await projectService.updateCutEntry(created.id, entry.id, {
      speed_mm_min: sourceEntry.speed_mm_min,
      power_percent: sourceEntry.power_percent,
      power_min_percent: sourceEntry.power_min_percent,
      air_assist: sourceEntry.air_assist,
      z_offset_mm: sourceEntry.z_offset_mm,
      gcode_prefix: sourceEntry.gcode_prefix,
      gcode_suffix: sourceEntry.gcode_suffix,
    });
  }
  const paletteEntry = PALETTE_COLORS.find(
    (c) => c.hex.toLowerCase() === req.colorTag.toLowerCase(),
  );
  return decorateLayer({
    ...colorSynced,
    color_tag: req.colorTag,
    entries: [{ ...entry }],
    is_tool_layer: paletteEntry?.is_tool_layer ?? false,
  });
}

interface ProjectStoreState {
  project: Project | null;
  projectPath: string | null;
  selectedLayerId: string | null;
  selectedObjectIds: string[];
  assetCache: Map<string, string>;
  assetLoadErrors: Map<string, string>;
  loading: boolean;
  booleanPending: boolean;
  pendingPaletteColor: string | null;
  error: string | null;

  createProject: (name: string) => Promise<void>;
  loadProject: (options?: { invalidatePreview?: boolean }) => Promise<void>;
  closeProject: () => Promise<void>;
  addLayer: (name: string, operation: CutEntry['operation']) => Promise<void>;
  updateLayer: (layerId: string, updates: LayerPatch) => Promise<boolean>;
  addCutEntry: (layerId: string, afterEntryId?: string | null) => Promise<void>;
  removeCutEntry: (layerId: string, entryId: string) => Promise<void>;
  reorderCutEntry: (layerId: string, entryId: string, newIndex: number) => Promise<void>;
  updateCutEntry: (layerId: string, entryId: string, patch: CutEntryPatch) => Promise<boolean>;
  removeLayer: (layerId: string) => Promise<void>;
  reorderLayer: (layerId: string, newIndex: number) => Promise<void>;

  // M4 — Layer/Cut Workflow Polish
  copyLayerSettings: (layerId: string) => void;
  pasteLayerSettings: (layerId: string) => Promise<void>;
  resetCutEntryToDefaults: (layerId: string, entryId: string) => Promise<void>;
  setAllLayersEnabled: (mode: LayerBatchToggle) => Promise<void>;
  setAllLayersVisible: (mode: LayerBatchToggle) => Promise<void>;
  sortLayersCutLast: () => Promise<void>;
  selectLayer: (layerId: string | null) => void;
  addObject: (
    name: string,
    layerId: string,
    objectData: ObjectData,
    bounds: Bounds,
  ) => Promise<ProjectObject | null>;
  addRulerGuide: (axis: 'horizontal' | 'vertical', valueMm: number) => Promise<ProjectObject | null>;
  updateObject: (
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
  ) => Promise<boolean>;
  updateObjectData: (objectId: string, data: ObjectData) => Promise<boolean>;
  resizeShapeObject: (objectId: string, bounds: Bounds) => Promise<boolean>;
  removeObject: (objectId: string) => Promise<void>;
  removeObjects: (objectIds: string[]) => Promise<boolean>;
  nudgeObjects: (objectIds: string[], dx: number, dy: number) => Promise<void>;
  selectObjects: (objectIds: string[]) => void;
  selectAllObjects: () => void;
  toggleObjectSelection: (objectId: string) => void;
  duplicateObject: (objectId: string) => Promise<void>;
  duplicateObjectInPlace: (objectId: string) => Promise<void>;
  duplicateObjects: (objectIds: string[]) => Promise<void>;
  duplicateObjectsInPlace: (objectIds: string[]) => Promise<void>;
  pasteObjects: (objects: ProjectObject[], inPlace: boolean) => Promise<void>;
  alignObjects: (objectIds: string[], alignmentType: AlignmentType, anchorObjectId?: string | null) => Promise<void>;
  distributeObjects: (objectIds: string[], direction: DistributionDirection) => Promise<void>;
  setProject: (project: Project) => void;
  applyBackendProjectUpdate: (
    project: Project,
    options?: { selectedObjectIds?: string[]; selectedLayerId?: string | null },
  ) => Promise<void>;
  restoreRecoveredProject: (project: Project) => void;

  importFiles: (layerId?: string) => Promise<void>;
  importFilePaths: (filePaths: string[], layerId?: string) => Promise<void>;
  importFileData: (
    files: { filename: string; dataBase64: string }[],
    layerId?: string,
  ) => Promise<void>;
  importClipboardArtwork: (
    artwork: { dataBase64: string; filename: string; mediaType: string },
    drop?: Point2D | null,
  ) => Promise<ProjectObject[]>;
  saveProject: () => Promise<void>;
  saveProjectAs: () => Promise<void>;
  openProject: () => Promise<void>;
  openProjectFromPath: (filePath: string) => Promise<void>;
  loadAssetData: (assetId: string) => Promise<string>;
  advanceAutoVariableText: () => Promise<boolean>;
  exportGcode: () => Promise<string | null>;

  bindMachineProfile: () => Promise<void>;

  convertToPath: (objectId: string) => Promise<void>;
  booleanUnion: (objectIdA: string, objectIdB: string) => Promise<void>;
  booleanSubtract: (objectIdA: string, objectIdB: string) => Promise<void>;
  booleanExclude: (objectIdA: string, objectIdB: string) => Promise<void>;
  groupObjects: (objectIds: string[]) => Promise<void>;
  autoGroupObjects: (objectIds?: string[]) => Promise<void>;
  ungroupObjects: (groupId: string) => Promise<void>;

  // Batch operations (reload-project pattern)
  lockObjects: (objectIds: string[]) => Promise<void>;
  unlockObjects: (objectIds: string[]) => Promise<void>;
  flipObjects: (objectIds: string[], axis: FlipAxis) => Promise<void>;
  rotateObjects: (
    objectIds: string[],
    degrees: number,
    pivot?: { x: number; y: number },
  ) => Promise<void>;
  rotateObjectsAndBakeActivePath: (
    objectIds: string[],
    degrees: number,
    pivot: { x: number; y: number } | undefined,
    activeObjectId: string,
  ) => Promise<void>;
  shearObjects: (
    objectIds: string[],
    shearX: number,
    shearY: number,
    pivot?: { x: number; y: number },
  ) => Promise<void>;
  setObjectsVisible: (objectIds: string[], visible: boolean) => Promise<void>;
  updateObjectBoundsBatch: (entries: { id: string; bounds: Bounds }[]) => Promise<void>;
  pushDrawOrder: (objectId: string, direction: DrawOrderDirection) => Promise<void>;
  moveObjectsTo: (objectIds: string[], x: number, y: number) => Promise<void>;
  computeDockArrangementSelection: () => string[];
  computeMirrorAcrossLineSelection: () => string[];
  mirrorAcrossLine: () => Promise<void>;
  makeSameSize: (axis: SameSizeAxis, preserveAspect: boolean) => Promise<void>;
  resizeSlots: (objectIds: string[], options: ResizeSlotsOptions) => Promise<boolean>;
  moveObjectsTogether: (axis: MoveTogetherAxis) => Promise<void>;
  dockObjects: (objectIds: string[], direction: DockDirection, options: DockOptions) => Promise<boolean>;
  reassignLayer: (objectIds: string[], layerId: string) => Promise<boolean>;
  countDuplicates: (objectIds: string[]) => Promise<number>;
  deleteDuplicates: (objectIds: string[]) => Promise<void>;
  autoJoinShapes: (objectIds: string[], toleranceMm: number) => Promise<void>;
  optimizeShapes: (objectIds: string[]) => Promise<void>;
  selectOpenShapes: () => Promise<void>;
  selectOpenShapesSetToFill: () => Promise<void>;
  selectAllShapesInCurrentLayer: () => Promise<void>;
  selectContainedShapes: () => Promise<void>;
  selectShapesSmallerThanSelected: () => Promise<void>;
  unlinkVirtualClone: (objectId: string) => Promise<void>;

  // Project-level setters
  setStartFrom: (mode: StartFromMode) => Promise<void>;
  setJobOrigin: (anchor: AnchorPoint) => Promise<void>;
  setUserOrigin: (x: number, y: number) => Promise<void>;
  setOptimization: (patch: ProjectOptimizationPatch) => Promise<void>;
  setMaterialHeight: (value: number | null) => Promise<void>;
  updateProjectNotes: (notes: string) => Promise<boolean>;
  setTransformLocks: (locks: TransformLocks) => Promise<void>;

  setPendingPaletteColor: (color: string | null) => void;

  // Boolean / vector ops
  booleanIntersection: (objectIdA: string, objectIdB: string) => Promise<void>;
  booleanWeld: (objectIds: string[]) => Promise<void>;
  cutShapes: (objectIds: string[]) => Promise<void>;
  closeAndJoin: (
    objectIds: string[],
    toleranceMm?: number,
    options?: { warnIfOpen?: boolean },
  ) => Promise<{ object: ProjectObject; fullyClosed: boolean } | null>;
  offsetShapes: (
    objectIds: string[],
    distanceMm: number,
    direction: OffsetDirection,
    cornerStyle?: OffsetCornerStyle,
    deleteOriginal?: boolean,
  ) => Promise<void>;
  breakApart: (objectId: string) => Promise<void>;
  closePath: (objectId: string) => Promise<void>;
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
  }) => Promise<void>;
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
  }) => Promise<void>;
  addTabs: (objectId: string, count: number, widthMm: number) => Promise<void>;
  placeTab: (objectId: string, worldX: number, worldY: number) => Promise<void>;
  removeTab: (objectId: string, worldX: number, worldY: number) => Promise<void>;
  applyRadius: (objectId: string, radiusMm: number) => Promise<void>;
  applyCornerRadius: (
    objectId: string,
    subpathIndex: number,
    vertexIndex: number,
    radiusMm: number,
  ) => Promise<void>;
  convertToBitmap: (objectId: string, dpi: number) => Promise<void>;
  applyPathToText: (textObjectId: string, pathObjectId: string) => Promise<void>;
  cropImage: (imageObjectId: string, maskObjectId: string) => Promise<void>;
  applyMaskToImage: (imageObjectId: string, maskObjectId: string) => Promise<void>;
  assignImageMask: (imageObjectId: string, maskObjectIds: string[], polarity?: ImageMaskPolarity) => Promise<void>;
  setImageMaskPolarity: (imageObjectId: string, maskObjectId: string, polarity: ImageMaskPolarity) => Promise<void>;
  removeImageMask: (imageObjectId: string, maskObjectId?: string) => Promise<void>;
  closeSelectedPathsWithTolerance: (objectIds: string[], toleranceMm: number, mode: 'move_ends_together' | 'join_with_line') => Promise<void>;
  refreshImage: (objectId: string) => Promise<void>;
  replaceImage: (objectId: string, filePath?: string) => Promise<void>;
  replaceImageToFit: (objectId: string, filePath?: string) => Promise<void>;
  copyAlongPath: (objectIds: string[], pathObjectId: string, options: CopyAlongPathOptions) => Promise<boolean>;
  rubberBandOutline: (objectIds: string[]) => Promise<void>;
}

function resolveSelectedLayerId(
  project: ProjectStoreState['project'],
  preferredLayerId: string | null,
): string | null {
  if (!project || project.layers.length === 0) return null;
  if (preferredLayerId && project.layers.some((layer) => layer.id === preferredLayerId)) {
    return preferredLayerId;
  }
  return project.layers[0]?.id ?? null;
}

function resolveSelectedLayerForObjects(
  project: ProjectStoreState['project'],
  objectIds: string[],
  preferredLayerId: string | null,
): string | null {
  if (!project) return null;
  const existingIds = objectIds.filter((id) => project.objects.some((obj) => obj.id === id));
  if (existingIds.length === 0) {
    return resolveSelectedLayerId(project, preferredLayerId);
  }

  const selectedLayers = existingIds
    .map((id) => project.objects.find((obj) => obj.id === id)?.layer_id ?? null)
    .filter((layerId): layerId is string => Boolean(layerId));

  if (selectedLayers.length === 0) {
    return resolveSelectedLayerId(project, preferredLayerId);
  }

  const uniqueLayers = [...new Set(selectedLayers)];
  if (uniqueLayers.length === 1) {
    return uniqueLayers[0];
  }
  if (preferredLayerId && uniqueLayers.includes(preferredLayerId)) {
    return preferredLayerId;
  }
  return selectedLayers[0];
}

export const useProjectStore = create<ProjectStoreState>((set, get) => ({
  project: null,
  projectPath: null,
  selectedLayerId: null,
  selectedObjectIds: [],
  assetCache: new Map(),
  assetLoadErrors: new Map(),
  loading: false,
  booleanPending: false,
  pendingPaletteColor: null,
  error: null,

  createProject: async (name) => {
    try {
      set({ loading: true, error: null });
      const project = decorateProject(await projectService.createProject(name));
      revokeCachedAssetUrls(get().assetCache);
      set({
        project,
        loading: false,
        selectedLayerId: resolveSelectedLayerId(project, null),
        selectedObjectIds: [],
        projectPath: null,
        assetCache: new Map(),
        assetLoadErrors: new Map(),
        pendingPaletteColor: null,
      });
      clearUndo();
      usePreviewStore.getState().clearPreview();
      // M4: clipboard is project-scoped; clear on project switch.
      useUiStore.getState().setLayerSettingsClipboard(null);
      // New projects start with one ready-to-use layer so the layer tab
      // strip is never empty.
      const fresh = get().project;
      if (fresh && fresh.layers.length === 0) {
        const firstColor = PALETTE_COLORS.find((c) => !c.is_tool_layer);
        if (firstColor) {
          const out = resolveDestinationLayer({
            project: fresh,
            requestedLayerId: null,
            pendingColor: firstColor.hex,
            selectedLayerId: null,
            contentKind: 'non_raster',
          });
          if (out.kind === 'needs_create') {
            await get().addLayer(out.suggestedName, out.operation);
            const layers = get().project?.layers ?? [];
            const created = layers[layers.length - 1];
            if (created) await get().updateLayer(created.id, { color_tag: out.colorTag });
          }
        }
      }
    } catch (e) {
      const msg = String(e);
      set({ error: msg, loading: false });
      notifyError(msg);
    }
  },

  loadProject: async (options) => {
    try {
      set({ loading: true, error: null });
      const previousSelectedLayerId = get().selectedLayerId;
      const previousSelectedObjectIds = get().selectedObjectIds;
      const previousAssetCache = get().assetCache;
      const previousAssetLoadErrors = get().assetLoadErrors;
      const project = decorateProject(await projectService.getProject());
      // Migration: bake tx/ty into bounds for vector_path objects with non-zero transform offset
      if (project) {
        for (const obj of project.objects) {
          if (
            obj.data?.type === 'vector_path' &&
            obj.transform &&
            (obj.transform.tx !== 0 || obj.transform.ty !== 0)
          ) {
            obj.bounds.min.x += obj.transform.tx;
            obj.bounds.min.y += obj.transform.ty;
            obj.bounds.max.x += obj.transform.tx;
            obj.bounds.max.y += obj.transform.ty;
            obj.transform = { a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 0 };
          }
        }
      }
      const nextAssetIds = new Set((project?.assets ?? []).map((asset) => asset.id));
      const nextAssetCache = new Map<string, string>();
      for (const [assetId, url] of previousAssetCache) {
        if (nextAssetIds.has(assetId)) {
          nextAssetCache.set(assetId, url);
        } else {
          revokeCachedAssetUrls(new Map([[assetId, url]]));
        }
      }
      const nextAssetLoadErrors = new Map<string, string>();
      for (const [assetId, message] of previousAssetLoadErrors) {
        if (nextAssetIds.has(assetId)) {
          nextAssetLoadErrors.set(assetId, message);
        }
      }

      set({
        project,
        loading: false,
        assetCache: nextAssetCache,
        assetLoadErrors: nextAssetLoadErrors,
        pendingPaletteColor: null,
        selectedLayerId: resolveSelectedLayerId(project, previousSelectedLayerId),
        selectedObjectIds: project
          ? previousSelectedObjectIds.filter((id) => project.objects.some((obj) => obj.id === id))
          : [],
      });
      if (options?.invalidatePreview) {
        invalidatePreview();
      }
      await refreshUndo();
    } catch (e) {
      const msg = String(e);
      set({ error: msg, loading: false });
      notifyError(msg);
    }
  },

  closeProject: async () => {
    try {
      await projectService.closeProject();
      revokeCachedAssetUrls(get().assetCache);
      set({
        project: null,
        selectedLayerId: null,
        selectedObjectIds: [],
        assetCache: new Map(),
        assetLoadErrors: new Map(),
        pendingPaletteColor: null,
        error: null,
      });
      clearUndo();
      usePreviewStore.getState().clearPreview();
      useUiStore.getState().setLayerSettingsClipboard(null);
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    }
  },

  addLayer: async (name, operation) => {
    try {
      const layer = decorateLayer(await projectService.addLayer(name, operation));
      const { project } = get();
      if (project) {
        set({
          project: { ...project, layers: [...project.layers, layer], dirty: true },
          selectedLayerId: layer.id,
        });
        invalidatePreview();
        await refreshUndo();
      }
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    }
  },

  updateLayer: async (layerId, updates) => {
    try {
      const { project } = get();
      const layer = project?.layers.find((candidate) => candidate.id === layerId) ?? null;
      if (!layer) return false;
      const candidate: Layer = decorateLayer({
        ...layer,
        ...(updates.name !== undefined ? { name: updates.name } : {}),
        ...(updates.enabled !== undefined ? { enabled: updates.enabled } : {}),
        ...(updates.visible !== undefined ? { visible: updates.visible } : {}),
        ...(updates.color_tag !== undefined ? { color_tag: updates.color_tag } : {}),
      });
      if (JSON.stringify(candidate) === JSON.stringify(layer)) {
        return false;
      }
      const updatedLayer = decorateLayer(await projectService.updateLayer(layerId, updates));
      if (project) {
        set({
          project: {
              ...project,
              layers: project.layers.map((l) => (l.id === layerId ? updatedLayer! : l)),
              dirty: true,
          },
        });
        invalidatePreview();
        await refreshUndo();
      }
      return true;
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
      return false;
    }
  },

  addCutEntry: async (layerId, afterEntryId) => {
    try {
      const existingLayer = get().project?.layers.find((layer) => layer.id === layerId);
      if (existingLayer?.is_tool_layer) return;
      const createdEntry = await projectService.addCutEntry(layerId, afterEntryId);
      const { project } = get();
      if (!project) return;
      set({
        project: {
          ...project,
          layers: project.layers.map((layer) => {
            if (layer.id !== layerId) return layer;
            const baseEntries = [...layer.entries];
            const insertIndex = afterEntryId
              ? baseEntries.findIndex((entry) => entry.id === afterEntryId) + 1
              : baseEntries.length;
            baseEntries.splice(Math.max(insertIndex, 0), 0, createdEntry);
            return decorateLayer({ ...layer, entries: baseEntries });
          }),
          dirty: true,
        },
      });
      invalidatePreview();
      await refreshUndo();
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    }
  },

  removeCutEntry: async (layerId, entryId) => {
    try {
      const existingLayer = get().project?.layers.find((layer) => layer.id === layerId);
      if (existingLayer?.is_tool_layer) return;
      await projectService.removeCutEntry(layerId, entryId);
      const { project } = get();
      if (!project) return;
      set({
        project: {
          ...project,
          layers: project.layers.map((layer) =>
            layer.id === layerId
              ? decorateLayer({
                  ...layer,
                  entries: layer.entries.filter((entry) => entry.id !== entryId),
                })
              : layer,
          ),
          dirty: true,
        },
      });
      invalidatePreview();
      await refreshUndo();
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    }
  },

  reorderCutEntry: async (layerId, entryId, newIndex) => {
    try {
      const existingLayer = get().project?.layers.find((layer) => layer.id === layerId);
      if (existingLayer?.is_tool_layer) return;
      const updatedLayer = decorateLayer(
        await projectService.reorderCutEntry(layerId, entryId, newIndex),
      );
      const { project } = get();
      if (!project) return;
      set({
        project: {
          ...project,
          layers: project.layers.map((layer) => (layer.id === layerId ? updatedLayer : layer)),
          dirty: true,
        },
      });
      invalidatePreview();
      await refreshUndo();
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    }
  },

  updateCutEntry: async (layerId, entryId, patch) => {
    try {
      const existingLayer = get().project?.layers.find((layer) => layer.id === layerId);
      if (existingLayer?.is_tool_layer) return true;
      const updatedEntry = await projectService.updateCutEntry(layerId, entryId, patch);
      const { project } = get();
      if (!project) return false;
      set({
        project: {
          ...project,
          layers: project.layers.map((layer) =>
            layer.id === layerId
              ? decorateLayer({
                  ...layer,
                  entries: layer.entries.map((entry) =>
                    entry.id === entryId ? updatedEntry : entry,
                  ),
                })
              : layer,
          ),
          dirty: true,
        },
      });
      invalidatePreview();
      await refreshUndo();
      return true;
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
      return false;
    }
  },

  removeLayer: async (layerId) => {
    try {
      await projectService.removeLayer(layerId);
      const { project, selectedLayerId, selectedObjectIds } = get();
      if (project) {
        const nextLayers = project.layers.filter((l) => l.id !== layerId);
        const removedObjectIds = new Set(
          project.objects.filter((o) => o.layer_id === layerId).map((o) => o.id),
        );
        set({
          project: {
            ...project,
            layers: nextLayers,
            objects: project.objects.filter((o) => o.layer_id !== layerId),
            dirty: true,
          },
          selectedLayerId: resolveSelectedLayerId(
            { ...project, layers: nextLayers },
            selectedLayerId === layerId ? null : selectedLayerId,
          ),
          selectedObjectIds: selectedObjectIds.filter((id) => !removedObjectIds.has(id)),
        });
        invalidatePreview();
        await refreshUndo();
      }
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    }
  },

  reorderLayer: async (layerId, newIndex) => {
    try {
      const layers = await projectService.reorderLayer(layerId, newIndex);
      const { project } = get();
      if (project) {
        set({ project: { ...project, layers: layers.map(decorateLayer), dirty: true } });
        invalidatePreview();
        await refreshUndo();
      }
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    }
  },

  // ---------------------------------------------------------------------------
  // M4 — Layer/Cut Workflow Polish
  // ---------------------------------------------------------------------------

  copyLayerSettings: (layerId) => {
    const { project } = get();
    const layer = project?.layers.find((l) => l.id === layerId);
    if (!layer || layer.is_tool_layer) return;
    // Strip ids; the backend will mint fresh ones on every paste.
    const templates: CutEntryTemplate[] = layer.entries.map((e) => ({
      operation: e.operation,
      speed_mm_min: e.speed_mm_min,
      power_percent: e.power_percent,
      raster_settings: e.raster_settings,
      vector_settings: e.vector_settings,
      air_assist: e.air_assist,
      power_min_percent: e.power_min_percent,
      z_offset_mm: e.z_offset_mm,
      gcode_prefix: e.gcode_prefix,
      gcode_suffix: e.gcode_suffix,
      output_enabled: e.output_enabled,
    }));
    useUiStore.getState().setLayerSettingsClipboard(templates);
  },

  pasteLayerSettings: async (layerId) => {
    const clipboard = useUiStore.getState().layerSettingsClipboard;
    if (!clipboard || clipboard.length === 0) return;
    const target = get().project?.layers.find((layer) => layer.id === layerId);
    if (target?.is_tool_layer) return;
    try {
      const updated = await projectService.pasteLayerEntries(layerId, clipboard);
      const { project } = get();
      if (project) {
        set({
          project: {
            ...project,
            layers: project.layers.map((l) =>
              l.id === updated.id ? decorateLayer(updated) : l,
            ),
            dirty: true,
          },
        });
        invalidatePreview();
        await refreshUndo();
      }
    } catch (e) {
      notifyError(String(e));
    }
  },

  resetCutEntryToDefaults: async (layerId, entryId) => {
    try {
      const existingLayer = get().project?.layers.find((layer) => layer.id === layerId);
      if (existingLayer?.is_tool_layer) return;
      const updatedEntry = await projectService.resetCutEntryToDefaults(layerId, entryId);
      const { project } = get();
      if (project) {
        set({
          project: {
            ...project,
            layers: project.layers.map((l) =>
              l.id === layerId
                ? {
                    ...l,
                    entries: l.entries.map((e) => (e.id === entryId ? updatedEntry : e)),
                  }
                : l,
            ),
            dirty: true,
          },
        });
        invalidatePreview();
        await refreshUndo();
      }
    } catch (e) {
      notifyError(String(e));
    }
  },

  setAllLayersEnabled: async (mode) => {
    try {
      const layers = await projectService.setAllLayersEnabled(mode);
      const { project } = get();
      if (project) {
        set({ project: { ...project, layers: layers.map(decorateLayer), dirty: true } });
        invalidatePreview();
        await refreshUndo();
      }
    } catch (e) {
      notifyError(String(e));
    }
  },

  setAllLayersVisible: async (mode) => {
    try {
      const layers = await projectService.setAllLayersVisible(mode);
      const { project } = get();
      if (project) {
        set({ project: { ...project, layers: layers.map(decorateLayer), dirty: true } });
        invalidatePreview();
        await refreshUndo();
      }
    } catch (e) {
      notifyError(String(e));
    }
  },

  sortLayersCutLast: async () => {
    try {
      const layers = await projectService.sortLayersCutLast();
      const { project } = get();
      if (project) {
        set({ project: { ...project, layers: layers.map(decorateLayer), dirty: true } });
        invalidatePreview();
        await refreshUndo();
      }
    } catch (e) {
      notifyError(String(e));
    }
  },

  selectLayer: (layerId) => {
    set({ selectedLayerId: layerId, pendingPaletteColor: null });
  },

  addObject: async (name, layerId, objectData, bounds) => {
    try {
      // Classify the new object's content type so the layer-family
      // resolver can route it to the correct sibling (image vs
      // non-image). Raster content → image sibling; everything else
      // → non-image sibling. Lazily creates a sibling layer the
      // first time a content type lands on a color.
      const project0 = get().project;
      const contentKind = project0
        ? objectContentKind(objectData, project0.objects)
        : ('non_raster' as const);
      const pending = get().pendingPaletteColor;
      const selectedLayerId = get().selectedLayerId;

      let resolvedLayerId: string = layerId;
      let backendLayerId: string = layerId;
      let createdLayer: Layer | null = null;
      let createLayerSpec:
        | {
            name: string;
            operation: CutEntry['operation'];
            color_tag?: string;
            entry_patch?: CutEntryPatch;
          }
        | undefined;

      if (project0) {
        // First resolver pass — may return a concrete layer id OR
        // a "needs create" request for a sibling layer.
        let resolveOut: ResolveOutput = resolveDestinationLayer({
          project: project0,
          requestedLayerId: layerId,
          pendingColor: pending,
          selectedLayerId,
          contentKind,
        });

        if (resolveOut.kind === 'needs_create') {
          const req = resolveOut as NeedsBackendCreate;
          const sourceEntry = req.copyFrom ? primaryEntryOf(req.copyFrom) : null;
          const entryPatch: CutEntryPatch | undefined = sourceEntry && req.operation !== 'tool'
            ? {
                speed_mm_min: sourceEntry.speed_mm_min,
                power_percent: sourceEntry.power_percent,
                power_min_percent: sourceEntry.power_min_percent,
              }
            : undefined;
          createLayerSpec = {
            name: req.suggestedName,
            operation: req.operation,
            color_tag: req.colorTag,
            entry_patch: entryPatch,
          };

          // Re-resolve against an in-memory snapshot that already
          // contains the freshly-created layer. This path always
          // hits the "match found" branch and returns a concrete
          // id — we never need more than two resolver passes.
          const tempLayer: Layer = decorateLayer({
            id: '__pending_layer__',
            name: req.suggestedName,
            entries: [
              {
                id: '__pending_entry__',
                operation: req.operation as CutEntry['operation'],
                speed_mm_min: req.operation === 'tool' ? 0 : (sourceEntry?.speed_mm_min ?? 1000),
                power_percent: req.operation === 'tool' ? 0 : (sourceEntry?.power_percent ?? 50),
                raster_settings: null,
                vector_settings: null,
                air_assist: sourceEntry?.air_assist ?? false,
                power_min_percent: sourceEntry?.power_min_percent ?? 0,
                z_offset_mm: sourceEntry?.z_offset_mm ?? 0,
                gcode_prefix: sourceEntry?.gcode_prefix ?? '',
                gcode_suffix: sourceEntry?.gcode_suffix ?? '',
                output_enabled: req.operation !== 'tool',
              },
            ],
            enabled: true,
            order_index: project0.layers.length,
            color_tag: req.colorTag,
            visible: true,
            is_tool_layer: req.operation === 'tool' || (req.copyFrom?.is_tool_layer ?? false),
          });
          const projectWithNew: Project = {
            ...project0,
            layers: [...project0.layers, tempLayer],
          };
          resolveOut = resolveDestinationLayer({
            project: projectWithNew,
            requestedLayerId: layerId,
            pendingColor: pending,
            selectedLayerId,
            contentKind,
          });
        }

        if (resolveOut.kind === 'resolved') {
          resolvedLayerId = resolveOut.layerId;
          if (createLayerSpec && resolveOut.layerId === '__pending_layer__') {
            backendLayerId = layerId;
          } else {
            backendLayerId = resolveOut.layerId;
          }
        }
      }

      // Fallback: resolver returned `__auto__` (fresh project / no
      // target color / no selection). Preserve the original
      // auto-create-default-Line behavior so empty projects still
      // work even when the resolver has nothing to go on.
      if (resolvedLayerId === AUTO_LAYER_ID || (get().project?.layers.length ?? 0) === 0) {
        const defaultOp = contentKind === 'raster' ? 'image' : 'line';
        const defaultName = contentKind === 'raster' ? 'Image' : 'Line';
        const defaultColor = PALETTE_COLORS.find((c) => !c.is_tool_layer)?.hex ?? '#000000';
        createLayerSpec = {
          name: defaultName,
          operation: defaultOp,
          color_tag: defaultColor,
        };
      }

      if (
        createLayerSpec &&
        (backendLayerId === AUTO_LAYER_ID || backendLayerId === '__pending_layer__')
      ) {
        backendLayerId = project0?.layers[0]?.id ?? NIL_UUID;
      }

      let createdObject;
      if (createLayerSpec) {
        const result = await projectService.addObjectAtomic(
          name,
          backendLayerId,
          objectData,
          bounds,
          createLayerSpec,
        );
        createdObject = result.object;
        createdLayer = result.createdLayer ? decorateLayer(result.createdLayer) : null;
        if (createdLayer) {
          resolvedLayerId = createdLayer.id;
        }
      } else {
        createdObject = await projectService.addObject(name, resolvedLayerId, objectData, bounds);
      }
      const { project } = get();
      if (project) {
        const nextLayers = createdLayer ? [...project.layers, createdLayer] : project.layers;
        set({
          project: {
            ...project,
            layers: nextLayers,
            objects: [...project.objects, createdObject],
            dirty: true,
          },
          pendingPaletteColor: null,
          selectedLayerId: resolvedLayerId,
          selectedObjectIds: [createdObject.id],
        });
      }
      invalidatePreview();
      await refreshUndo();
      return createdObject;
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
      return null;
    }
  },

  addRulerGuide: async (axis, valueMm) => {
    try {
      const { project } = get();
      if (!project) return null;
      const existingLayer = project.layers.find((layer) => layer.is_tool_layer && layer.color_tag.toLowerCase() === TOOL1_COLOR.toLowerCase());
      const geometry = buildRulerGuideGeometry(axis, valueMm, project.workspace);
      const objectData: ObjectData = {
        type: 'vector_path',
        path_data: geometry.path_data,
        closed: false,
        ruler_guide_axis: axis,
      };
      const bounds: Bounds = geometry.bounds;

      let createdObject: ProjectObject;
      let createdLayer: Layer | null = null;
      if (existingLayer) {
        createdObject = await projectService.addObject('Guide', existingLayer.id, objectData, bounds);
      } else if (project.layers.length === 0) {
        const baseLayer = decorateLayer(await projectService.addLayer('T1', 'line'));
        const toolLayer = decorateLayer(
          await projectService.updateLayer(baseLayer.id, { color_tag: TOOL1_COLOR }),
        );
        createdLayer = toolLayer;
        createdObject = await projectService.addObject('Guide', toolLayer.id, objectData, bounds);
      } else {
        const result = await projectService.addObjectAtomic(
          'Guide',
          project.layers[0].id,
          objectData,
          bounds,
          {
            name: 'T1',
            operation: 'line',
            color_tag: TOOL1_COLOR,
          },
        );
        createdObject = result.object;
        createdLayer = result.createdLayer ? decorateLayer(result.createdLayer) : null;
      }

      const nextLayers = createdLayer ? [...project.layers, createdLayer] : project.layers;
      set({
        project: {
          ...project,
          layers: nextLayers,
          objects: [...project.objects, createdObject],
          dirty: true,
        },
        selectedLayerId: createdLayer?.id ?? existingLayer?.id ?? get().selectedLayerId,
        selectedObjectIds: [createdObject.id],
      });
      invalidatePreview();
      await refreshUndo();
      return createdObject;
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
      return null;
    }
  },

  updateObject: async (objectId, updates) => {
    try {
      const updated = await projectService.updateObject(objectId, updates);
      const { project } = get();
      if (project) {
        set({
          project: {
            ...project,
            objects: project.objects.map((o) => (o.id === objectId ? updated : o)),
            dirty: true,
          },
        });
        invalidatePreview();
        await refreshUndo();
      }
      return true;
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
      return false;
    }
  },

  updateObjectData: async (objectId, data) => {
    try {
      const updated = await projectService.updateObjectData(objectId, data);
      const { project } = get();
      if (project) {
        set({
          project: {
            ...project,
            objects: project.objects.map((o) => (o.id === objectId ? updated : o)),
            dirty: true,
          },
        });
        invalidatePreview();
        await refreshUndo();
      }
      return true;
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
      return false;
    }
  },

  advanceAutoVariableText: async () => {
    try {
      const updatedObjects = await projectService.advanceAutoVariableText();
      if (updatedObjects.length === 0) {
        return false;
      }
      const { project } = get();
      if (!project) return false;
      const updateMap = new Map(updatedObjects.map((object) => [object.id, object]));
      set({
        project: {
          ...project,
          objects: project.objects.map((object) => updateMap.get(object.id) ?? object),
          dirty: true,
        },
      });
      invalidatePreview();
      await refreshUndo();
      return true;
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
      return false;
    }
  },

  resizeShapeObject: async (objectId, bounds) => {
    try {
      const updated = await projectService.resizeShapeObject(objectId, bounds);
      const { project } = get();
      if (project) {
        set({
          project: {
            ...project,
            objects: project.objects.map((o) => (o.id === objectId ? updated : o)),
            dirty: true,
          },
        });
        invalidatePreview();
        await refreshUndo();
      }
      return true;
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
      return false;
    }
  },

  removeObject: async (objectId) => {
    try {
      await projectService.removeObject(objectId);
      const { selectedObjectIds, selectedLayerId } = get();
      const project = await projectService.getProject();
      if (project) {
        set({
      project: decorateProject({ ...project, dirty: true })!,
          selectedObjectIds: selectedObjectIds.filter((id) => id !== objectId),
          selectedLayerId: resolveSelectedLayerId(project, selectedLayerId),
        });
      } else {
        // Backend removed the object but project fetch returned null — clear stale state
        set({ selectedObjectIds: selectedObjectIds.filter((id) => id !== objectId) });
      }
      invalidatePreview();
      await refreshUndo();
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    }
  },

  removeObjects: async (objectIds) => {
    try {
      const currentProject = get().project;
      const targetIds = currentProject
        ? expandSelectionMembers(currentProject, objectIds)
        : objectIds;
      await projectService.removeObjects(targetIds);
      const { selectedObjectIds, selectedLayerId } = get();
      const idSet = new Set(targetIds);
      const project = await projectService.getProject();
      if (project) {
        set({
      project: decorateProject({ ...project, dirty: true })!,
          selectedObjectIds: selectedObjectIds.filter((id) => !idSet.has(id)),
          selectedLayerId: resolveSelectedLayerId(project, selectedLayerId),
        });
      } else {
        // Backend removed objects but project fetch returned null — clear stale state
        set({ selectedObjectIds: selectedObjectIds.filter((id) => !idSet.has(id)) });
      }
      invalidatePreview();
      await refreshUndo();
      return true;
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
      return false;
    }
  },

  nudgeObjects: async (objectIds, dx, dy) => {
    try {
      const currentProject = get().project;
      const targetIds = currentProject
        ? expandArrangementSelectionMembers(currentProject, objectIds)
        : objectIds;
      await projectService.nudgeObjects(targetIds, dx, dy);
      const { project } = get();
      if (project) {
        const idSet = new Set(targetIds);
        set({
          project: {
            ...project,
            objects: project.objects.map((o) =>
              idSet.has(o.id)
                ? {
                    ...o,
                    bounds: {
                      min: { x: o.bounds.min.x + dx, y: o.bounds.min.y + dy },
                      max: { x: o.bounds.max.x + dx, y: o.bounds.max.y + dy },
                    },
                  }
                : o,
            ),
            dirty: true,
          },
        });
        invalidatePreview();
        await refreshUndo();
      }
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    }
  },

  selectObjects: (objectIds) => {
    set((state) => ({
      selectedObjectIds: mergeSelectionAddOrder(
        state.selectedObjectIds,
        state.project ? normalizeSelectionMembers(state.project, objectIds) : objectIds,
      ),
    }));
  },

  selectAllObjects: () => {
    const { project } = get();
    if (project) {
      const objectIds = orderBatchForDrawOrderAnchor(
        normalizeArrangementSelection(project, project.objects.map((o) => o.id)),
        project.objects,
      );
      set((state) => ({
        selectedObjectIds: mergeSelectionAddOrder(state.selectedObjectIds, objectIds),
      }));
    }
  },

  toggleObjectSelection: (objectId) => {
    const project = get().project;
    const selectableId = project
      ? normalizeSelectionMembers(project, [objectId])[0] ?? objectId
      : objectId;
    const { selectedObjectIds } = get();
    if (selectedObjectIds.includes(selectableId)) {
      set({ selectedObjectIds: selectedObjectIds.filter((id) => id !== selectableId) });
    } else {
      set({ selectedObjectIds: [...selectedObjectIds, selectableId] });
    }
  },

  duplicateObject: async (objectId) => {
    await get().duplicateObjects([objectId]);
  },

  duplicateObjectInPlace: async (objectId) => {
    await get().duplicateObjectsInPlace([objectId]);
  },

  duplicateObjects: async (objectIds) => {
    try {
      if (objectIds.length === 0) return;
      const duplicated = await projectService.duplicateObjects(objectIds);
      const { project } = get();
      if (project) {
        const duplicatedIds = duplicated.map((object) => object.id);
        const nextProject = { ...project, objects: [...project.objects, ...duplicated], dirty: true };
        const selectedIds = normalizeSelectionMembers(nextProject, duplicatedIds);
        set({
          project: nextProject,
          selectedObjectIds: selectedIds,
          selectedLayerId: resolveSelectedLayerForObjects(
            nextProject,
            selectedIds,
            get().selectedLayerId,
          ),
        });
        invalidatePreview();
        await refreshUndo();
      }
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    }
  },

  duplicateObjectsInPlace: async (objectIds) => {
    try {
      if (objectIds.length === 0) return;
      const duplicated = await projectService.duplicateObjectsInPlace(objectIds);
      const { project } = get();
      if (project) {
        const duplicatedIds = duplicated.map((object) => object.id);
        const nextProject = { ...project, objects: [...project.objects, ...duplicated], dirty: true };
        const selectedIds = normalizeSelectionMembers(nextProject, duplicatedIds);
        set({
          project: nextProject,
          selectedObjectIds: selectedIds,
          selectedLayerId: resolveSelectedLayerForObjects(
            nextProject,
            selectedIds,
            get().selectedLayerId,
          ),
        });
        invalidatePreview();
        await refreshUndo();
      }
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    }
  },

  pasteObjects: async (objects, inPlace) => {
    try {
      if (objects.length === 0) return;
      const pasted = await projectService.pasteObjects(objects, inPlace);
      const pastedIds = pasted.map((object) => object.id);
      const project = await projectService.getProject();
      if (project) {
        const decorated = decorateProject({ ...project, dirty: true })!;
        const selectedIds = normalizeSelectionMembers(decorated, pastedIds);
        set({
          project: decorated,
          selectedObjectIds: selectedIds,
          selectedLayerId: resolveSelectedLayerForObjects(
            decorated,
            selectedIds,
            get().selectedLayerId,
          ),
        });
      } else {
        set({ selectedObjectIds: pastedIds });
      }
      invalidatePreview();
      await refreshUndo();
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    }
  },

  alignObjects: async (objectIds, alignmentType, anchorObjectId) => {
    try {
      if (objectIds.length < 2) return;
      const currentProject = get().project;
      if (!currentProject || !canPositionObjects(currentProject)) return;
      const normalizedIds = normalizeArrangementSelection(currentProject, objectIds);
      if (normalizedIds.length < 2) return;
      const resolvedAnchor = anchorObjectId ?? resolveArrangementAnchorId(currentProject, objectIds);
      const updatedObjects = await projectService.alignObjects(normalizedIds, alignmentType, resolvedAnchor);
      if (updatedObjects.length === 0) return;
      const { project } = get();
      if (project) {
        const updatedMap = new Map(updatedObjects.map((object) => [object.id, object]));
        set({
          project: {
            ...project,
            objects: project.objects.map((object) => updatedMap.get(object.id) ?? object),
            dirty: true,
          },
        });
        invalidatePreview();
        await refreshUndo();
      }
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    }
  },

  distributeObjects: async (objectIds, direction) => {
    try {
      if (objectIds.length < 3) return;
      const currentProject = get().project;
      if (!currentProject || !canPositionObjects(currentProject)) return;
      const normalizedIds = normalizeArrangementSelection(currentProject, objectIds);
      if (normalizedIds.length < 3) return;
      const updatedObjects = await projectService.distributeObjects(normalizedIds, direction);
      if (updatedObjects.length === 0) return;
      const { project } = get();
      if (project) {
        const updatedMap = new Map(updatedObjects.map((object) => [object.id, object]));
        set({
          project: {
            ...project,
            objects: project.objects.map((object) => updatedMap.get(object.id) ?? object),
            dirty: true,
          },
        });
        invalidatePreview();
        await refreshUndo();
      }
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    }
  },

  setProject: (project) => {
    revokeCachedAssetUrls(get().assetCache);
    set({
      project: decorateProject(project)!,
      selectedLayerId: null,
      selectedObjectIds: [],
      assetCache: new Map(),
      assetLoadErrors: new Map(),
      pendingPaletteColor: null,
    });
    clearUndo();
    usePreviewStore.getState().clearPreview();
    notifyMissingFonts(project);
    void refreshUndo();
  },

  applyBackendProjectUpdate: async (project, options) => {
    const decorated = decorateProject(project)!;
    const selectedObjectIds = (options?.selectedObjectIds ?? [])
      .filter((id) => decorated.objects.some((object) => object.id === id));
    set({
      project: decorated,
      selectedLayerId: resolveSelectedLayerForObjects(
        decorated,
        selectedObjectIds,
        options?.selectedLayerId ?? get().selectedLayerId,
      ),
      selectedObjectIds,
      pendingPaletteColor: null,
      error: null,
    });
    invalidatePreview();
    notifyMissingFonts(project);
    await refreshUndo();
  },

  restoreRecoveredProject: (project) => {
    revokeCachedAssetUrls(get().assetCache);
    set({
      project: decorateProject(project)!,
      projectPath: null,
      selectedLayerId: resolveSelectedLayerId(project, null),
      selectedObjectIds: [],
      assetCache: new Map(),
      assetLoadErrors: new Map(),
      pendingPaletteColor: null,
      loading: false,
      error: null,
    });
    clearUndo();
    usePreviewStore.getState().clearPreview();
    notifyMissingFonts(project);
    void refreshUndo();
  },

  importFilePaths: async (filePaths, layerId) => {
    const artworkPaths = filePaths.filter((path) => !isGcodeImportName(path));
    await importArtworkBatch(artworkPaths, layerId, (resolvedLayerId) =>
      importService.importFilePaths(artworkPaths, resolvedLayerId),
    );
  },

  importFileData: async (files, layerId) => {
    const artworkFiles = files.filter((file) => !isGcodeImportName(file.filename));
    await importArtworkBatch(
      artworkFiles.map((file) => file.filename),
      layerId,
      (resolvedLayerId) => importService.importFileData(artworkFiles, resolvedLayerId),
    );
  },

  importClipboardArtwork: async (artwork, drop) => {
    try {
      let { project } = get();
      if (!project) {
        useNotificationStore.getState().push(i18n.t('notifications.no_project_for_paste'), 'warning');
        return [];
      }

      let layerId = get().selectedLayerId ?? project.layers[0]?.id ?? null;
      if (!layerId) {
        const operation = artwork.mediaType === 'image/svg+xml' ? 'line' : 'image';
        const createdLayer = decorateLayer(await projectService.addLayer(
          operation === 'image' ? 'Image' : 'Line',
          operation,
        ));
        project = { ...project, layers: [...project.layers, createdLayer] };
        layerId = createdLayer.id;
      }

      const importedObjects = await importService.importClipboardArtwork({
        ...artwork,
        layerId,
        ...(drop ? { dropX: drop.x, dropY: drop.y } : {}),
      });
      const refreshed = decorateProject(await projectService.getProject());
      if (refreshed) {
        const destLayerIds: string[] = [];
        const destCounts = new Map<string, number>();
        for (const obj of importedObjects) {
          const destLayerId = obj.layer_id;
          if (!destCounts.has(destLayerId)) destLayerIds.push(destLayerId);
          destCounts.set(destLayerId, (destCounts.get(destLayerId) ?? 0) + 1);
        }
        let selectedLayerId = layerId;
        if ((!selectedLayerId || !destCounts.has(selectedLayerId)) && destLayerIds.length > 0) {
          selectedLayerId = destLayerIds[0];
        }

        set({
          project: { ...refreshed, dirty: true },
          pendingPaletteColor: null,
          selectedLayerId,
          selectedObjectIds: importedObjects.map((obj) => obj.id),
        });
        invalidatePreview();
        await refreshUndo();
      }
      return importedObjects;
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
      return [];
    }
  },

  importFiles: async (layerId) => {
    try {
      const filePaths = await importService.pickFiles();
      if (filePaths.length === 0) return;
      await get().importFilePaths(filePaths, layerId);
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    }
  },

  saveProject: async () => {
    try {
      const { projectPath } = get();
      const savedPath = await persistenceService.saveProject(projectPath ?? undefined);
      set({ projectPath: savedPath });
      // Reload to clear dirty flag
      const project = await projectService.getProject();
      if (project) set({ project });
      await refreshUndo();
    } catch (e) {
      // "Save cancelled" is not an error
      if (!String(e).includes('cancelled')) {
        const msg = String(e);
        set({ error: msg });
        notifyError(msg);
      }
    }
  },

  saveProjectAs: async () => {
    try {
      const savedPath = await persistenceService.saveProjectAs();
      set({ projectPath: savedPath });
      const project = await projectService.getProject();
      if (project) set({ project });
      await refreshUndo();
    } catch (e) {
      if (!String(e).includes('cancelled')) {
        const msg = String(e);
        set({ error: msg });
        notifyError(msg);
      }
    }
  },

  openProject: async () => {
    try {
      set({ loading: true, error: null });
      const { project: rawProject, path } = await persistenceService.openProject();
      const project = decorateProject(rawProject)!;
      revokeCachedAssetUrls(get().assetCache);
      set({
        project,
        projectPath: path,
        loading: false,
        selectedLayerId: resolveSelectedLayerId(project, null),
        selectedObjectIds: [],
        assetCache: new Map(),
        assetLoadErrors: new Map(),
        pendingPaletteColor: null,
      });
      clearUndo();
      usePreviewStore.getState().clearPreview();
      useUiStore.getState().setLayerSettingsClipboard(null);
      notifyMissingFonts(project);
      await refreshUndo();
    } catch (e) {
      if (!String(e).includes('cancelled')) {
        const msg = String(e);
        set({ error: msg, loading: false });
        notifyError(msg);
      } else {
        set({ loading: false });
      }
    }
  },

  openProjectFromPath: async (filePath) => {
    try {
      set({ loading: true, error: null });
      const project = decorateProject(await persistenceService.openProjectFromPath(filePath))!;
      revokeCachedAssetUrls(get().assetCache);
      set({
        project,
        projectPath: filePath,
        loading: false,
        selectedLayerId: resolveSelectedLayerId(project, null),
        selectedObjectIds: [],
        assetCache: new Map(),
        assetLoadErrors: new Map(),
        pendingPaletteColor: null,
      });
      clearUndo();
      usePreviewStore.getState().clearPreview();
      useUiStore.getState().setLayerSettingsClipboard(null);
      notifyMissingFonts(project);
      await refreshUndo();
    } catch (e) {
      const msg = String(e);
      set({ error: msg, loading: false });
      notifyError(msg);
    }
  },

  loadAssetData: async (assetId) => {
    const { assetCache, assetLoadErrors } = get();
    const cached = assetCache.get(assetId);
    if (cached) return cached;
    const cachedError = assetLoadErrors.get(assetId);
    if (cachedError) {
      throw new Error(cachedError);
    }

    try {
      const bytes = await persistenceService.getAssetData(assetId);
      const uint8 = new Uint8Array(bytes);
      const blob = new Blob([uint8]);
      const dataUrl = URL.createObjectURL(blob);

      const newCache = new Map(assetCache);
      newCache.set(assetId, dataUrl);
      const newErrors = new Map(assetLoadErrors);
      newErrors.delete(assetId);
      set({ assetCache: newCache, assetLoadErrors: newErrors });
      return dataUrl;
    } catch (error) {
      const message = String(error);
      const newErrors = new Map(get().assetLoadErrors);
      newErrors.set(assetId, message);
      set({ assetLoadErrors: newErrors, error: message });
      notifyError(message);
      throw error;
    }
  },

  exportGcode: async () => {
    try {
      const { jobOptions } = useUiStore.getState();
      const path = await previewService.exportGcode(
        sessionJobOptions(jobOptions, get().selectedObjectIds),
      );
      await get().advanceAutoVariableText();
      return path;
    } catch (e) {
      const msg = String(e);
      if (msg !== 'Error: Export cancelled' && msg !== 'Export cancelled') {
        set({ error: msg });
        notifyError(msg);
      }
      return null;
    }
  },

  bindMachineProfile: async () => {
    try {
      const updated = decorateProject(await projectService.bindMachineProfile());
      set({ project: updated });
      await refreshUndo();
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    }
  },

  convertToPath: async (objectId) => {
    try {
      const updated = await vectorService.convertToPath(objectId);
      const { project, selectedObjectIds, selectedLayerId } = get();
      if (project) {
        const nextProject = {
          ...project,
          objects: project.objects.map((o) => (o.id === objectId ? updated : o)),
          dirty: true,
        };
        set({
          project: nextProject,
          selectedLayerId: selectedObjectIds.includes(objectId)
            ? resolveSelectedLayerForObjects(nextProject, [updated.id], updated.layer_id)
            : resolveSelectedLayerId(nextProject, selectedLayerId),
        });
        invalidatePreview();
        await refreshUndo();
      }
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    }
  },

  booleanUnion: async (objectIdA, objectIdB) => {
    if (get().booleanPending) return;
    set({ booleanPending: true });
    try {
      const newObj = await vectorService.booleanUnion(objectIdA, objectIdB);
      const project = await projectService.getProject();
      if (project) {
        const nextProject = decorateProject({ ...project, dirty: true })!;
        set({
          project: nextProject,
          selectedLayerId: resolveSelectedLayerForObjects(
            nextProject,
            [newObj.id],
            newObj.layer_id,
          ),
          selectedObjectIds: [newObj.id],
        });
        invalidatePreview();
        await refreshUndo();
      }
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    } finally {
      set({ booleanPending: false });
    }
  },

  booleanSubtract: async (objectIdA, objectIdB) => {
    if (get().booleanPending) return;
    set({ booleanPending: true });
    try {
      const newObj = await vectorService.booleanSubtract(objectIdA, objectIdB);
      const project = await projectService.getProject();
      if (project) {
        const nextProject = decorateProject({ ...project, dirty: true })!;
        set({
          project: nextProject,
          selectedLayerId: resolveSelectedLayerForObjects(
            nextProject,
            [newObj.id],
            newObj.layer_id,
          ),
          selectedObjectIds: [newObj.id],
        });
        invalidatePreview();
        await refreshUndo();
      }
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    } finally {
      set({ booleanPending: false });
    }
  },

  booleanExclude: async (objectIdA, objectIdB) => {
    if (get().booleanPending) return;
    set({ booleanPending: true });
    try {
      const newObj = await vectorService.booleanExclude(objectIdA, objectIdB);
      const project = await projectService.getProject();
      if (project) {
        const nextProject = decorateProject({ ...project, dirty: true })!;
        set({
          project: nextProject,
          selectedLayerId: resolveSelectedLayerForObjects(
            nextProject,
            [newObj.id],
            newObj.layer_id,
          ),
          selectedObjectIds: [newObj.id],
        });
        invalidatePreview();
        await refreshUndo();
      }
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    } finally {
      set({ booleanPending: false });
    }
  },

  groupObjects: async (objectIds) => {
    try {
      const { project } = get();
      const groupObjectIds = project ? normalizeArrangementSelection(project, objectIds) : objectIds;
      if (groupObjectIds.length < 2) return;
      const group = await vectorService.groupObjects(groupObjectIds);
      if (project) {
        set({
          project: {
            ...project,
            objects: [...project.objects, group],
            dirty: true,
          },
          selectedLayerId: group.layer_id,
          selectedObjectIds: [group.id],
        });
        invalidatePreview();
        await refreshUndo();
      }
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    }
  },

  autoGroupObjects: async (objectIds) => {
    try {
      const current = get();
      const project = current.project;
      if (!project) return;
      const selectedIds = objectIds ?? current.selectedObjectIds;
      if (findAutoGroupCandidates(project, selectedIds).length === 0) return;
      const groups = await vectorService.autoGroupObjects(selectedIds);
      if (groups.length === 0) return;
      const nextProject = {
        ...project,
        objects: [...project.objects, ...groups],
        dirty: true,
      };
      const groupIds = groups.map((group) => group.id);
      set({
        project: nextProject,
        selectedLayerId: resolveSelectedLayerForObjects(
          nextProject,
          groupIds,
          groups[0]?.layer_id ?? current.selectedLayerId,
        ),
        selectedObjectIds: groupIds,
      });
      invalidatePreview();
      await refreshUndo();
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    }
  },

  ungroupObjects: async (groupId) => {
    try {
      const childIds = await vectorService.ungroupObjects(groupId);
      const { project } = get();
      if (project) {
        const nextProject = {
          ...project,
          objects: project.objects.filter((o) => o.id !== groupId),
          dirty: true,
        };
        set({
          project: nextProject,
          selectedLayerId: resolveSelectedLayerForObjects(
            nextProject,
            childIds,
            get().selectedLayerId,
          ),
          selectedObjectIds: childIds,
        });
        invalidatePreview();
        await refreshUndo();
      }
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    }
  },

  // --- Batch operations (reload-project pattern) ---

  lockObjects: async (objectIds) => {
    try {
      await projectService.lockObjects(objectIds);
      const project = await projectService.getProject();
      if (project) set({ project: { ...project, dirty: true } });
      invalidatePreview();
      await refreshUndo();
    } catch (e) {
      notifyError(String(e));
    }
  },

  unlockObjects: async (objectIds) => {
    try {
      await projectService.unlockObjects(objectIds);
      const project = await projectService.getProject();
      if (project) set({ project: { ...project, dirty: true } });
      invalidatePreview();
      await refreshUndo();
    } catch (e) {
      notifyError(String(e));
    }
  },

  flipObjects: async (objectIds, axis) => {
    try {
      const currentProject = get().project;
      let targetIds = objectIds;
      let pivot: Point2D | undefined;
      if (currentProject) {
        const rootIds = normalizeArrangementSelection(currentProject, objectIds);
        const rootObjects = rootIds
          .map((id) => currentProject.objects.find((object) => object.id === id))
          .filter((object): object is ProjectObject => Boolean(object));
        const includesGroup = rootObjects.some((object) => object.data.type === 'group');
        if (includesGroup && rootObjects.length > 0) {
          targetIds = expandArrangementSelectionMembers(currentProject, rootIds);
          pivot = boundsCenter(getCombinedBounds(rootObjects.map((object) => object.bounds)));
        }
      }
      if (pivot) {
        await projectService.flipObjects(targetIds, axis, pivot);
      } else {
        await projectService.flipObjects(targetIds, axis);
      }
      const project = await projectService.getProject();
      if (project) set({ project: { ...project, dirty: true } });
      invalidatePreview();
      await refreshUndo();
    } catch (e) {
      notifyError(String(e));
    }
  },

  rotateObjects: async (objectIds, degrees, pivot?) => {
    try {
      const currentProject = get().project;
      let targetIds = objectIds;
      let resolvedPivot = pivot;
      if (currentProject) {
        const rootIds = normalizeArrangementSelection(currentProject, objectIds);
        const rootObjects = rootIds
          .map((id) => currentProject.objects.find((object) => object.id === id))
          .filter((object): object is ProjectObject => Boolean(object));
        const includesGroup = rootObjects.some((object) => object.data.type === 'group');
        if (includesGroup && rootObjects.length > 0) {
          targetIds = expandArrangementSelectionMembers(currentProject, rootIds);
          resolvedPivot = pivot ?? boundsCenter(getCombinedBounds(rootObjects.map((object) => object.bounds)));
        }
      }
      await projectService.rotateObjects(targetIds, degrees, resolvedPivot);
      const project = await projectService.getProject();
      if (project) set({ project: { ...project, dirty: true } });
      invalidatePreview();
      await refreshUndo();
    } catch (e) {
      notifyError(String(e));
    }
  },

  rotateObjectsAndBakeActivePath: async (objectIds, degrees, pivot, activeObjectId) => {
    try {
      const currentProject = get().project;
      let targetIds = objectIds;
      let resolvedPivot = pivot;
      if (currentProject) {
        const rootIds = normalizeArrangementSelection(currentProject, objectIds);
        const rootObjects = rootIds
          .map((id) => currentProject.objects.find((object) => object.id === id))
          .filter((object): object is ProjectObject => Boolean(object));
        const includesGroup = rootObjects.some((object) => object.data.type === 'group');
        if (includesGroup && rootObjects.length > 0) {
          targetIds = expandArrangementSelectionMembers(currentProject, rootIds);
          resolvedPivot = pivot ?? boundsCenter(getCombinedBounds(rootObjects.map((object) => object.bounds)));
        }
      }
      await projectService.rotateObjectsAndBakeActivePath(
        targetIds,
        degrees,
        resolvedPivot,
        activeObjectId,
      );
      const project = await projectService.getProject();
      if (project) set({ project: { ...project, dirty: true } });
      invalidatePreview();
      await refreshUndo();
    } catch (e) {
      notifyError(String(e));
    }
  },

  shearObjects: async (objectIds, shearX, shearY, pivot?) => {
    try {
      await projectService.shearObjects(objectIds, shearX, shearY, pivot);
      const project = await projectService.getProject();
      if (project) set({ project: { ...project, dirty: true } });
      invalidatePreview();
      await refreshUndo();
    } catch (e) {
      notifyError(String(e));
    }
  },

  setObjectsVisible: async (objectIds, visible) => {
    try {
      await projectService.setObjectsVisible(objectIds, visible);
      const project = await projectService.getProject();
      if (project) set({ project: { ...project, dirty: true } });
      invalidatePreview();
      await refreshUndo();
    } catch (e) {
      notifyError(String(e));
    }
  },

  updateObjectBoundsBatch: async (entries) => {
    try {
      await projectService.updateObjectBoundsBatch(entries);
      const project = await projectService.getProject();
      if (project) {
        set({ project: decorateProject({ ...project, dirty: true })! });
      } else {
        set((state) => ({
          project: state.project ? { ...state.project, dirty: true } : state.project,
        }));
      }
      invalidatePreview();
      await refreshUndo();
    } catch (e) {
      const project = await projectService.getProject();
      if (project) {
        set({ project: decorateProject({ ...project, dirty: true })! });
      }
      notifyError(String(e));
    }
  },

  pushDrawOrder: async (objectId, direction) => {
    try {
      await projectService.pushDrawOrder(objectId, direction);
      const project = await projectService.getProject();
      if (project) set({ project: { ...project, dirty: true } });
      invalidatePreview();
      await refreshUndo();
    } catch (e) {
      notifyError(String(e));
    }
  },

  moveObjectsTo: async (objectIds, x, y) => {
    try {
      const currentProject = get().project;
      if (!currentProject) return;
      const targetIds = expandArrangementSelectionMembers(currentProject, objectIds);
      const targetObjects = targetIds
        .map((id) => currentProject.objects.find((object) => object.id === id))
        .filter(Boolean) as ProjectObject[];
      if (targetObjects.length === 0) return;
      const minX = Math.min(...targetObjects.map((object) => object.bounds.min.x));
      const minY = Math.min(...targetObjects.map((object) => object.bounds.min.y));
      const dx = x - minX;
      const dy = y - minY;
      await projectService.updateObjectBoundsBatch(
        targetObjects.map((object) => ({
          id: object.id,
          bounds: {
            min: { x: object.bounds.min.x + dx, y: object.bounds.min.y + dy },
            max: { x: object.bounds.max.x + dx, y: object.bounds.max.y + dy },
          },
        })),
      );
      const project = await projectService.getProject();
      if (project) set({ project: { ...project, dirty: true } });
      invalidatePreview();
      await refreshUndo();
    } catch (e) {
      notifyError(String(e));
    }
  },

  computeDockArrangementSelection: () => {
    const project = get().project;
    if (!project) return [];
    return normalizeArrangementSelection(project, get().selectedObjectIds);
  },

  computeMirrorAcrossLineSelection: () => {
    const project = get().project;
    if (!project) return [];
    return computeMirrorAcrossLineSelection(project, get().selectedObjectIds).objectIds;
  },

  mirrorAcrossLine: async () => {
    const current = get();
    const project = current.project;
    if (!project) return;
    if (!canArrangeObjects(project, current.selectedObjectIds)) return;
    const mirrorSelection = computeMirrorAcrossLineSelection(project, current.selectedObjectIds);
    if (mirrorSelection.sourceIds.length === 0) return;
    const { axisObjectId } = mirrorSelection;
    if (!axisObjectId) {
      useNotificationStore.getState().push(i18n.t('notifications.select_mirror_axis'), 'warning');
      return;
    }
    try {
      const duplicated = await projectService.mirrorAcrossLine(mirrorSelection.objectIds, axisObjectId);
      if (duplicated.length === 0) return;
      const duplicatedIds = topLevelCreatedSelectionIds(duplicated);
      const nextProject = { ...project, objects: [...project.objects, ...duplicated], dirty: true };
      set({
        project: nextProject,
        selectedObjectIds: duplicatedIds,
        selectedLayerId: resolveSelectedLayerForObjects(
          nextProject,
          duplicatedIds,
          current.selectedLayerId,
        ),
      });
      invalidatePreview();
      await refreshUndo();
    } catch (e) {
      notifyError(String(e));
    }
  },

  makeSameSize: async (axis, preserveAspect) => {
    const current = get();
    const project = current.project;
    if (!project) return;
    if (!canScaleObjects(project, current.selectedObjectIds)) return;
    const normalizedIds = normalizeArrangementSelection(project, current.selectedObjectIds);
    if (normalizedIds.length < 2) return;
    const anchorObjectId = normalizedIds[normalizedIds.length - 1];
    try {
      const updatedObjects = await projectService.makeSameSize(
        normalizedIds,
        anchorObjectId,
        axis,
        preserveAspect,
      );
      if (updatedObjects.length === 0) return;
      const updatedMap = new Map(updatedObjects.map((object) => [object.id, object]));
      set({
        project: {
          ...project,
          objects: project.objects.map((object) => updatedMap.get(object.id) ?? object),
          dirty: true,
        },
      });
      invalidatePreview();
      await refreshUndo();
    } catch (e) {
      notifyError(String(e));
    }
  },

  resizeSlots: async (objectIds, options) => {
    const current = get();
    const project = current.project;
    if (!project) return false;
    if (!canScaleObjects(project, objectIds)) return false;
    const normalizedIds = normalizeArrangementSelection(project, objectIds);
    if (normalizedIds.length === 0) return false;
    try {
      const updatedObjects = await projectService.resizeSlots(normalizedIds, options);
      if (updatedObjects.length === 0) return true;
      const updatedMap = new Map(updatedObjects.map((object) => [object.id, object]));
      set({
        project: {
          ...project,
          objects: project.objects.map((object) => updatedMap.get(object.id) ?? object),
          dirty: true,
        },
      });
      invalidatePreview();
      await refreshUndo();
      return true;
    } catch (e) {
      notifyError(String(e));
      return false;
    }
  },

  moveObjectsTogether: async (axis) => {
    const current = get();
    const project = current.project;
    if (!project) return;
    if (!canPositionObjects(project)) return;
    const normalizedIds = normalizeArrangementSelection(project, current.selectedObjectIds);
    if (normalizedIds.length < 2) return;
    const anchorObjectId = resolveArrangementAnchorId(project, current.selectedObjectIds) ?? normalizedIds[normalizedIds.length - 1];
    try {
      const updatedObjects = await projectService.moveObjectsTogether(normalizedIds, axis, anchorObjectId);
      if (updatedObjects.length === 0) return;
      const updatedMap = new Map(updatedObjects.map((object) => [object.id, object]));
      set({
        project: {
          ...project,
          objects: project.objects.map((object) => updatedMap.get(object.id) ?? object),
          dirty: true,
        },
      });
      invalidatePreview();
      await refreshUndo();
    } catch (e) {
      notifyError(String(e));
    }
  },

  dockObjects: async (objectIds, direction, options) => {
    const project = get().project;
    if (!project || objectIds.length === 0) return false;
    if (!canArrangeObjects(project, objectIds)) return false;
    try {
      const updatedObjects = await projectService.dockObjects(objectIds, direction, options);
      if (updatedObjects.length === 0) return true;
      const updatedMap = new Map(updatedObjects.map((object) => [object.id, object]));
      set({
        project: {
          ...project,
          objects: project.objects.map((object) => updatedMap.get(object.id) ?? object),
          dirty: true,
        },
      });
      invalidatePreview();
      await refreshUndo();
      return true;
    } catch (e) {
      notifyError(String(e));
      return false;
    }
  },

  reassignLayer: async (objectIds, layerId) => {
    try {
      await projectService.reassignLayer(objectIds, layerId);
      const { selectedObjectIds, selectedLayerId } = get();
      const project = await projectService.getProject();
      if (project) {
        const shouldFollowSelection = selectedObjectIds.some((id) => objectIds.includes(id));
        set({
          project: { ...project, dirty: true },
          selectedLayerId: shouldFollowSelection
            ? resolveSelectedLayerId(project, layerId)
            : resolveSelectedLayerId(project, selectedLayerId),
        });
      }
      invalidatePreview();
      await refreshUndo();
      return true;
    } catch (e) {
      notifyError(String(e));
      return false;
    }
  },

  countDuplicates: async (objectIds) => {
    try {
      return await projectService.countDuplicates(objectIds);
    } catch (e) {
      notifyError(String(e));
      return 0;
    }
  },

  deleteDuplicates: async (objectIds) => {
    try {
      const remainingIds = await projectService.deleteDuplicates(objectIds);
      const project = await projectService.getProject();
      if (project) set({ project: { ...project, dirty: true }, selectedObjectIds: remainingIds });
      invalidatePreview();
      await refreshUndo();
    } catch (e) {
      notifyError(String(e));
    }
  },

  autoJoinShapes: async (objectIds, toleranceMm) => {
    try {
      const paths = await projectService.autoJoinShapes(objectIds, toleranceMm);
      if (paths.length === 0) return;
      const project = await projectService.getProject();
      if (project) set({ project: { ...project, dirty: true } });
      invalidatePreview();
      await refreshUndo();
    } catch (e) {
      notifyError(String(e));
    }
  },

  optimizeShapes: async (objectIds) => {
    try {
      const paths = await projectService.optimizeShapes(objectIds);
      if (paths.length === 0) return;
      const project = await projectService.getProject();
      if (project) set({ project: { ...project, dirty: true } });
      invalidatePreview();
      await refreshUndo();
    } catch (e) {
      notifyError(String(e));
    }
  },

  selectOpenShapes: async () => {
    try {
      const ids = await projectService.selectOpenShapes();
      const project = await projectService.getProject();
      if (project) {
        set((state) => ({
          project,
          selectedObjectIds: mergeSelectionAddOrder(
            state.selectedObjectIds,
            orderBatchForDrawOrderAnchor(ids, project.objects),
          ),
        }));
      }
    } catch (e) {
      notifyError(String(e));
    }
  },

  selectOpenShapesSetToFill: async () => {
    try {
      const ids = await projectService.selectOpenShapesSetToFill();
      const project = await projectService.getProject();
      if (project) {
        set((state) => ({
          project,
          selectedObjectIds: mergeSelectionAddOrder(
            state.selectedObjectIds,
            orderBatchForDrawOrderAnchor(ids, project.objects),
          ),
        }));
      }
    } catch (e) {
      notifyError(String(e));
    }
  },

  selectAllShapesInCurrentLayer: async () => {
    try {
      const layerId = get().selectedLayerId;
      if (!layerId) return;
      const ids = await projectService.selectAllInLayer(layerId);
      const project = get().project;
      const orderedIds = project ? orderBatchForDrawOrderAnchor(ids, project.objects) : ids;
      set((state) => ({
        selectedObjectIds: mergeSelectionAddOrder(state.selectedObjectIds, orderedIds),
      }));
    } catch (e) {
      notifyError(String(e));
    }
  },

  selectContainedShapes: async () => {
    try {
      const selectedIds = get().selectedObjectIds;
      if (selectedIds.length !== 1) return;
      const ids = await projectService.selectContainedShapes(selectedIds[0]);
      const project = get().project;
      const orderedIds = project ? orderBatchForDrawOrderAnchor(ids, project.objects) : ids;
      set((state) => ({
        selectedObjectIds: mergeSelectionAddOrder(state.selectedObjectIds, orderedIds),
      }));
    } catch (e) {
      notifyError(String(e));
    }
  },

  selectShapesSmallerThanSelected: async () => {
    try {
      const selectedIds = get().selectedObjectIds;
      if (selectedIds.length === 0) return;
      const ids = await projectService.selectShapesSmallerThanSelected(selectedIds);
      const project = await projectService.getProject();
      if (project) {
        set((state) => ({
          project,
          selectedObjectIds: mergeSelectionAddOrder(
            state.selectedObjectIds,
            orderBatchForDrawOrderAnchor(ids, project.objects),
          ),
        }));
      }
    } catch (e) {
      notifyError(String(e));
    }
  },

  unlinkVirtualClone: async (objectId) => {
    try {
      const updated = await vectorService.unlinkVirtualClone(objectId);
      set((state) => {
        const project = state.project;
        if (!project) return state;
        return {
          project: {
            ...project,
            objects: project.objects.map((o) => (o.id === updated.id ? updated : o)),
            dirty: true,
          },
        };
      });
      invalidatePreview();
      await refreshUndo();
    } catch (e) {
      notifyError(String(e));
    }
  },

  // --- Project-level setters ---

  setStartFrom: async (mode) => {
    try {
      const current = get().project;
      if (current?.start_from === mode) return;
      await projectService.setStartFrom(mode);
      if (current) {
        set({ project: { ...current, start_from: mode, dirty: true } });
      }
      invalidatePreview();
      await refreshUndo();
    } catch (e) {
      notifyError(String(e));
    }
  },

  setJobOrigin: async (anchor) => {
    try {
      const current = get().project;
      if (current?.job_origin === anchor) return;
      await projectService.setJobOrigin(anchor);
      if (current) {
        set({ project: { ...current, job_origin: anchor, dirty: true } });
      }
      invalidatePreview();
      await refreshUndo();
    } catch (e) {
      notifyError(String(e));
    }
  },

  setUserOrigin: async (x: number, y: number) => {
    try {
      await projectService.setUserOrigin(x, y);
      const current = get().project;
      if (current) {
        set({ project: { ...current, user_origin: [x, y], dirty: true } });
      }
      invalidatePreview();
      await refreshUndo();
    } catch (e) {
      notifyError(String(e));
    }
  },

  setOptimization: async (patch: ProjectOptimizationPatch) => {
    try {
      const current = get().project;
      if (!current) return;

      const merged = await projectService.setOptimization(patch);
      const latest = get().project;
      if (!latest) return;
      if (JSON.stringify(latest.optimization) === JSON.stringify(merged)) {
        return;
      }
      set({
        project: { ...latest, optimization: merged, dirty: true },
      });
      invalidatePreview();
      await refreshUndo();
    } catch (e) {
      notifyError(String(e));
    }
  },

  setMaterialHeight: async (value: number | null) => {
    try {
      const current = get().project;
      if (!current) return;
      const existing = current.material_height_mm ?? null;
      if (existing === value) return;
      await projectService.setMaterialHeight(value);
      set({ project: { ...current, material_height_mm: value, dirty: true } });
      invalidatePreview();
      await refreshUndo();
    } catch (e) {
      notifyError(String(e));
    }
  },

  updateProjectNotes: async (notes) => {
    try {
      await projectService.updateProjectNotes(notes);
      const current = get().project;
      if (current) {
        set({ project: { ...current, notes, dirty: true }, error: null });
      }
      await refreshUndo();
      return true;
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
      return false;
    }
  },

  setTransformLocks: async (locks) => {
    try {
      await projectService.setTransformLocks(locks);
      const current = get().project;
      if (current) {
        set({ project: { ...current, transform_locks: locks, dirty: true } });
      }
      await refreshUndo();
    } catch (e) {
      notifyError(String(e));
    }
  },

  setPendingPaletteColor: (color) => {
    set({ pendingPaletteColor: color });
  },

  // --- Boolean / vector ops ---

  booleanIntersection: async (objectIdA, objectIdB) => {
    if (get().booleanPending) return;
    set({ booleanPending: true });
    try {
      const newObj = await vectorService.booleanIntersection(objectIdA, objectIdB);
      const project = await projectService.getProject();
      if (project) {
        const nextProject = decorateProject({ ...project, dirty: true })!;
        set({
          project: nextProject,
          selectedLayerId: resolveSelectedLayerForObjects(
            nextProject,
            [newObj.id],
            newObj.layer_id,
          ),
          selectedObjectIds: [newObj.id],
        });
        invalidatePreview();
        await refreshUndo();
      }
    } catch (e) {
      notifyError(String(e));
    } finally {
      set({ booleanPending: false });
    }
  },

  booleanWeld: async (objectIds) => {
    if (get().booleanPending) return;
    set({ booleanPending: true });
    try {
      const newObj = await vectorService.booleanWeld(objectIds);
      const project = await projectService.getProject();
      if (project) {
        const nextProject = decorateProject({ ...project, dirty: true })!;
        set({
          project: nextProject,
          selectedLayerId: resolveSelectedLayerForObjects(
            nextProject,
            [newObj.id],
            newObj.layer_id,
          ),
          selectedObjectIds: [newObj.id],
        });
        invalidatePreview();
        await refreshUndo();
      }
    } catch (e) {
      notifyError(String(e));
    } finally {
      set({ booleanPending: false });
    }
  },

  cutShapes: async (objectIds) => {
    if (get().booleanPending) return;
    set({ booleanPending: true });
    try {
      const result = await vectorService.cutShapesApply(objectIds);
      const nextProject = await projectService.getProject();
      if (nextProject) {
        const groupedIds = [result.insideGroupId, result.outsideGroupId].filter(Boolean) as string[];
        const createdIds = groupedIds.length > 0 ? groupedIds : result.createdObjectIds;
        set({
          project: decorateProject({ ...nextProject, dirty: true })!,
          selectedLayerId: resolveSelectedLayerForObjects(
            nextProject,
            createdIds,
            get().selectedLayerId,
          ),
          selectedObjectIds: createdIds,
        });
        invalidatePreview();
        await refreshUndo();
      }
    } catch (e) {
      notifyError(String(e));
    } finally {
      set({ booleanPending: false });
    }
  },

  closeAndJoin: async (objectIds, toleranceMm = 0.1, options) => {
    if (get().booleanPending) return null;
    set({ booleanPending: true });
    try {
      const result = await vectorService.closeAndJoin(objectIds, toleranceMm);
      const project = await projectService.getProject();
      if (project) {
        const nextProject = decorateProject({ ...project, dirty: true })!;
        set({
          project: nextProject,
          selectedLayerId: resolveSelectedLayerForObjects(
            nextProject,
            [result.object.id],
            result.object.layer_id,
          ),
          selectedObjectIds: [result.object.id],
        });
        invalidatePreview();
        await refreshUndo();
      }
      if (!result.fullyClosed && (options?.warnIfOpen ?? true)) {
        useNotificationStore
          .getState()
          .push(i18n.t('notifications.paths_not_closed'), 'warning');
      }
      return result;
    } catch (e) {
      notifyError(String(e));
      return null;
    } finally {
      set({ booleanPending: false });
    }
  },

  offsetShapes: async (objectIds, distanceMm, direction, cornerStyle, deleteOriginal) => {
    try {
      const created = await vectorService.offsetShapes(
        objectIds,
        distanceMm,
        direction,
        cornerStyle,
        deleteOriginal,
      );
      const createdIds = created.map((o) => o.id);
      const project = await projectService.getProject();
      if (project) {
        set({
          project: { ...project, dirty: true },
          selectedObjectIds: createdIds,
          selectedLayerId: resolveSelectedLayerForObjects(
            project,
            createdIds,
            get().selectedLayerId,
          ),
        });
      }
      invalidatePreview();
      await refreshUndo();
    } catch (e) {
      notifyError(String(e));
      throw e;
    }
  },

  breakApart: async (objectId) => {
    try {
      const created = await vectorService.breakApart(objectId);
      if (created.length === 0) return; // nothing to break — silent no-op
      const project = await projectService.getProject();
      if (project) {
        const createdIds = created.map((o) => o.id);
        set({
          project: { ...project, dirty: true },
          selectedObjectIds: createdIds,
          selectedLayerId: resolveSelectedLayerForObjects(
            project,
            createdIds,
            get().selectedLayerId,
          ),
        });
        invalidatePreview();
        await refreshUndo();
      }
    } catch (e) {
      notifyError(String(e));
    }
  },

  closePath: async (objectId) => {
    try {
      await vectorService.closePath(objectId);
      const project = await projectService.getProject();
      if (project) {
        set({ project: { ...project, dirty: true } });
        invalidatePreview();
        await refreshUndo();
      }
    } catch (e) {
      notifyError(String(e));
    }
  },

  gridArray: async (params) => {
    try {
      const result = await vectorService.gridArray(params);
      const project = await projectService.getProject();
      if (project) {
        const selectIds = result.groupId ? [result.groupId] : result.createdIds;
        const previousLayerId = get().selectedLayerId;
        set({
          project: { ...project, dirty: true },
          selectedObjectIds: selectIds,
          selectedLayerId: resolveSelectedLayerForObjects(project, selectIds, previousLayerId),
        });
      }
      invalidatePreview();
      await refreshUndo();
    } catch (e) {
      notifyError(String(e));
      throw e;
    }
  },

  circularArray: async (params) => {
    try {
      const result = await vectorService.circularArray(params);
      const project = await projectService.getProject();
      if (project) {
        const selectIds = result.groupId ? [result.groupId] : result.createdIds;
        const previousLayerId = get().selectedLayerId;
        set({
          project: { ...project, dirty: true },
          selectedObjectIds: selectIds,
          selectedLayerId: resolveSelectedLayerForObjects(project, selectIds, previousLayerId),
        });
      }
      invalidatePreview();
      await refreshUndo();
    } catch (e) {
      notifyError(String(e));
      throw e;
    }
  },

  addTabs: async (objectId, count, widthMm) => {
    try {
      const updated = await vectorService.addTabs(objectId, count, widthMm);
      const { project } = get();
      if (project) {
        set({
          project: {
            ...project,
            objects: project.objects.map((o) => (o.id === objectId ? updated : o)),
            dirty: true,
          },
        });
        invalidatePreview();
        await refreshUndo();
      }
    } catch (e) {
      notifyError(String(e));
    }
  },

  placeTab: async (objectId, worldX, worldY) => {
    try {
      const updated = await vectorService.placeTab(objectId, worldX, worldY);
      const { project } = get();
      if (project) {
        set({
          project: {
            ...project,
            objects: project.objects.map((o) => (o.id === objectId ? updated : o)),
            dirty: true,
          },
        });
        invalidatePreview();
        await refreshUndo();
      }
    } catch (e) {
      notifyError(String(e));
    }
  },

  removeTab: async (objectId, worldX, worldY) => {
    try {
      const updated = await vectorService.removeTab(objectId, worldX, worldY);
      const { project } = get();
      if (project) {
        set({
          project: {
            ...project,
            objects: project.objects.map((o) => (o.id === objectId ? updated : o)),
            dirty: true,
          },
        });
        invalidatePreview();
        await refreshUndo();
      }
    } catch (e) {
      notifyError(String(e));
    }
  },

  applyRadius: async (objectId, radiusMm) => {
    try {
      const updated = await vectorService.applyRadius(objectId, radiusMm);
      const { project } = get();
      if (project) {
        set({
          project: {
            ...project,
            objects: project.objects.map((o) => (o.id === objectId ? updated : o)),
            dirty: true,
          },
        });
        invalidatePreview();
        await refreshUndo();
      }
    } catch (e) {
      notifyError(String(e));
    }
  },

  applyCornerRadius: async (objectId, subpathIndex, vertexIndex, radiusMm) => {
    try {
      const updated = await vectorService.applyCornerRadius(
        objectId,
        subpathIndex,
        vertexIndex,
        radiusMm,
      );
      const { project } = get();
      if (project) {
        set({
          project: {
            ...project,
            objects: project.objects.map((o) => (o.id === objectId ? updated : o)),
            dirty: true,
          },
        });
        invalidatePreview();
        await refreshUndo();
      }
    } catch (e) {
      notifyError(String(e));
    }
  },

  convertToBitmap: async (objectId, dpi) => {
    try {
      const updated = await vectorService.convertToBitmap(objectId, dpi);
      const project = await projectService.getProject();
      if (project) {
        set({
          project: { ...project, dirty: true },
          selectedLayerId: resolveSelectedLayerId(project, updated.layer_id),
          selectedObjectIds: [updated.id],
        });
        invalidatePreview();
        await refreshUndo();
      }
    } catch (e) {
      notifyError(String(e));
    }
  },

  applyPathToText: async (textObjectId, pathObjectId) => {
    try {
      const created = await vectorService.applyPathToText(textObjectId, pathObjectId);
      const project = await projectService.getProject();
      if (project) {
        set({
          project: { ...project, dirty: true },
          selectedLayerId: resolveSelectedLayerId(project, created.layer_id),
          selectedObjectIds: [created.id],
        });
        invalidatePreview();
        await refreshUndo();
      }
    } catch (e) {
      notifyError(String(e));
    }
  },

  cropImage: async (imageObjectId, maskObjectId) => {
    try {
      const updated = await vectorService.cropImage(imageObjectId, maskObjectId);
      const project = await projectService.getProject();
      if (project) {
        set({
          project: { ...project, dirty: true },
          selectedLayerId: resolveSelectedLayerId(project, updated.layer_id),
          selectedObjectIds: [updated.id],
        });
        invalidatePreview();
        await refreshUndo();
      }
    } catch (e) {
      notifyError(String(e));
    }
  },

  applyMaskToImage: async (imageObjectId, maskObjectId) => {
    try {
      const updated = await vectorService.applyMaskToImage(imageObjectId, maskObjectId);
      const project = await projectService.getProject();
      if (project) {
        set({
          project: { ...project, dirty: true },
          selectedLayerId: resolveSelectedLayerId(project, updated.layer_id),
          selectedObjectIds: [updated.id],
        });
        invalidatePreview();
        await refreshUndo();
      }
    } catch (e) {
      notifyError(String(e));
    }
  },

  assignImageMask: async (imageObjectId, maskObjectIds, polarity = 'keep_inside') => {
    try {
      const updated = await vectorService.assignImageMask(imageObjectId, maskObjectIds, polarity);
      const project = await projectService.getProject();
      if (project) {
        set({
          project: { ...project, dirty: true },
          selectedLayerId: resolveSelectedLayerId(project, updated.layer_id),
          selectedObjectIds: [updated.id, ...maskObjectIds],
        });
        invalidatePreview();
        await refreshUndo();
      }
    } catch (e) {
      notifyError(String(e));
    }
  },

  setImageMaskPolarity: async (imageObjectId, maskObjectId, polarity) => {
    try {
      const updated = await vectorService.setImageMaskPolarity(imageObjectId, maskObjectId, polarity);
      const project = await projectService.getProject();
      if (project) {
        set({
          project: { ...project, dirty: true },
          selectedLayerId: resolveSelectedLayerId(project, updated.layer_id),
        });
        invalidatePreview();
        await refreshUndo();
      }
    } catch (e) {
      notifyError(String(e));
    }
  },

  removeImageMask: async (imageObjectId, maskObjectId) => {
    try {
      const updated = await vectorService.removeImageMask(imageObjectId, maskObjectId);
      const project = await projectService.getProject();
      if (project) {
        set({
          project: { ...project, dirty: true },
          selectedLayerId: resolveSelectedLayerId(project, updated.layer_id),
        });
        invalidatePreview();
        await refreshUndo();
      }
    } catch (e) {
      notifyError(String(e));
    }
  },

  closeSelectedPathsWithTolerance: async (objectIds, toleranceMm, mode) => {
    try {
      const result = await projectService.closeSelectedPathsWithTolerance(objectIds, toleranceMm, mode);
      const project = await projectService.getProject();
      if (project) {
        set({ project: { ...project, dirty: result.shapesClosed > 0 }, selectedObjectIds: result.objectIds });
        if (result.shapesClosed > 0) {
          invalidatePreview();
          await refreshUndo();
        }
      }
    } catch (e) {
      notifyError(String(e));
    }
  },

  refreshImage: async (objectId) => {
    try {
      await importService.refreshImage(objectId);
      const currentObject = get().project?.objects.find((obj) => obj.id === objectId);
      if (currentObject?.data.type === 'raster_image') {
        set(dropCachedAsset(get().assetCache, get().assetLoadErrors, currentObject.data.asset_key));
      }
      await get().loadProject();
      invalidatePreview();
    } catch (e) {
      notifyError(String(e));
    }
  },

  replaceImage: async (objectId, filePath) => {
    try {
      const replaced = await importService.replaceImage(objectId, filePath);
      if (!replaced) return;
      const currentObject = get().project?.objects.find((obj) => obj.id === objectId);
      if (currentObject?.data.type === 'raster_image') {
        set(dropCachedAsset(get().assetCache, get().assetLoadErrors, currentObject.data.asset_key));
      }
      await get().loadProject();
      invalidatePreview();
    } catch (e) {
      notifyError(String(e));
    }
  },

  replaceImageToFit: async (objectId, filePath) => {
    try {
      const replaced = await importService.replaceImageToFit(objectId, filePath);
      if (!replaced) return;
      const currentObject = get().project?.objects.find((obj) => obj.id === objectId);
      if (currentObject?.data.type === 'raster_image') {
        set(dropCachedAsset(get().assetCache, get().assetLoadErrors, currentObject.data.asset_key));
      }
      await get().loadProject();
      invalidatePreview();
    } catch (e) {
      notifyError(String(e));
    }
  },

  copyAlongPath: async (objectIds, pathObjectId, options) => {
    try {
      const projectBefore = get().project;
      if (!projectBefore || objectIds.length === 0) {
        return false;
      }
      if (!canCopyAlongPathObjects(projectBefore, objectIds, pathObjectId, options.scaleCopies)) {
        return false;
      }
      const created = await vectorService.copyAlongPathBatch(objectIds, pathObjectId, options);
      const project = await projectService.getProject();
      if (project) {
        const createdIds = topLevelCreatedSelectionIds(created);
        const selectedLayerId = created[0]
          ? resolveSelectedLayerId(project, created[0].layer_id)
          : resolveSelectedLayerId(project, get().selectedLayerId);
        set({
          project: { ...project, dirty: true },
          selectedLayerId,
          selectedObjectIds: createdIds,
        });
        invalidatePreview();
        await refreshUndo();
      }
      return true;
    } catch (e) {
      notifyError(String(e));
      return false;
    }
  },

  rubberBandOutline: async (objectIds) => {
    try {
      const created = await vectorService.rubberBandOutline(objectIds);
      const { project } = get();
      if (project) {
        set({
          project: {
            ...project,
            objects: [...project.objects, created],
            dirty: true,
          },
          selectedLayerId: created.layer_id,
          selectedObjectIds: [created.id],
        });
        invalidatePreview();
        await refreshUndo();
      }
    } catch (e) {
      notifyError(String(e));
    }
  },
}));

const RASTER_IMPORT_EXTS = ['png', 'jpg', 'jpeg', 'bmp', 'gif', 'tif', 'tiff', 'webp', 'tga'];
const GCODE_IMPORT_EXTS = ['gc', 'gcode', 'nc', 'ngc'];

function importExtOf(name: string): string {
  return name.split('.').pop()?.toLowerCase() ?? '';
}

function isRasterImportName(name: string): boolean {
  return RASTER_IMPORT_EXTS.includes(importExtOf(name));
}

function isGcodeImportName(name: string): boolean {
  return GCODE_IMPORT_EXTS.includes(importExtOf(name));
}

/**
 * Shared flow for path-based (File > Import, native drops) and content-based
 * (HTML5 drops, which have no OS paths) artwork imports: resolves the
 * destination layer (auto-routing + create-on-demand), runs `doImport`
 * against it, then refreshes the project and applies the post-import
 * selection policy. `artworkNames` must already exclude G-code files and is
 * used only for raster/vector content detection.
 */
async function importArtworkBatch(
  artworkNames: string[],
  requestedLayerId: string | undefined,
  doImport: (layerId: string) => Promise<ProjectObject[]>,
): Promise<void> {
  const store = useProjectStore.getState();
  let projectSnapshot = store.project;
  const pendingAtEntry = store.pendingPaletteColor;
  const selectedAtEntry = store.selectedLayerId;
  const resolveImportLayerId = async (contentKind: 'raster' | 'non_raster') => {
    let resolvedLayerId =
      requestedLayerId ?? selectedAtEntry ?? projectSnapshot?.layers[0]?.id ?? null;

    if (projectSnapshot) {
      let out = resolveDestinationLayer({
        project: projectSnapshot,
        requestedLayerId: requestedLayerId ?? null,
        pendingColor: pendingAtEntry,
        selectedLayerId: selectedAtEntry,
        contentKind,
      });
      if (out.kind === 'needs_create') {
        const req = out as NeedsBackendCreate;
        const newLayer = await createFamilySiblingLayer(req);
        projectSnapshot = {
          ...projectSnapshot,
          layers: [...projectSnapshot.layers, newLayer],
        };
        out = resolveDestinationLayer({
          project: projectSnapshot,
          requestedLayerId: requestedLayerId ?? null,
          pendingColor: pendingAtEntry,
          selectedLayerId: selectedAtEntry,
          contentKind,
        });
        if (out.kind === 'needs_create') {
          resolvedLayerId = newLayer.id;
        }
      }
      if (out.kind === 'resolved' && out.layerId !== AUTO_LAYER_ID) {
        resolvedLayerId = out.layerId;
      }
    }

    if (!resolvedLayerId || (projectSnapshot?.layers.length ?? 0) === 0) {
      const [nextLayerName, nextLayerOp] =
        contentKind === 'raster' ? ['Image', 'image' as const] : ['Line', 'line' as const];
      const createdLayer = await projectService.addLayer(nextLayerName, nextLayerOp);
      projectSnapshot = projectSnapshot
        ? { ...projectSnapshot, layers: [...projectSnapshot.layers, createdLayer] }
        : projectSnapshot;
      resolvedLayerId = createdLayer.id;
    }

    return resolvedLayerId;
  };

  const importedObjects: ProjectObject[] = [];
  if (artworkNames.length > 0) {
    const artworkContentKind = artworkNames.some(isRasterImportName)
      ? ('raster' as const)
      : ('non_raster' as const);
    const artworkLayerId = await resolveImportLayerId(artworkContentKind);
    if (artworkLayerId) {
      importedObjects.push(...(await doImport(artworkLayerId)));
    }
  }

  if (importedObjects.length === 0) return;
  const { project } = useProjectStore.getState();
  if (project) {
    // Reload full project to get assets list and layers in sync
    const refreshed = await projectService.getProject();
    if (refreshed) {
      // Compute the union of destination layer ids the backend
      // actually routed the objects to. A mixed raster+vector batch
      // can split across multiple sibling layers (image + non-image),
      // so we need to pick a deliberate selection policy rather than
      // just grabbing the first object's layer.
      const destLayerIds: string[] = [];
      const destCounts = new Map<string, number>();
      for (const obj of importedObjects) {
        const lid = obj.layer_id as string | undefined;
        if (!lid) continue;
        if (!destCounts.has(lid)) destLayerIds.push(lid);
        destCounts.set(lid, (destCounts.get(lid) ?? 0) + 1);
      }

      // Policy for `selectedLayerId` (single-select UI):
      //   1. If the backend routed the caller's requested layer
      //      unchanged (i.e. `resolvedLayerId` is among the
      //      destinations), keep it — the user's intended target is
      //      still valid for some of the imported objects.
      //   2. Otherwise pick the destination layer with the most
      //      imported objects, breaking ties by preferring a
      //      non-image layer (assumption: the user was working on a
      //      vector layer when triggering the import, so the
      //      majority-vector destination is the more natural focus).
      //   3. Fall back to the first routed layer if neither rule
      //      applies.
      const pickSelection = (): string => {
        const fallbackLayerId =
          requestedLayerId ??
          selectedAtEntry ??
          project.layers[0]?.id ??
          refreshed.layers[0]?.id ??
          '';
        if (destLayerIds.length === 0) return fallbackLayerId;
        if (fallbackLayerId && destCounts.has(fallbackLayerId)) return fallbackLayerId;
        let best = destLayerIds[0];
        let bestCount = destCounts.get(best) ?? 0;
        for (const lid of destLayerIds) {
          const count = destCounts.get(lid) ?? 0;
          if (count > bestCount) {
            best = lid;
            bestCount = count;
          } else if (count === bestCount) {
            const layer = refreshed.layers.find((l) => l.id === lid);
            const bestLayer = refreshed.layers.find((l) => l.id === best);
            if (
              layer &&
              bestLayer &&
              primaryEntryOf(layer).operation !== 'image' &&
              primaryEntryOf(bestLayer).operation === 'image'
            ) {
              best = lid;
            }
          }
        }
        return best;
      };
      const actualLayerId = pickSelection();

      // Select every imported object regardless of which destination
      // layer it ended up on — this is the "union of routed layers"
      // visible state that the user expects from a mixed batch drop.
      // The active layer (selectedLayerId) is still a single value
      // because the Cuts/Layers panel is single-select today, but
      // the object-level selection makes the split visible in the
      // canvas and properties panel.
      const importedIds = topLevelCreatedSelectionIds(importedObjects);

      useProjectStore.setState({
        project: { ...refreshed, dirty: true },
        pendingPaletteColor: null,
        selectedLayerId: actualLayerId,
        selectedObjectIds: importedIds,
      });
      invalidatePreview();
      await refreshUndo();
    }
  }
}
