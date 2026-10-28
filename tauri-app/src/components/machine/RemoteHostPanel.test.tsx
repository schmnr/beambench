import '@testing-library/jest-dom/vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useProjectStore } from '../../stores/projectStore';
import { RemoteHostPanel } from './RemoteHostPanel';
import { remoteHostService } from '../../services/remoteHostService';
import {
  useRemoteHostStore,
  type RemoteReview,
  type RemoteSnapshot,
} from '../../stores/remoteHostStore';
import { makeJobProgress, makeMachineProfile, makeProject } from '../../test-utils/projectFixtures';

vi.mock('../../services/remoteHostService', () => ({
  remoteHostService: {
    request: vi.fn(),
    status: vi.fn(),
    settings: vi.fn(),
    review: vi.fn(),
    configure: vi.fn(),
    prepare: vi.fn(),
  },
}));
vi.mock('../dialogs/PreviewWindow', () => ({ PreviewWindow: () => <div /> }));
const profile = makeMachineProfile();
const snapshot = {
  boot_id: 'boot-1',
  owned: true,
  consumed: false,
  busy: false,
  transfer_id: 'transfer-1',
  profiles: [profile],
  active_profile_id: profile.id,
  project_name: 'Host copy',
  machine: { session_state: 'ready' },
  progress: null,
} as RemoteSnapshot;
const review = {
  transfer_id: 'transfer-1',
  project_name: 'Host copy',
  preflight: { outcome: 'pass', checks: [], advisories: [] },
  layers: [],
  workspace: makeProject().workspace,
} as unknown as RemoteReview;

const initialProjectState = useProjectStore.getState();

describe('remote host controls', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useRemoteHostStore.setState({
      enabled: true,
      configured: true,
      url: 'http://host:5900',
      online: true,
      snapshot,
      review,
      busy: false,
      error: null,
    });
    vi.mocked(remoteHostService.status).mockResolvedValue({ ...snapshot, consumed: true });
    vi.mocked(remoteHostService.request).mockResolvedValue({});
  });
  afterEach(() => {
    cleanup();
    useRemoteHostStore.setState({ enabled: false });
    useProjectStore.setState(initialProjectState, true);
  });
  it('edits laptop placement and captures user origin from the host before transfer', async () => {
    const setStartFrom = vi.fn().mockResolvedValue(undefined);
    const setJobOrigin = vi.fn().mockResolvedValue(undefined);
    const setUserOrigin = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({
      project: makeProject({ start_from: 'user_origin', user_origin: null }),
      setStartFrom,
      setJobOrigin,
      setUserOrigin,
    });
    vi.mocked(remoteHostService.request).mockResolvedValue({ position: [12, 34] });
    render(<RemoteHostPanel />);
    fireEvent.change(screen.getByRole('combobox', { name: 'Start From' }), {
      target: { value: 'current_position' },
    });
    expect(setStartFrom).toHaveBeenCalledWith('current_position');
    fireEvent.change(screen.getByRole('combobox', { name: 'Job Origin' }), {
      target: { value: 'center' },
    });
    expect(setJobOrigin).toHaveBeenCalledWith('center');
    fireEvent.click(screen.getByRole('button', { name: 'Set here' }));
    await waitFor(() => expect(setUserOrigin).toHaveBeenCalledWith(12, 34));
    expect(remoteHostService.request).toHaveBeenCalledWith('remote/origin', {});
    await waitFor(() => expect(useRemoteHostStore.getState().busy).toBe(false));
  });

  it('requires both confirmations and sends one explicit start', async () => {
    render(<RemoteHostPanel />);
    const start = screen.getByRole('button', { name: 'Start on host' });
    expect(start).toBeDisabled();
    fireEvent.click(
      screen.getByRole('checkbox', { name: 'I confirm the laser area is clear for motion.' }),
    );
    expect(start).toBeDisabled();
    fireEvent.click(
      screen.getByRole('checkbox', {
        name: 'I confirm the laser can fire and someone is attending the machine.',
      }),
    );
    fireEvent.click(start);
    fireEvent.click(start);
    await waitFor(() => expect(remoteHostService.request).toHaveBeenCalledTimes(1));
    expect(remoteHostService.request).toHaveBeenCalledWith(
      'remote/start',
      expect.objectContaining({
        transfer_id: 'transfer-1',
        confirm_motion: true,
        confirm_laser_on: true,
      }),
    );
    await waitFor(() => expect(useRemoteHostStore.getState().busy).toBe(false));
    expect(start).toBeDisabled();
  });
  it('disables start on stale state while keeping emergency stop available during pending work', async () => {
    useRemoteHostStore.setState({
      online: false,
      busy: true,
      snapshot: { ...snapshot, progress: makeJobProgress({ state: 'running' }) },
    });
    render(<RemoteHostPanel />);
    expect(screen.getByText(/Host state unknown/)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Start on host' })).toBeDisabled();
    fireEvent.click(screen.getByRole('button', { name: 'Stop host laser' }));
    await waitFor(() =>
      expect(remoteHostService.request).toHaveBeenCalledWith('machine/emergency-stop', {}),
    );
  });
});
