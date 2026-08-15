import i18n from './index';
import type { Project, WorkspaceOrigin } from '../types/project';
import type { DisplayUnit } from '../utils/lengthUnits';
import { lengthUnitLabel, mmToDisplay } from '../utils/lengthUnits';

type BoundsAxis = 'x' | 'y';
type BoundsBoundary = 'min' | 'max';

interface BoundsViolation {
  axis: BoundsAxis;
  boundary: BoundsBoundary;
  amount_mm: number;
  source_object_id?: string;
}

interface BoundsErrorDetails {
  kind: 'bounds_exceeded';
  violation: BoundsViolation;
  workspace_origin?: WorkspaceOrigin;
}

export interface ActionableBoundsError {
  message: string;
  sourceObjectId: string | null;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function parseJsonRecord(value: string): Record<string, unknown> | null {
  try {
    const parsed: unknown = JSON.parse(value);
    return isRecord(parsed) ? parsed : null;
  } catch {
    return null;
  }
}

function errorRecord(error: unknown): Record<string, unknown> | null {
  if (isRecord(error) && 'details' in error) return error;
  if (error instanceof Error) return parseJsonRecord(error.message);
  if (typeof error === 'string') return parseJsonRecord(error);
  return null;
}

function parseBoundsDetails(error: unknown): BoundsErrorDetails | null {
  const record = errorRecord(error);
  if (!record || !isRecord(record.details)) return null;
  const details = record.details;
  if (details.kind !== 'bounds_exceeded' || !isRecord(details.violation)) return null;

  const violation = details.violation;
  if (
    (violation.axis !== 'x' && violation.axis !== 'y')
    || (violation.boundary !== 'min' && violation.boundary !== 'max')
    || typeof violation.amount_mm !== 'number'
    || !Number.isFinite(violation.amount_mm)
  ) {
    return null;
  }

  const workspaceOrigin = details.workspace_origin === 'bottom_left'
    || details.workspace_origin === 'top_left'
    ? details.workspace_origin
    : undefined;
  return {
    kind: 'bounds_exceeded',
    violation: {
      axis: violation.axis,
      boundary: violation.boundary,
      amount_mm: Math.abs(violation.amount_mm),
      source_object_id: typeof violation.source_object_id === 'string'
        ? violation.source_object_id
        : undefined,
    },
    workspace_origin: workspaceOrigin,
  };
}

function visualEdge(
  axis: BoundsAxis,
  boundary: BoundsBoundary,
  origin: WorkspaceOrigin,
): 'left' | 'right' | 'top' | 'bottom' {
  if (axis === 'x') return boundary === 'min' ? 'left' : 'right';
  if (origin === 'top_left') return boundary === 'min' ? 'top' : 'bottom';
  return boundary === 'min' ? 'bottom' : 'top';
}

export function formatWorkspaceBoundsError(
  error: unknown,
  project: Project | null,
  displayUnit: DisplayUnit = 'mm',
): ActionableBoundsError | null {
  const details = parseBoundsDetails(error);
  if (!details) return null;

  const explicitSourceObject = details.violation.source_object_id
    ? project?.objects.find((object) => object.id === details.violation.source_object_id)
    : undefined;
  const outputLayerIds = new Set(
    project?.layers
      .filter((layer) => layer.enabled && !layer.is_tool_layer)
      .map((layer) => layer.id) ?? [],
  );
  const visibleOutputObjects = project?.objects.filter(
    (object) => object.visible !== false && outputLayerIds.has(object.layer_id),
  ) ?? [];
  // Raster planning segments do not always retain a source object id. When
  // there is only one possible output object, selecting it is still exact and
  // gives the user an immediate route to inspect or move it.
  const sourceObject = explicitSourceObject
    ?? (visibleOutputObjects.length === 1 ? visibleOutputObjects[0] : undefined);
  const origin = project?.workspace.origin ?? details.workspace_origin ?? 'top_left';
  const edge = i18n.t(`errors.workspace_edge_${visualEdge(
    details.violation.axis,
    details.violation.boundary,
    origin,
  )}`);
  const displayAmount = mmToDisplay(details.violation.amount_mm, displayUnit);
  const amount = new Intl.NumberFormat(i18n.resolvedLanguage ?? i18n.language, {
    maximumFractionDigits: displayUnit === 'inches' ? 4 : 2,
  }).format(displayAmount);

  return {
    message: i18n.t('errors.artwork_outside_workspace', {
      hasObject: sourceObject ? 'yes' : 'no',
      object: sourceObject?.name ?? '',
      amount,
      unit: lengthUnitLabel(displayUnit),
      edge,
    }),
    sourceObjectId: sourceObject?.id ?? null,
  };
}
