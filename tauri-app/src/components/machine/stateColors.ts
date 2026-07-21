import type { JobState, MachineRunState, SessionState } from '../../types/machine';

// Theme-aware state colors for the machine indicators. These must stay theme
// tokens (see machineStateThemeGuard.test.ts): hardcoded hex values rendered
// as ~2:1-contrast text on the light theme.

export const RUN_STATE_TEXT_CLASSES: Record<MachineRunState, string> = {
  idle: 'text-bb-success-fg',
  run: 'text-bb-accent',
  hold: 'text-bb-warning-fg',
  jog: 'text-bb-accent',
  home: 'text-bb-accent',
  alarm: 'text-bb-error-fg',
  door: 'text-bb-warning-fg',
  sleep: 'text-bb-text-muted',
  check: 'text-bb-accent',
  unknown: 'text-bb-text-muted',
};

export const JOB_STATE_TEXT_CLASSES: Record<JobState, string> = {
  idle: 'text-bb-text-muted',
  preparing: 'text-bb-accent',
  ready_to_run: 'text-bb-accent',
  running: 'text-bb-accent',
  paused: 'text-bb-warning-fg',
  completed: 'text-bb-success-fg',
  failed: 'text-bb-error-fg',
  cancelled: 'text-bb-text-muted',
};

export const SESSION_STATE_DOT_CLASSES: Record<SessionState, string> = {
  disconnected: 'bg-bb-text-dim',
  connecting: 'bg-bb-warning',
  transport_open: 'bg-bb-warning',
  waiting_for_banner: 'bg-bb-warning',
  validating: 'bg-bb-warning',
  ready: 'bg-bb-success',
  running: 'bg-bb-accent',
  paused: 'bg-bb-warning',
  alarm: 'bg-bb-error',
  error: 'bg-bb-error',
};
