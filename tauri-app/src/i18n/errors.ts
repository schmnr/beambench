import i18n from './index';
import type { FeedbackSourceContext } from '../types/feedback';

const MACHINE_ZERO_REQUIRES_HOME = 'Machine-zero moves require homing in the current session first';
const SERIAL_PORT_UNAVAILABLE = /\[serial_port_unavailable\]\s+Could not open ([^:]+):/u;
const LIHUIYU_INCOMPATIBLE_WINDOWS_DRIVER = '[lihuiyu_incompatible_windows_driver]';
const USER_ORIGIN_NOT_SET = 'User Origin is selected, but no user origin has been set.';
const CURRENT_POSITION_UNAVAILABLE =
  'Current Position requires a connected machine with a reported work position.';
const DXF_NO_USABLE_GEOMETRY =
  /^DXF import found no usable 2D vector geometry\.(?: Unsupported or malformed entities: (.+)\.)?$/u;
const DXF_SKIPPED_ENTITIES =
  /^DXF import skipped unsupported or malformed entities: (.+)\.$/u;
const INTERNAL_SAFETY_MARKER = /^\[(?:controller_connection_lost|emergency_stop_unconfirmed)\]\s*/u;

/**
 * Localize a raw backend error string for display to the user.
 *
 * Backend commands return plain English error strings across the IPC bridge.
 * We wrap them in a localized frame ("Operation failed: …") while preserving
 * the original detail verbatim. Known exact safety errors may get a friendlier
 * instruction, but we deliberately do NOT pattern-match keywords: backend
 * errors are often complete, meaningful sentences (e.g. rate-limit guidance),
 * and keyword matching mis-categorized them and discarded useful information.
 */
export function wrapBackendError(detail: string): string {
  const normalized = detail.replace(/^(?:Error:\s*|Operation failed:\s*)+/u, '');
  if (normalized === MACHINE_ZERO_REQUIRES_HOME) {
    return i18n.t('errors.machine_zero_requires_home');
  }
  const unavailablePort = normalized.match(SERIAL_PORT_UNAVAILABLE);
  if (unavailablePort) {
    return i18n.t('errors.serial_port_unavailable', { port: unavailablePort[1].trim() });
  }
  if (normalized.includes(LIHUIYU_INCOMPATIBLE_WINDOWS_DRIVER)) {
    return i18n.t('errors.lihuiyu_incompatible_windows_driver');
  }
  if (normalized.startsWith(USER_ORIGIN_NOT_SET)) {
    return i18n.t('errors.user_origin_not_set');
  }
  if (normalized.startsWith(CURRENT_POSITION_UNAVAILABLE)) {
    return i18n.t('errors.current_position_unavailable');
  }
  const noUsableDxfGeometry = normalized.match(DXF_NO_USABLE_GEOMETRY);
  if (noUsableDxfGeometry) {
    return noUsableDxfGeometry[1]
      ? i18n.t('errors.dxf_no_usable_geometry_with_entities', {
          entities: noUsableDxfGeometry[1],
        })
      : i18n.t('errors.dxf_no_usable_geometry');
  }
  return i18n.t('errors.operation_failed_with_detail', {
    detail: INTERNAL_SAFETY_MARKER.test(normalized)
      ? normalized.replace(INTERNAL_SAFETY_MARKER, '')
      : detail,
  });
}

/** Localize known import warnings while preserving useful unknown details. */
export function localizeImportWarning(warning: string): string {
  const skippedDxfEntities = warning.match(DXF_SKIPPED_ENTITIES);
  if (skippedDxfEntities) {
    return i18n.t('notifications.dxf_skipped_entities', {
      entities: skippedDxfEntities[1],
    });
  }
  return warning;
}

/** Read the useful message from either a structured Tauri error or a legacy error. */
export function backendErrorMessage(error: unknown): string {
  if (
    typeof error === 'object' &&
    error !== null &&
    'message' in error &&
    typeof error.message === 'string'
  ) {
    return error.message;
  }
  return String(error);
}

function structuredErrorRecord(error: unknown): Record<string, unknown> | null {
  if (typeof error === 'object' && error !== null) {
    return error as Record<string, unknown>;
  }
  if (typeof error !== 'string') return null;
  try {
    const parsed: unknown = JSON.parse(error);
    return typeof parsed === 'object' && parsed !== null
      ? (parsed as Record<string, unknown>)
      : null;
  } catch {
    return null;
  }
}

/** Preserve safe structured backend fields for an opt-in diagnostic report. */
export function backendErrorReportContext(
  error: unknown,
): Pick<FeedbackSourceContext, 'error_code' | 'error_details'> {
  const record = structuredErrorRecord(error);
  return {
    error_code: typeof record?.code === 'string' ? record.code : null,
    error_details: record?.details ?? null,
  };
}
