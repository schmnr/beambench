/**
 * Beam Bench layer-family resolver.
 *
 * Layers sharing the same `color_tag` form a "family". Within a
 * family each color can carry at most two content rows: an image
 * sibling (operation === 'image') and a non-image sibling
 * (everything else — line/cut/score/fill/offset_fill). Raster
 * content always targets the image sibling; vector/text/shape
 * content always targets the non-image sibling. Siblings are
 * created lazily the first time each content type lands on a color.
 *
 * This module is the single source of truth for "where does a new
 * object go" across canvas tools, `projectStore.addObject`, import
 * flows, and palette-driven reassignment. It mirrors the backend's
 * `resolve_layer_for_object` + `effective_is_raster` (see
 * `crates/beambench-service/src/validation.rs`) so both sides
 * agree on classification and destination selection.
 *
 * The resolver is a pure synchronous function — async layer
 * creation happens in the caller via a two-step handshake:
 *   1. Call `resolveDestinationLayer(...)`.
 *   2. If it returns `{ needsBackendCreate }`, `await
 *      projectService.addLayer(...)` + `updateLayer(... color_tag ...)`,
 *      then re-invoke with the freshly-created layer present in
 *      `project.layers`.
 *   3. The second call returns a concrete `ResolveResult`.
 */

import type { Layer, OperationType, Project } from '../types/project';
import type { ContentKind } from '../commands/selectionContext';
import { PALETTE_COLORS } from '../constants/palette';

export type { ContentKind };

/** Sentinel layer id meaning "no specific layer — use the fallback
 *  auto-create path". Matches the existing `projectStore.addObject`
 *  convention. */
export const AUTO_LAYER_ID = '__auto__';

export interface ResolveInput {
  project: Project;
  /** Layer id the caller wants to target. May be `AUTO_LAYER_ID`
   *  (fresh-project fallback) or a stale id no longer in the
   *  project. */
  requestedLayerId?: string | null;
  /** Pending palette color waiting for its first object (set via
   *  `setPendingPaletteColor`). Takes precedence over
   *  `requestedLayerId` / `selectedLayerId` for target-color
   *  selection. */
  pendingColor?: string | null;
  /** Currently-active row in the Cuts/Layers panel. Used as the
   *  fallback target color when no pending color or requested
   *  layer applies. */
  selectedLayerId?: string | null;
  contentKind: ContentKind;
}

export interface ResolveResult {
  kind: 'resolved';
  /** Concrete destination layer id to pass to the backend. */
  layerId: string;
  /** Row the UI should activate after the mutation lands.
   *  Equal to `layerId` today, carried separately in case future
   *  flows want to surface the creating user back to a different
   *  row. */
  nextSelectedLayerId: string;
}

export interface NeedsBackendCreate {
  kind: 'needs_create';
  /** `color_tag` to apply to the new layer after addLayer. Always
   *  lowercase to match the comparison everywhere else in the
   *  store. */
  colorTag: string;
  /** Operation type for the new sibling. For `'raster'` content
   *  this is always `'image'`; for `'non_raster'` this is the
   *  modern `'line'` operation. Existing legacy Cut/Score siblings
   *  are still reused when present, but new non-image siblings do
   *  not revive those retired modes. */
  operation: OperationType;
  /** Layer to copy base settings from (speed/power), if the family
   *  had any existing member. `null` if the family is empty or no
   *  target color could be determined. */
  copyFrom: Layer | null;
  /** Human-readable name for the new layer. Follows the
   *  "inherit the color label, add a suffix if needed" convention. */
  suggestedName: string;
}

export type ResolveOutput = ResolveResult | NeedsBackendCreate;

const DEFAULT_AUTO_BASE_NAMES = new Set(['Layer', 'Line', 'Fill', 'Offset Fill', 'Image', 'Cut', 'Score']);

/** Returns `true` iff the layer can host raster objects (only
 *  `operation === 'image'`). All other operations — line, cut,
 *  score, fill, offset_fill — host non-raster objects per the
 *  backend layer-content invariant. */
function isImageLayer(layer: Layer): boolean {
  return layer.entries[0]?.operation === 'image';
}

/** Does the layer's operation match the requested content kind? */
function layerMatchesKind(layer: Layer, kind: ContentKind): boolean {
  if (layer.is_tool_layer) return true;
  return kind === 'raster' ? isImageLayer(layer) : !isImageLayer(layer);
}

/** Normalize color_tag: lowercase + strip 8-digit RGBA to 6-digit
 *  RGB (e.g. `#ff0000ff` → `#ff0000`). Prevents family-matching
 *  misses between alpha-suffixed tags and plain palette colors. */
export function normColor(s: string | null | undefined): string | null {
  if (!s) return null;
  let h = s.toLowerCase().trim();
  if (h.length === 9 && h.startsWith('#')) h = h.slice(0, 7);
  return h;
}

/** Collect all layers sharing a given `color_tag` (normalized). */
function familyFor(project: Project, colorTag: string): Layer[] {
  const target = normColor(colorTag);
  return project.layers
    .filter((l) => normColor(l.color_tag) === target)
    .slice()
    .sort((a, b) => a.order_index - b.order_index);
}

/** Pick a `Layer` by id, returning null if not found. */
function findLayer(project: Project, id: string | null | undefined): Layer | null {
  if (!id || id === AUTO_LAYER_ID) return null;
  return project.layers.find((l) => l.id === id) ?? null;
}

