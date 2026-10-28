import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { useRemoteHostStore } from '../stores/remoteHostStore';
import { invocationFailureContext, invoke } from './invoke';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

describe('failed invocation diagnostics', () => {
  beforeEach(() => {
    vi.mocked(tauriInvoke).mockReset();
  });

  it('preserves successful values and invocation arguments', async () => {
    vi.mocked(tauriInvoke).mockResolvedValue({ ok: true });
    expect(await invoke('successful_command', { id: 'local-id' })).toEqual({ ok: true });
    expect(tauriInvoke).toHaveBeenCalledWith('successful_command', { id: 'local-id' });
    expect(invocationFailureContext('success', (detail) => detail)).toBeUndefined();
  });

  it('preserves the original rejection and matches its localized toast without saving arguments', async () => {
    vi.mocked(tauriInvoke).mockRejectedValue('No active machine profile');
    await expect(invoke('bind_active_machine_profile', { privatePath: '/private/design.svg' }))
      .rejects.toBe('No active machine profile');
    const context = invocationFailureContext('Configure a machine', () => 'Configure a machine');
    expect(context?.action).toBe('bind_active_machine_profile');
    expect(context?.error_details).toEqual({ backend_error: 'No active machine profile' });
    expect(JSON.stringify(context)).not.toContain('/private/design.svg');
  });

  it('correlates separate errors and keeps the latest matching command', async () => {
    vi.mocked(tauriInvoke).mockRejectedValueOnce('first failure').mockRejectedValueOnce('second failure');
    await Promise.allSettled([invoke('first_action'), invoke('second_action')]);
    expect(invocationFailureContext('first failure', (detail) => detail)?.action).toBe('first_action');
    expect(invocationFailureContext('second failure', (detail) => detail)?.action).toBe('second_action');
    vi.mocked(tauriInvoke).mockRejectedValue('first failure');
    await expect(invoke('retried_action')).rejects.toBe('first failure');
    expect(invocationFailureContext('first failure', (detail) => detail)?.action).toBe('retried_action');
  });

  it('does not attribute a later notification to an old command failure', async () => {
    const now = vi.spyOn(Date, 'now').mockReturnValue(1_000_000);
    try {
      vi.mocked(tauriInvoke).mockRejectedValue('expired failure');
      await expect(invoke('expired_action')).rejects.toBe('expired failure');
      now.mockReturnValue(1_030_001);
      expect(invocationFailureContext('expired failure', (detail) => detail)).toBeUndefined();
    } finally {
      now.mockRestore();
    }
  });
});

describe('machine destination guard', () => {
  beforeEach(() => { vi.mocked(tauriInvoke).mockReset().mockResolvedValue(undefined); });
  afterEach(() => { useRemoteHostStore.setState({ enabled: false }); vi.clearAllMocks(); });
  it('routes global emergency stop to the selected host without waiting for pending work', async () => {
    useRemoteHostStore.setState({ enabled: true, busy: true });
    await invoke('emergency_stop');
    expect(tauriInvoke).toHaveBeenCalledExactlyOnceWith('remote_host_request', { path: '/api/v1/machine/emergency-stop', body: {} });
  });
  it('blocks local motion and laser starts while allowing normal project editing', async () => {
    useRemoteHostStore.setState({ enabled: true });
    for (const command of ['start_job', 'machine_jog', 'begin_controller_connection', 'quality_test_start', 'quality_test_frame', 'laser_fire_start']) {
      await expect(invoke(command)).rejects.toThrow('remote host controls');
    }
    expect(tauriInvoke).not.toHaveBeenCalled();
    await invoke('set_start_from', { mode: 'absolute_coords' });
    expect(tauriInvoke).toHaveBeenCalledExactlyOnceWith('set_start_from', { mode: 'absolute_coords' });
  });
});
