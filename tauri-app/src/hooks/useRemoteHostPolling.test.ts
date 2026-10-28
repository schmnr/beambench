import { act, cleanup, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useRemoteHostPolling } from './useRemoteHostPolling';
import { useRemoteHostStore, type RemoteSnapshot } from '../stores/remoteHostStore';
import { remoteHostService } from '../services/remoteHostService';

vi.mock('../services/remoteHostService', () => ({
  remoteHostService: {
    settings: vi.fn(),
    status: vi.fn(),
    request: vi.fn(),
    prepare: vi.fn(),
  },
}));
const snapshot = {
  boot_id: 'host-1',
  transfer_id: 'job-1',
  owned: true,
  consumed: true,
  progress: { state: 'running', acknowledged_lines: 8 },
  machine: { session_state: 'running' },
} as RemoteSnapshot;

describe('remote reconnect', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.clearAllMocks();
    useRemoteHostStore.setState({
      enabled: true,
      configured: false,
      online: false,
      snapshot: null,
      review: null,
      error: null,
      lastSeen: null,
    });
    vi.mocked(remoteHostService.settings).mockResolvedValue({
      url: 'http://host:5900',
      configured: true,
    });
  });
  afterEach(() => {
    cleanup();
    useRemoteHostStore.setState({ enabled: false });
    vi.useRealTimers();
  });
  it('keeps the last snapshot during an outage and reconnects by reading state only', async () => {
    vi.mocked(remoteHostService.status)
      .mockResolvedValueOnce(snapshot)
      .mockRejectedValueOnce(new Error('offline'))
      .mockResolvedValue({
        ...snapshot,
        progress: { ...snapshot.progress!, state: 'completed', acknowledged_lines: 24 },
      });
    renderHook(useRemoteHostPolling);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(useRemoteHostStore.getState().online).toBe(true);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1500);
    });
    expect(useRemoteHostStore.getState().online).toBe(false);
    expect(useRemoteHostStore.getState().snapshot?.progress?.state).toBe('running');
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1500);
    });
    expect(useRemoteHostStore.getState().online).toBe(true);
    expect(useRemoteHostStore.getState().snapshot?.progress?.state).toBe('completed');
    expect(remoteHostService.request).not.toHaveBeenCalled();
    expect(remoteHostService.prepare).not.toHaveBeenCalled();
  });
  it('drops a prior review when the host restarts instead of replaying it', async () => {
    vi.mocked(remoteHostService.status).mockResolvedValue({
      ...snapshot,
      boot_id: 'host-2',
      transfer_id: null,
    });
    useRemoteHostStore.setState({ snapshot, review: { transfer_id: 'job-1' } as never });
    renderHook(useRemoteHostPolling);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(useRemoteHostStore.getState().review).toBeNull();
    expect(remoteHostService.request).not.toHaveBeenCalled();
  });
});
