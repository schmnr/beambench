import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { check } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import {
  checkForUpdate,
  clearPendingUpdate,
  downloadAndInstallUpdate,
  getUpdateInstallBlocker,
  UpdateInstallBlockedError,
} from './updateService';
import { useMachineStore } from '../stores/machineStore';

vi.mock('@tauri-apps/plugin-updater', () => ({
  check: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-process', () => ({
  relaunch: vi.fn(),
}));

const initialMachineState = useMachineStore.getState();

function makeUpdate() {
  return {
    currentVersion: '1.0.0',
    version: '1.0.1',
    date: '2026-05-12T00:00:00Z',
    body: 'Bug fixes',
    rawJson: {},
    download: vi.fn(async (onEvent?: (event: unknown) => void) => {
      onEvent?.({ event: 'Started', data: { contentLength: 100 } });
      onEvent?.({ event: 'Progress', data: { chunkLength: 25 } });
      onEvent?.({ event: 'Progress', data: { chunkLength: 75 } });
      onEvent?.({ event: 'Finished' });
    }),
    install: vi.fn().mockResolvedValue(undefined),
    close: vi.fn().mockResolvedValue(undefined),
  };
}

describe('updateService', () => {
  beforeEach(() => {
    vi.mocked(check).mockReset();
    vi.mocked(relaunch).mockReset();
    useMachineStore.setState(initialMachineState, true);
    clearPendingUpdate();
  });

  afterEach(() => {
    clearPendingUpdate();
    useMachineStore.setState(initialMachineState, true);
  });

  it('maps available updates from the Tauri updater', async () => {
    vi.mocked(check).mockResolvedValue(makeUpdate() as never);

    const update = await checkForUpdate();

    expect(update).toMatchObject({
      currentVersion: '1.0.0',
      version: '1.0.1',
      body: 'Bug fixes',
    });
  });

  it('blocks install while a job is active', async () => {
    const update = makeUpdate();
    vi.mocked(check).mockResolvedValue(update as never);
    await checkForUpdate();
    useMachineStore.setState({
      jobProgress: {
        state: 'running',
        total_lines: 10,
        queued_lines: 0,
        sent_lines: 1,
        acknowledged_lines: 1,
        elapsed_secs: 1,
        estimated_remaining_secs: 9,
        buffer_fill_bytes: 0,
      },
    });

    await expect(downloadAndInstallUpdate()).rejects.toBeInstanceOf(UpdateInstallBlockedError);
    expect(update.download).not.toHaveBeenCalled();
    expect(update.install).not.toHaveBeenCalled();
    expect(relaunch).not.toHaveBeenCalled();
    expect(getUpdateInstallBlocker()).toBe('Finish or stop the current job before installing.');
  });

  it('downloads, installs, and relaunches when the machine is idle', async () => {
    const update = makeUpdate();
    vi.mocked(check).mockResolvedValue(update as never);
    await checkForUpdate();
    const progress = vi.fn();

    await downloadAndInstallUpdate(progress);

    expect(update.download).toHaveBeenCalledOnce();
    expect(update.install).toHaveBeenCalledOnce();
    expect(relaunch).toHaveBeenCalledOnce();
    expect(progress).toHaveBeenLastCalledWith({
      phase: 'finished',
      downloadedBytes: 100,
      totalBytes: 100,
      percent: 100,
    });
  });
});
