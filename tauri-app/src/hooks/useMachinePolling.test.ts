import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook } from '@testing-library/react';
import { useMachineStore } from '../stores/machineStore';
import { useMachinePolling } from './useMachinePolling';

// Mock machineService
vi.mock('../services/machineService', () => ({
  machineService: {
    listSerialPorts: vi.fn(),
    connect: vi.fn(),
    disconnect: vi.fn(),
    getMachineStatus: vi.fn(),
    getSessionState: vi.fn(),
    home: vi.fn(),
    unlock: vi.fn(),
    jog: vi.fn(),
    runPreflightCheck: vi.fn(),
    startJob: vi.fn(),
    getJobProgress: vi.fn(),
    pauseJob: vi.fn(),
    resumeJob: vi.fn(),
    cancelJob: vi.fn(),
    getMachineProfiles: vi.fn(),
    saveMachineProfile: vi.fn(),
    deleteMachineProfile: vi.fn(),
    setActiveProfile: vi.fn(),
  },
}));

describe('useMachinePolling', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    useMachineStore.setState({
      sessionState: 'disconnected',
      machineStatus: null,
      jobProgress: null,
      error: null,
    });
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it('hydrates backend session while disconnected without starting connected polling', () => {
    const refreshStatus = vi.fn();
    const refreshSessionState = vi.fn();
    const refreshJobProgress = vi.fn();
    const hydrateSession = vi.fn().mockResolvedValue(undefined);
    useMachineStore.setState({
      refreshStatus,
      refreshSessionState,
      refreshJobProgress,
      hydrateSession,
    });

    renderHook(() => useMachinePolling());

    expect(hydrateSession).toHaveBeenCalledTimes(1);

    vi.advanceTimersByTime(1000);

    expect(refreshStatus).not.toHaveBeenCalled();
    expect(refreshSessionState).not.toHaveBeenCalled();
    expect(refreshJobProgress).not.toHaveBeenCalled();
    expect(hydrateSession).toHaveBeenCalledTimes(1);

    vi.advanceTimersByTime(1000);

    expect(hydrateSession).toHaveBeenCalledTimes(2);
  });

  it('refreshes status immediately and then polls when session is ready', () => {
    const refreshStatus = vi.fn().mockResolvedValue(undefined);
    const refreshSessionState = vi.fn().mockResolvedValue(undefined);
    useMachineStore.setState({
      sessionState: 'ready',
      refreshStatus,
      refreshSessionState,
    });

    renderHook(() => useMachinePolling());

    expect(refreshStatus).toHaveBeenCalledTimes(1);
    expect(refreshSessionState).toHaveBeenCalledTimes(1);

    vi.advanceTimersByTime(2000);

    expect(refreshStatus).toHaveBeenCalledTimes(2);
    expect(refreshSessionState).toHaveBeenCalledTimes(2);

    vi.advanceTimersByTime(2000);

    expect(refreshStatus).toHaveBeenCalledTimes(3);
    expect(refreshSessionState).toHaveBeenCalledTimes(3);
  });

  it('refreshes immediately and uses fast polling interval when session is active', () => {
    const refreshStatus = vi.fn().mockResolvedValue(undefined);
    const refreshSessionState = vi.fn().mockResolvedValue(undefined);
    useMachineStore.setState({
      sessionState: 'running',
      refreshStatus,
      refreshSessionState,
    });

    renderHook(() => useMachinePolling());

    expect(refreshStatus).toHaveBeenCalledTimes(1);
    expect(refreshSessionState).toHaveBeenCalledTimes(1);

    vi.advanceTimersByTime(500);

    expect(refreshStatus).toHaveBeenCalledTimes(2);
    expect(refreshSessionState).toHaveBeenCalledTimes(2);

    vi.advanceTimersByTime(500);

    expect(refreshStatus).toHaveBeenCalledTimes(3);
    expect(refreshSessionState).toHaveBeenCalledTimes(3);
  });

  it('starts job polling when job is running', () => {
    const refreshJobProgress = vi.fn().mockResolvedValue(undefined);
    useMachineStore.setState({
      sessionState: 'running',
      jobProgress: {
        state: 'running',
        total_lines: 100,
        queued_lines: 0,
        sent_lines: 50,
        acknowledged_lines: 45,
        elapsed_secs: 30,
        estimated_remaining_secs: 30,
        buffer_fill_bytes: 64,
      },
      refreshJobProgress,
    });

    renderHook(() => useMachinePolling());

    vi.advanceTimersByTime(250);

    expect(refreshJobProgress).toHaveBeenCalledTimes(1);

    vi.advanceTimersByTime(250);

    expect(refreshJobProgress).toHaveBeenCalledTimes(2);
  });

  it('does not poll job when job state is idle', () => {
    const refreshJobProgress = vi.fn().mockResolvedValue(undefined);
    useMachineStore.setState({
      sessionState: 'ready',
      jobProgress: {
        state: 'completed',
        total_lines: 100,
        queued_lines: 0,
        sent_lines: 100,
        acknowledged_lines: 100,
        elapsed_secs: 60,
        estimated_remaining_secs: 0,
        buffer_fill_bytes: 0,
      },
      refreshJobProgress,
    });

    renderHook(() => useMachinePolling());

    vi.advanceTimersByTime(1000);

    expect(refreshJobProgress).not.toHaveBeenCalled();
  });

  it('cleans up intervals on unmount', () => {
    const refreshStatus = vi.fn().mockResolvedValue(undefined);
    const refreshSessionState = vi.fn().mockResolvedValue(undefined);
    useMachineStore.setState({
      sessionState: 'ready',
      refreshStatus,
      refreshSessionState,
    });

    const { unmount } = renderHook(() => useMachinePolling());

    expect(refreshStatus).toHaveBeenCalledTimes(1);

    vi.advanceTimersByTime(2000);
    expect(refreshStatus).toHaveBeenCalledTimes(2);

    unmount();

    vi.advanceTimersByTime(2000);
    // Should not have been called again after unmount
    expect(refreshStatus).toHaveBeenCalledTimes(2);
  });
});
