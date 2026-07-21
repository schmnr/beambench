import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { useMachineStore } from '../../stores/machineStore';
import { usePreviewStore } from '../../stores/previewStore';
import { JobControlPanel } from './JobControlPanel';

// Mock machineService
vi.mock('../../services/machineService', () => ({
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

vi.mock('../../services/previewService', () => ({
  previewService: {
    generatePreview: vi.fn(),
    generatePlan: vi.fn(),
    getPlanStats: vi.fn(),
    cancelPlanning: vi.fn(),
    exportGcode: vi.fn(),
  },
}));

const idleMachineStatus = {
  run_state: 'idle' as const,
  machine_position: { x: 0, y: 0, z: 0 },
  work_position: { x: 0, y: 0, z: 0 },
  feed_rate: 0,
  spindle_speed: 0,
  feed_override: 100,
  spindle_override: 100,
  rapid_override: 100,
  pin_states: '',
};

describe('JobControlPanel', () => {
  const onShowPreflight = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    useMachineStore.setState({
      sessionState: 'disconnected',
      machineStatus: null,
      jobProgress: null,
      preflightReport: null,
      loading: false,
      error: null,
    });
    usePreviewStore.setState({
      state: 'idle',
    });
  });

  it('renders all four buttons', () => {
    render(<JobControlPanel onShowPreflight={onShowPreflight} />);

    expect(screen.getByText('Start')).toBeDefined();
    expect(screen.getByText('Pause')).toBeDefined();
    expect(screen.getByText('Resume')).toBeDefined();
    expect(screen.getByText('Cancel')).toBeDefined();
  });

  it('disables Start when not ready', () => {
    useMachineStore.setState({ sessionState: 'disconnected' });
    render(<JobControlPanel onShowPreflight={onShowPreflight} />);

    expect(screen.getByText('Start').closest('button')?.disabled).toBe(true);
  });

  it('enables Start when ready and idle', () => {
    useMachineStore.setState({
      sessionState: 'ready',
      machineStatus: idleMachineStatus,
    });

    render(<JobControlPanel onShowPreflight={onShowPreflight} />);

    expect(screen.getByText('Start').closest('button')?.disabled).toBe(false);
  });

  it('auto-generates preview when Start clicked and preview is not current', async () => {
    const generatePreview = vi.fn().mockResolvedValue(true);
    const runPreflight = vi.fn().mockResolvedValue({ outcome: 'pass', checks: [] });
    const startJob = vi.fn().mockResolvedValue(undefined);

    useMachineStore.setState({
      sessionState: 'ready',
      machineStatus: idleMachineStatus,
      runPreflight,
      startJob,
    });
    usePreviewStore.setState({ state: 'idle', generatePreview });

    render(<JobControlPanel onShowPreflight={onShowPreflight} />);

    fireEvent.click(screen.getByText('Start'));

    await waitFor(() => {
      expect(generatePreview).toHaveBeenCalled();
      expect(runPreflight).toHaveBeenCalled();
    });
  });

  it('aborts the start flow when preview bootstrap fails', async () => {
    const generatePreview = vi.fn().mockResolvedValue(false);
    const runPreflight = vi.fn().mockResolvedValue({ outcome: 'pass', checks: [] });
    const startJob = vi.fn().mockResolvedValue(undefined);

    useMachineStore.setState({
      sessionState: 'ready',
      machineStatus: idleMachineStatus,
      runPreflight,
      startJob,
    });
    usePreviewStore.setState({ state: 'idle', generatePreview });

    render(<JobControlPanel onShowPreflight={onShowPreflight} />);

    fireEvent.click(screen.getByText('Start'));

    await waitFor(() => {
      expect(generatePreview).toHaveBeenCalled();
    });
    expect(runPreflight).not.toHaveBeenCalled();
    expect(startJob).not.toHaveBeenCalled();
  });

  it('disables Start while preview bootstrap is in flight', async () => {
    let resolvePreview!: (value: boolean) => void;
    const generatePreview = vi.fn().mockImplementation(
      () => new Promise<boolean>((resolve) => {
        resolvePreview = resolve;
      }),
    );
    const runPreflight = vi.fn().mockResolvedValue({ outcome: 'pass', checks: [] });
    const startJob = vi.fn().mockResolvedValue(undefined);

    useMachineStore.setState({
      sessionState: 'ready',
      machineStatus: idleMachineStatus,
      runPreflight,
      startJob,
    });
    usePreviewStore.setState({ state: 'idle', generatePreview });

    render(<JobControlPanel onShowPreflight={onShowPreflight} />);

    const startButton = screen.getByText('Start').closest('button');
    expect(startButton?.disabled).toBe(false);

    fireEvent.click(screen.getByText('Start'));

    await waitFor(() => {
      expect(startButton?.disabled).toBe(true);
    });

    resolvePreview(true);

    await waitFor(() => {
      expect(runPreflight).toHaveBeenCalled();
    });
  });

  it('enables Pause when job is running', () => {
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
    });

    render(<JobControlPanel onShowPreflight={onShowPreflight} />);

    expect(screen.getByText('Pause').closest('button')?.disabled).toBe(false);
  });

  it('enables Resume when job is paused', () => {
    useMachineStore.setState({
      sessionState: 'paused',
      jobProgress: {
        state: 'paused',
        total_lines: 100,
        queued_lines: 0,
        sent_lines: 50,
        acknowledged_lines: 45,
        elapsed_secs: 30,
        estimated_remaining_secs: 30,
        buffer_fill_bytes: 64,
      },
    });

    render(<JobControlPanel onShowPreflight={onShowPreflight} />);

    expect(screen.getByText('Resume').closest('button')?.disabled).toBe(false);
  });

  it('enables Cancel when job is running or paused', () => {
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
    });

    render(<JobControlPanel onShowPreflight={onShowPreflight} />);

    expect(screen.getByText('Cancel').closest('button')?.disabled).toBe(false);
  });

  it('calls preflight then startJob on Start when preflight passes', async () => {
    const runPreflight = vi.fn().mockResolvedValue({ outcome: 'pass', checks: [] });
    const startJob = vi.fn().mockResolvedValue(undefined);

    useMachineStore.setState({
      sessionState: 'ready',
      machineStatus: idleMachineStatus,
      runPreflight,
      startJob,
    });
    usePreviewStore.setState({ state: 'current' });

    render(<JobControlPanel onShowPreflight={onShowPreflight} />);

    fireEvent.click(screen.getByText('Start'));

    await waitFor(() => {
      expect(runPreflight).toHaveBeenCalled();
      expect(startJob).toHaveBeenCalled();
      expect(onShowPreflight).not.toHaveBeenCalled();
    });
  });

  it('calls onShowPreflight when preflight fails', async () => {
    const runPreflight = vi.fn().mockResolvedValue({ outcome: 'fail', checks: [] });
    const startJob = vi.fn().mockResolvedValue(undefined);

    useMachineStore.setState({
      sessionState: 'ready',
      machineStatus: idleMachineStatus,
      runPreflight,
      startJob,
    });
    usePreviewStore.setState({ state: 'current' });

    render(<JobControlPanel onShowPreflight={onShowPreflight} />);

    fireEvent.click(screen.getByText('Start'));

    await waitFor(() => {
      expect(runPreflight).toHaveBeenCalled();
      expect(startJob).not.toHaveBeenCalled();
      expect(onShowPreflight).toHaveBeenCalled();
    });
  });

  it('does not start the job when a new preflight fails after a stale passing report', async () => {
    const runPreflight = vi.fn().mockResolvedValue(null);
    const startJob = vi.fn().mockResolvedValue(undefined);

    useMachineStore.setState({
      sessionState: 'ready',
      machineStatus: idleMachineStatus,
      preflightReport: { outcome: 'pass', checks: [] },
      runPreflight,
      startJob,
    });
    usePreviewStore.setState({ state: 'current' });

    render(<JobControlPanel onShowPreflight={onShowPreflight} />);

    fireEvent.click(screen.getByText('Start'));

    await waitFor(() => {
      expect(runPreflight).toHaveBeenCalled();
      expect(startJob).not.toHaveBeenCalled();
    });
  });
});
