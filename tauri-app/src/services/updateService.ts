import { check, type DownloadEvent, type Update } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import { invoke } from '@tauri-apps/api/core';
import { useMachineStore } from '../stores/machineStore';
import type { JobState, MachineRunState, SessionState } from '../types/machine';

export interface UpdateInfo {
  currentVersion: string;
  version: string;
  date?: string;
  body?: string;
  rawJson: Record<string, unknown>;
}

export interface UpdateProgress {
  phase: 'started' | 'progress' | 'finished';
  downloadedBytes: number;
  totalBytes: number | null;
  percent: number | null;
}

export class UpdateInstallBlockedError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'UpdateInstallBlockedError';
  }
}

const ACTIVE_JOB_STATES = new Set<JobState>(['preparing', 'ready_to_run', 'running', 'paused']);
const BUSY_SESSION_STATES = new Set<SessionState>([
  'connecting',
  'transport_open',
  'waiting_for_banner',
  'validating',
  'running',
  'paused',
]);
const SAFE_JOB_STATES = new Set<JobState>(['idle', 'completed', 'failed', 'cancelled']);

let pendingUpdate: Update | null = null;

function toUpdateInfo(update: Update): UpdateInfo {
  return {
    currentVersion: update.currentVersion,
    version: update.version,
    date: update.date,
    body: update.body,
    rawJson: update.rawJson,
  };
}

function isNonIdleRunState(runState: MachineRunState | undefined): boolean {
  return runState !== undefined && runState !== 'idle';
}

export function getUpdateInstallBlocker(): string | null {
  const machine = useMachineStore.getState();
  const jobState = machine.jobProgress?.state;

  if (jobState && ACTIVE_JOB_STATES.has(jobState)) {
    return 'Finish or stop the current job before installing.';
  }
  if (jobState && !SAFE_JOB_STATES.has(jobState)) {
    return 'Wait for the current machine operation to finish before installing.';
  }
  if (BUSY_SESSION_STATES.has(machine.sessionState)) {
    return 'Finish or stop the current machine operation before installing.';
  }
  if (isNonIdleRunState(machine.machineStatus?.run_state)) {
    return 'Wait for the machine to return to idle before installing.';
  }
  return null;
}

/// Where-the-app-lives problems that make the macOS in-place swap
/// impossible: running translocated or from the mounted DMG
/// ('not_in_applications'), or installed on a different volume than the
/// system temp folder ('other_disk'). Mapped to dialog.update.blocked_*
/// messages by the store.
export type UpdateEnvironmentBlocker = 'not_in_applications' | 'other_disk';

export async function getUpdateEnvironmentBlocker(): Promise<UpdateEnvironmentBlocker | null> {
  try {
    return await invoke<UpdateEnvironmentBlocker | null>('get_update_environment_blocker');
  } catch {
    // Never block an update because the probe itself failed.
    return null;
  }
}

/// The raw failure the updater surfaces when its rename-based swap crosses
/// volumes (EXDEV). Belt-and-suspenders for cases the precheck missed.
export function isCrossDeviceInstallError(error: unknown): boolean {
  const message = error instanceof Error ? error.message : String(error);
  return /cross-device link|os error 18/i.test(message);
}

export async function checkForUpdate(): Promise<UpdateInfo | null> {
  const update = await check();
  if (pendingUpdate && pendingUpdate !== update) {
    void pendingUpdate.close().catch(() => undefined);
  }
  pendingUpdate = update;
  return update ? toUpdateInfo(update) : null;
}

export function clearPendingUpdate(): void {
  if (pendingUpdate) {
    void pendingUpdate.close().catch(() => undefined);
  }
  pendingUpdate = null;
}

export async function downloadAndInstallUpdate(
  onProgress?: (progress: UpdateProgress) => void,
): Promise<void> {
  const update = pendingUpdate ?? await check();
  if (!update) {
    throw new Error('No update is available.');
  }
  pendingUpdate = update;

  const beforeDownloadBlocker = getUpdateInstallBlocker();
  if (beforeDownloadBlocker) {
    throw new UpdateInstallBlockedError(beforeDownloadBlocker);
  }

  let downloadedBytes = 0;
  let totalBytes: number | null = null;
  await update.download((event: DownloadEvent) => {
    switch (event.event) {
      case 'Started':
        downloadedBytes = 0;
        totalBytes = event.data.contentLength ?? null;
        onProgress?.({
          phase: 'started',
          downloadedBytes,
          totalBytes,
          percent: totalBytes ? 0 : null,
        });
        break;
      case 'Progress':
        downloadedBytes += event.data.chunkLength;
        onProgress?.({
          phase: 'progress',
          downloadedBytes,
          totalBytes,
          percent: totalBytes ? Math.min(100, Math.round((downloadedBytes / totalBytes) * 100)) : null,
        });
        break;
      case 'Finished':
        onProgress?.({
          phase: 'finished',
          downloadedBytes,
          totalBytes,
          percent: 100,
        });
        break;
    }
  });

  const beforeInstallBlocker = getUpdateInstallBlocker();
  if (beforeInstallBlocker) {
    throw new UpdateInstallBlockedError(beforeInstallBlocker);
  }

  await update.install();

  const beforeRelaunchBlocker = getUpdateInstallBlocker();
  if (beforeRelaunchBlocker) {
    throw new UpdateInstallBlockedError(beforeRelaunchBlocker);
  }

  await relaunch();
}
