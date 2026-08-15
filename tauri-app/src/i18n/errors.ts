import i18n from './index';

const MACHINE_ZERO_REQUIRES_HOME =
  'Machine-zero moves require homing in the current session first';
const SERIAL_PORT_UNAVAILABLE =
  /\[serial_port_unavailable\]\s+Could not open ([^:]+):/u;
const LIHUIYU_INCOMPATIBLE_WINDOWS_DRIVER = '[lihuiyu_incompatible_windows_driver]';
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
  return i18n.t('errors.operation_failed_with_detail', {
    detail: INTERNAL_SAFETY_MARKER.test(normalized)
      ? normalized.replace(INTERNAL_SAFETY_MARKER, '')
      : detail,
  });
}

/** Read the useful message from either a structured Tauri error or a legacy error. */
export function backendErrorMessage(error: unknown): string {
  if (
    typeof error === 'object'
    && error !== null
    && 'message' in error
    && typeof error.message === 'string'
  ) {
    return error.message;
  }
  return String(error);
}
