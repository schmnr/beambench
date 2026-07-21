import type { ProjectObject, ObjectData, Asset } from '../types/project';

/** Types that can be converted to a VecPath (from object_to_vecpath in convert.rs) */
const VECPATH_TYPES = new Set(['shape', 'vector_path', 'text', 'polygon', 'star']);
/** Types that are inherently closed; vector_path needs an explicit closed check. */
const CLOSED_TYPES = new Set(['shape', 'text', 'polygon', 'star']);

/** Content category used by the layer-family resolver and layer-
 *  content invariant. Matches the backend's `effective_is_raster`
 *  semantics — raster clones of raster sources count as raster. */
export type ContentKind = 'raster' | 'non_raster';

/** Resolve through VirtualClone chains to get the underlying object data. */
export function resolveEffectiveData(obj: ProjectObject, allObjects: ProjectObject[]): ProjectObject['data'] | null {
  let current = obj;
  let depth = 0;
  while (current.data.type === 'virtual_clone' && depth < 10) {
    const sourceId = (current.data as { source_id: string }).source_id;
    const src = allObjects.find((o) => o.id === sourceId);
    if (!src) return null;
    current = src;
    depth++;
  }
  return current.data;
}

/** Given an `ObjectData` and the full object list (needed to walk
 *  `VirtualClone.source_id` references), return `'raster'` iff the
 *  effective resolved data is a `raster_image`. Mirrors
 *  `beambench_service::effective_is_raster` in Rust so both sides
 *  agree on clone classification. */
export function objectContentKind(
  data: ObjectData,
  allObjects: ProjectObject[],
): ContentKind {
  if (data.type === 'raster_image') return 'raster';
  if (data.type === 'virtual_clone') {
    const sourceId = (data as { source_id: string }).source_id;
    // Bounded walk in case of cycles — same depth limit as resolveEffectiveData.
    let currentId: string | undefined = sourceId;
    let depth = 0;
    while (currentId && depth < 10) {
      const src = allObjects.find((o) => o.id === currentId);
      if (!src) return 'non_raster';
      if (src.data.type === 'raster_image') return 'raster';
      if (src.data.type !== 'virtual_clone') return 'non_raster';
      currentId = (src.data as { source_id: string }).source_id;
      depth++;
    }
    return 'non_raster';
  }
  return 'non_raster';
}

/** Check whether an object is a vector type (possibly through VirtualClone). */
export function isEffectiveVector(obj: ProjectObject, allObjects: ProjectObject[]): boolean {
  const data = resolveEffectiveData(obj, allObjects);
  return data !== null && VECPATH_TYPES.has(data.type);
}

/** Preserve explicit selection order instead of project storage order. */
export function orderSelectedObjects(
  selectedObjectIds: string[],
  objects: ProjectObject[],
): ProjectObject[] {
  return selectedObjectIds
    .map((id) => objects.find((obj) => obj.id === id) ?? null)
    .filter((obj): obj is ProjectObject => obj !== null);
}

/** Copy-along-path uses the last selected eligible vector as the guide. */
export function pickLastSelectedVectorGuide(
  selectedObjectIds: string[],
  objects: ProjectObject[],
): ProjectObject | null {
  return [...orderSelectedObjects(selectedObjectIds, objects)]
    .reverse()
    .find((obj) => isEffectiveVector(obj, objects)) ?? null;
}

/** Check whether an object is suitable for boolean operations (closed vector shape). */
export function isBooleanCompatible(obj: ProjectObject, allObjects: ProjectObject[]): boolean {
  return isBooleanCompatibleObject(obj, allObjects, new Set());
}

function isBooleanCompatibleObject(
  obj: ProjectObject,
  allObjects: ProjectObject[],
  seen: Set<string>,
): boolean {
  if (seen.has(obj.id)) return false;
  seen.add(obj.id);
  if (!obj.visible || obj.locked) return false;
  const data = resolveEffectiveData(obj, allObjects);
  if (!data) return false;
  if (data.type === 'group') {
    const children = (data as { children: string[] }).children;
    return children.length > 0 && children.every((childId) => {
      const child = allObjects.find((candidate) => candidate.id === childId);
      return child !== undefined && isBooleanCompatibleObject(child, allObjects, seen);
    });
  }
  if (CLOSED_TYPES.has(data.type)) return true;
  if (data.type === 'vector_path') return (data as { closed: boolean }).closed;
  return false;
}

export function isClosedVectorCompatible(obj: ProjectObject, allObjects: ProjectObject[]): boolean {
  return isBooleanCompatible(obj, allObjects);
}

export function imageObjectHasSourcePath(obj: ProjectObject | null, assets: Asset[]): boolean {
  const data = obj?.data;
  if (data?.type !== 'raster_image') return false;
  const asset = assets.find((candidate) => candidate.id === data.asset_key);
  return typeof asset?.source_path === 'string' && asset.source_path.trim().length > 0;
}

