import i18n from './index';
import type { FeedbackSourceContext } from '../types/feedback';
import { useAppStore } from '../stores/appStore';
import { useProjectStore } from '../stores/projectStore';
import { rasterMotionText } from './preflightText';

const MACHINE_ZERO_REQUIRES_HOME = 'Machine-zero moves require homing in the current session first';
const SERIAL_PORT_UNAVAILABLE = /\[serial_port_unavailable\]\s+Could not open ([^:]+):/u;
const LIHUIYU_INCOMPATIBLE_WINDOWS_DRIVER = '[lihuiyu_incompatible_windows_driver]';
const RUIDA_UNKNOWN_VARIANT = '[ruida_unknown_variant]';
const RUIDA_PROBE_INCONCLUSIVE = '[ruida_probe_inconclusive]';
const USER_ORIGIN_NOT_SET = 'User Origin is selected, but no user origin has been set.';
const CURRENT_POSITION_UNAVAILABLE =
  'Current Position requires a connected machine with a reported work position.';
const RASTER_PLAN_TOO_COMPLEX = '[raster_plan_too_complex]';
const JOB_PREPARATION_STOPPED = '[job_preparation_stopped]';
const SLEEP_PROTECTION_FAILED = '[sleep_protection_failed]';
const PROJECT_FILE_MISSING = '[project_file_missing]';
const SETTINGS_FILE_RECOVERED = '[settings_file_recovered]';
const LAYERS_MIGRATED_ON_OPEN = '[layers_migrated_on_open]';
const EDIT_INVALID_GEOMETRY = '[edit_invalid_geometry]';
const EDIT_INTERNAL_ERROR = '[edit_internal_error]';
const PROJECT_INVALID_VALUE = /\[project_invalid_value\](?: '(.+)' has an invalid size or position)?/u;
const PROJECT_TOO_NEW = /\[project_too_new\][^]*?Beam Bench (\S+?)\.\s/u;
const PROJECT_FROM_NEWER_APP = /\[project_from_newer_app\] This project was saved by Beam Bench (\S+?)\.\s/u;
const DXF_NO_USABLE_GEOMETRY =
  /^DXF import found no usable 2D vector geometry\.(?: Unsupported or malformed entities: (.+)\.)?$/u;
const DXF_SKIPPED_ENTITIES =
  /^DXF import skipped unsupported or malformed entities: (.+)\.$/u;