/**
 * Resolve the destination layer for a new or reassigned object.
 * See module docstring for the two-step handshake contract.
 */
export function resolveDestinationLayer(input: ResolveInput): ResolveOutput {
  const { project, requestedLayerId, pendingColor, selectedLayerId, contentKind } = input;

  // --- Step 1: determine the target color.
  // Priority: pending palette color → requested layer → selected layer → null.
  const pending = normColor(pendingColor ?? null);
  const requested = findLayer(project, requestedLayerId);
  const selected = findLayer(project, selectedLayerId);
  const requestedTool = requested?.is_tool_layer === true;
  const targetColor: string | null =
    pending
    ?? normColor(requested?.color_tag ?? null)
    ?? normColor(selected?.color_tag ?? null);

  const targetPaletteEntry = PALETTE_COLORS.find((c) => normColor(c.hex) === targetColor);
  const targetIsToolColor = targetPaletteEntry?.is_tool_layer === true;

  if (requestedTool && (!pending || normColor(requested.color_tag) === pending)) {
    return { kind: 'resolved', layerId: requested.id, nextSelectedLayerId: requested.id };
  }

  if (targetIsToolColor) {
    const existingTool = project.layers.find(
      (layer) => layer.is_tool_layer && normColor(layer.color_tag) === targetColor,
    );
    if (existingTool) {
      return { kind: 'resolved', layerId: existingTool.id, nextSelectedLayerId: existingTool.id };
    }
    return {
      kind: 'needs_create',
      colorTag: targetColor ?? targetPaletteEntry.hex,
      operation: 'tool',
      copyFrom: null,
      suggestedName: targetPaletteEntry.name.replace('Tool ', 'T'),
    };
  }

  // --- Step 2: if caller requested a concrete layer that already
  // matches the content kind, short-circuit only when there is no
  // higher-priority pending palette color overriding the family.
  // This preserves intentional direct targeting (e.g. drag-drop
  // onto a specific row) without letting a stale selected row win
  // over "the next object should use this new color".
  if (
    requested
    && layerMatchesKind(requested, contentKind)
    && (!pending || normColor(requested.color_tag) === pending)
  ) {
    return { kind: 'resolved', layerId: requested.id, nextSelectedLayerId: requested.id };
  }

  // --- Step 3: no target color at all → fall back to __auto__.
  // The store's addObject path already knows how to handle this:
  // it creates a default Line layer if no layers exist, or picks
  // the first layer otherwise. This branch matters for fresh
  // projects and for callers that passed neither selection nor
  // pending color.
  if (!targetColor) {
    return { kind: 'resolved', layerId: AUTO_LAYER_ID, nextSelectedLayerId: AUTO_LAYER_ID };
  }

  // --- Step 4: walk the family and try to find a matching sibling.
  const family = familyFor(project, targetColor);
  const match = family.find((l) => layerMatchesKind(l, contentKind));
  if (match) {
    return { kind: 'resolved', layerId: match.id, nextSelectedLayerId: match.id };
  }

  // --- Step 5: no matching sibling — build a `needs_create`
  // request. The caller will addLayer + updateLayer(color_tag),
  // then re-call this function; step 4 will find the new sibling.
  let operation: OperationType;
  if (contentKind === 'raster') {
    operation = 'image';
  } else {
    operation = 'line';
  }

  const copyFrom = family[0] ?? null;
  const suggestedName = buildSiblingName(family, targetColor, operation, contentKind);

  return {
    kind: 'needs_create',
    colorTag: targetColor,
    operation,
    copyFrom,
    suggestedName,
  };
}

/** Name the new sibling layer consistently:
 *  - If the family already has a member, reuse its name + a mode
 *    suffix so the Cuts/Layers panel shows the pair clearly.
 *  - If the family is empty, use a generic name based on operation.
 */
function buildSiblingName(
  family: Layer[],
  colorTag: string,
  operation: OperationType,
  _kind: ContentKind,
): string {
  const base = familyBaseName(family, colorTag);
  return `${base} (${operationLabel(operation)})`;
}

function operationLabel(op: OperationType): string {
  switch (op) {
    case 'tool': return 'Tool';
    case 'image': return 'Image';
    case 'line': return 'Line';
    case 'cut': return 'Cut';
    case 'score': return 'Score';
    case 'fill': return 'Fill';
    case 'offset_fill': return 'Offset Fill';
  }
}

function stripModeSuffix(name: string): string {
  return name.replace(/\s*\((Image|Line|Cut|Score|Fill|Offset Fill)\)$/i, '').trim();
}

function familyBaseName(family: Layer[], colorTag: string): string {
  const baseFromFamily = family[0]?.name?.trim();
  if (baseFromFamily) {
    const stripped = stripModeSuffix(baseFromFamily);
    if (stripped.length > 0 && !DEFAULT_AUTO_BASE_NAMES.has(stripped)) {
      return stripped;
    }
  }
  return familyLabelForColor(colorTag);
}

function familyLabelForColor(colorTag: string): string {
  const norm = normColor(colorTag) ?? '';
  const idx = PALETTE_COLORS.findIndex((c) => (normColor(c.hex) ?? '') === norm);
  if (idx >= 0) {
    const entry = PALETTE_COLORS[idx];
    if (entry.is_tool_layer) return `T${idx - 29}`;
    return `C${String(idx).padStart(2, '0')}`;
  }
  return 'Layer';
}