export interface SelectionContext {
  selectedObjectIds: string[];
  selectedObjects: ProjectObject[];
  hasSelection: boolean;
  singleSelected: ProjectObject | null;
  hasLocked: boolean;
  hasUnlocked: boolean;
  canMutate: boolean;
  hasClipboard: boolean;
  canGroup: boolean;
  canUngroup: boolean;
  canConvertToPath: boolean;
  canClosePath: boolean;
  canBreakApart: boolean;
  canConvertToBitmap: boolean;
  canBoolean: boolean;
  canWeld: boolean;
  canTraceImage: boolean;
  canAdjustImage: boolean;
  canRefreshImage: boolean;
  canSelectContainedShapes: boolean;
  canSaveProcessedBitmap: boolean;
  canUseAsImageMask: boolean;
  imageMaskTargetId: string | null;
  imageMaskObjectIds: string[];
  imageMaskSelectionHasInvalidMasks: boolean;
  canRemoveImageMask: boolean;
  hiddenPanelIds: string[];
}

export function createSelectionContext(
  selectedObjectIds: string[],
  objects: ProjectObject[],
  hasClipboard: boolean,
  hiddenPanelIds: string[],
  assets: Asset[] = [],
): SelectionContext {
  const selectedObjects = objects.filter((o) => selectedObjectIds.includes(o.id));
  const singleSelected = selectedObjects.length === 1 ? selectedObjects[0] : null;
  const hasLocked = selectedObjects.some((o) => o.locked);
  const hasUnlocked = selectedObjects.some((o) => !o.locked);
  const hasSelection = selectedObjectIds.length > 0;
  const selectedRasterObject =
    selectedObjects.find((object) => resolveEffectiveData(object, objects)?.type === 'raster_image') ?? null;
  const selectedMaskObjects = selectedObjects.filter((object) => {
    if (selectedRasterObject?.id === object.id) return false;
    const data = resolveEffectiveData(object, objects);
    return data !== null && VECPATH_TYPES.has(data.type);
  });
  const imageMaskSelectionHasInvalidMasks = selectedMaskObjects.some(
    (object) => !isBooleanCompatible(object, objects),
  );
  return {
    selectedObjectIds,
    selectedObjects,
    hasSelection,
    singleSelected,
    hasLocked,
    hasUnlocked,
    canMutate: hasSelection && !hasLocked,
    hasClipboard,
    canGroup: selectedObjectIds.length >= 2 && !hasLocked,
    canUngroup:
      selectedObjectIds.length === 1 &&
      !hasLocked &&
      singleSelected?.data.type === 'group',
    // Convert to Path / Convert to Bitmap both resolve VirtualClones
    // to their source before operating (backend: ensure_resolved in
    // vector.rs). Mirror that here so a clone-backed vector object
    // keeps its affordance in the UI instead of appearing greyed out.
    canConvertToPath:
      selectedObjectIds.length === 1 &&
      !hasLocked &&
      singleSelected != null &&
      isEffectiveVector(singleSelected, objects),
    canClosePath:
      selectedObjectIds.length >= 1 &&
      !hasLocked &&
      selectedObjects.every((object) => isEffectiveVector(object, objects)),
    canBreakApart:
      selectedObjectIds.length === 1 &&
      !hasLocked &&
      singleSelected != null &&
      isEffectiveVector(singleSelected, objects),
    canConvertToBitmap:
      selectedObjectIds.length === 1 &&
      !hasLocked &&
      singleSelected != null &&
      isEffectiveVector(singleSelected, objects),
    canBoolean:
      selectedObjectIds.length === 2 &&
      !hasLocked &&
      selectedObjects.every((o) => isBooleanCompatible(o, objects)),
    canWeld:
      selectedObjectIds.length >= 2 &&
      !hasLocked &&
      selectedObjects.every((o) => isBooleanCompatible(o, objects)),
    canTraceImage:
      selectedObjectIds.length === 1 &&
      !hasLocked &&
      singleSelected?.data.type === 'raster_image',
    canAdjustImage:
      selectedObjectIds.length === 1 &&
      !hasLocked &&
      singleSelected?.data.type === 'raster_image',
    canRefreshImage:
      selectedObjectIds.length === 1 &&
      !hasLocked &&
      imageObjectHasSourcePath(singleSelected, assets),
    canSelectContainedShapes:
      selectedObjectIds.length === 1 &&
      !hasLocked &&
      singleSelected != null &&
      isClosedVectorCompatible(singleSelected, objects),
    canSaveProcessedBitmap:
      selectedObjectIds.length === 1 &&
      singleSelected != null &&
      resolveEffectiveData(singleSelected, objects)?.type === 'raster_image',
    canUseAsImageMask:
      !hasLocked &&
      selectedRasterObject !== null &&
      selectedMaskObjects.length > 0,
    imageMaskTargetId: selectedRasterObject?.id ?? null,
    imageMaskObjectIds: selectedMaskObjects.map((object) => object.id),
    imageMaskSelectionHasInvalidMasks,
    canRemoveImageMask:
      selectedObjectIds.length === 1 &&
      !hasLocked &&
      singleSelected?.data.type === 'raster_image' &&
      (singleSelected.data.masks?.length ?? 0) > 0,
    hiddenPanelIds,
  };
}