const SERIAL_OPEN_NO_RESPONSE = '[serial_open_no_response]';
const SERIAL_PROTOCOL_UNRECOGNIZED = '[serial_protocol_unrecognized]';
const NETWORK_GRBL_NO_STATUS = 'The network controller did not return a GRBL status report';
const NO_ACTIVE_MACHINE_PROFILE = 'No active machine profile';
const RASTER_MOTION_OFF_BED =
  /^\[raster_motion_off_bed\] Raster motion spans ([-\d.]+) to ([-\d.]+)mm on the 0 to ([\d.]+)mm ([XY]) axis \(([\d.]+)mm of overscan/u;
const SAFETY_TAG = /^\[(controller_connection_lost|emergency_stop_unconfirmed)\]/u;
/** Any leading `[code]` tag: meant for matching here, never for display. */
const ERROR_CODE_TAG = /^\[[a-z_]+\]\s*/u;

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
  if (normalized.includes(SERIAL_OPEN_NO_RESPONSE)) {
    return i18n.t('errors.serial_open_no_response');
  }
  if (normalized.includes(SERIAL_PROTOCOL_UNRECOGNIZED)) {
    return i18n.t('errors.serial_protocol_unrecognized');
  }
  if (normalized.startsWith(NETWORK_GRBL_NO_STATUS)) {
    return i18n.t('errors.network_grbl_no_status');
  }
  if (normalized === NO_ACTIVE_MACHINE_PROFILE) {
    return i18n.t('errors.no_active_machine_profile');
  }
  const rasterMotion = normalized.match(RASTER_MOTION_OFF_BED);
  if (rasterMotion) {
    const unit = useAppStore.getState().settings?.display_unit === 'inches' ? 'inches' : 'mm';
    return rasterMotionText(
      {
        lo: Number(rasterMotion[1]),
        hi: Number(rasterMotion[2]),
        limit: Number(rasterMotion[3]),
        axis: rasterMotion[4],
        margin: Number(rasterMotion[5]),
      },
      unit,
      useProjectStore.getState().project?.start_from ?? 'absolute_coords',
    );
  }
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
  if (normalized.includes(RUIDA_UNKNOWN_VARIANT)) {
    return i18n.t('errors.ruida_unknown_variant');
  }
  if (normalized.includes(RUIDA_PROBE_INCONCLUSIVE)) {
    return i18n.t('errors.ruida_probe_inconclusive');
  }
  if (normalized.startsWith(USER_ORIGIN_NOT_SET)) {
    return i18n.t('errors.user_origin_not_set');
  }
  if (normalized.startsWith(CURRENT_POSITION_UNAVAILABLE)) {
    return i18n.t('errors.current_position_unavailable');
  }
  if (normalized.includes(RASTER_PLAN_TOO_COMPLEX)) {
    return i18n.t('errors.raster_plan_too_complex');
  }
  if (normalized.startsWith(JOB_PREPARATION_STOPPED)) {
    return i18n.t('errors.job_preparation_stopped');
  }
  if (normalized.startsWith(SLEEP_PROTECTION_FAILED)) {
    return i18n.t('errors.sleep_protection_failed');
  }
  if (normalized.includes(PROJECT_FILE_MISSING)) {
    // Name the menus exactly as this language's menu bar shows them.
    return i18n.t('errors.project_file_missing', {
      recent: i18n.t('menus.recent_projects.label'),
      open: `${i18n.t('menus.file.label')} > ${i18n.t('menus.file.open')}`,
    });
  }
  if (normalized.startsWith(SETTINGS_FILE_RECOVERED)) {
    return i18n.t('errors.settings_file_recovered');
  }
  if (normalized.startsWith(LAYERS_MIGRATED_ON_OPEN)) {
    return i18n.t('errors.layers_migrated_on_open');
  }
  if (normalized.includes(EDIT_INVALID_GEOMETRY)) {
    return i18n.t('errors.edit_invalid_geometry');
  }
  if (normalized.includes(EDIT_INTERNAL_ERROR)) {
    return i18n.t('errors.edit_internal_error');
  }
  const invalidValue = normalized.match(PROJECT_INVALID_VALUE);
  if (invalidValue) {
    return invalidValue[1]
      ? i18n.t('errors.project_invalid_value_object', { name: invalidValue[1] })
      : i18n.t('errors.project_invalid_value');
  }
  const tooNew = normalized.match(PROJECT_TOO_NEW);
  if (tooNew) {
    return i18n.t('errors.project_too_new', { version: tooNew[1] });
  }
  const fromNewerApp = normalized.match(PROJECT_FROM_NEWER_APP);
  if (fromNewerApp) {
    return i18n.t('errors.project_from_newer_app', { version: fromNewerApp[1] });
  }
  const noUsableDxfGeometry = normalized.match(DXF_NO_USABLE_GEOMETRY);
  if (noUsableDxfGeometry) {
    return noUsableDxfGeometry[1]
      ? i18n.t('errors.dxf_no_usable_geometry_with_entities', {
          entities: noUsableDxfGeometry[1],
        })
      : i18n.t('errors.dxf_no_usable_geometry');
  }
  const safety = normalized.match(SAFETY_TAG);
  if (safety) {
    // Safety guidance in the user's language, with the controller detail.
    return i18n.t('errors.operation_failed_with_detail', {
      detail: i18n.t(`errors.${safety[1]}`, { detail: normalized.replace(ERROR_CODE_TAG, '') }),
    });
  }
  return i18n.t('errors.operation_failed_with_detail', {
    detail: ERROR_CODE_TAG.test(normalized) ? normalized.replace(ERROR_CODE_TAG, '') : detail,
  });
}

/** Localize known import warnings while preserving useful unknown details. */
/**
 * Localize a backend message shown where the context already says something
 * failed (a failed job's banner or progress bar): known codes are
 * translated, anything else is shown as-is without an "Operation failed" frame.
 */
export function localizeBackendMessage(detail: string): string {
  const wrapped = wrapBackendError(detail);
  const normalized = detail
    .replace(/^(?:Error:\s*|Operation failed:\s*)+/u, '')
    .replace(ERROR_CODE_TAG, '');
  const unknownFrames = [detail, normalized].map((value) =>
    i18n.t('errors.operation_failed_with_detail', { detail: value }),
  );
  return unknownFrames.includes(wrapped) ? normalized : wrapped;
}

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
