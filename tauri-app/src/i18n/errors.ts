import i18n from './index';

const MACHINE_ZERO_REQUIRES_HOME =
  'Machine-zero moves require homing in the current session first';

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
  return i18n.t('errors.operation_failed_with_detail', { detail });
}
