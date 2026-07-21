import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, render, screen, fireEvent, waitFor } from '@testing-library/react';
import { LaserPanel } from '../LaserPanel';
import { useMachineStore } from '../../../stores/machineStore';
import { usePreviewStore } from '../../../stores/previewStore';
import { useProjectStore } from '../../../stores/projectStore';
import { useUiStore } from '../../../stores/uiStore';
import { useNotificationStore } from '../../../stores/notificationStore';
import { useAppStore } from '../../../stores/appStore';
import { machineService } from '../../../services/machineService';
import { previewService } from '../../../services/previewService';
import { makeJobProgress, makeMachineProfile, makeMachineStatus, makeProject } from '../../../test-utils/projectFixtures';

vi.mock('../../../hooks/useMachinePolling', () => ({
  useMachinePolling: vi.fn(),
}));

vi.mock('../../../services/machineService', () => ({
  machineService: {
    home: vi.fn().mockResolvedValue(undefined),
    unlock: vi.fn().mockResolvedValue(undefined),
    connect: vi.fn().mockResolvedValue(undefined),
    disconnect: vi.fn().mockResolvedValue(undefined),
    sendGcodeLine: vi.fn().mockResolvedValue(undefined),
    frameJob: vi.fn().mockResolvedValue(undefined),
    startJob: vi.fn().mockResolvedValue(undefined),
    pauseJob: vi.fn().mockResolvedValue(undefined),
    resumeJob: vi.fn().mockResolvedValue(undefined),
    cancelJob: vi.fn().mockResolvedValue(undefined),
    emergencyStop: vi.fn().mockResolvedValue(undefined),
    setWorkOrigin: vi.fn().mockResolvedValue([10, 20]),
    resetWorkOrigin: vi.fn().mockResolvedValue(undefined),
    runPreflightCheck: vi.fn().mockResolvedValue({ outcome: 'pass', checks: [] }),
    setActiveProfile: vi.fn().mockResolvedValue(undefined),
  },
}));

vi.mock('../../../services/previewService', () => ({
  previewService: {
    exportGcode: vi.fn().mockResolvedValue('ok'),
    generatePreview: vi.fn().mockResolvedValue(null),
    cancelPlanning: vi.fn(),
    getOptimizationSettings: vi.fn().mockResolvedValue({ cut_order: 'optimized', travel_optimization: true }),
    updateOptimizationSettings: vi.fn().mockResolvedValue(undefined),
  },
}));

vi.mock('../JobProgressBar', () => ({
  JobProgressBar: () => <div data-testid="job-progress-bar">Job Progress</div>,
}));

vi.mock('../OverrideControls', () => ({
  OverrideControls: () => <div data-testid="override-controls">Override Controls</div>,
}));

vi.mock('../PreflightDialog', () => ({
  PreflightDialog: () => <div data-testid="preflight-dialog">Preflight</div>,
}));

vi.mock('../../dialogs/DeviceSettingsDialog', () => ({
  DeviceSettingsDialog: ({ onClose }: { onClose: () => void }) => (
    <div data-testid="devices-dialog">
      <button onClick={onClose}>Close Devices</button>
    </div>
  ),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockReturnValue(new Promise(() => {})) }));

const initialMachineState = useMachineStore.getState();
const initialPreviewState = usePreviewStore.getState();
const initialProjectState = useProjectStore.getState();
const initialUiState = useUiStore.getState();
const initialNotificationState = useNotificationStore.getState();
const initialAppState = useAppStore.getState();

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  useMachineStore.setState(initialMachineState, true);
  usePreviewStore.setState(initialPreviewState, true);
  useProjectStore.setState(initialProjectState, true);
  useUiStore.setState(initialUiState, true);
  useNotificationStore.setState(initialNotificationState, true);
  useAppStore.setState(initialAppState, true);
});

const setConnectedWithProject = () => {
  useMachineStore.setState({
    sessionState: 'ready',
    machineStatus: makeMachineStatus({ run_state: 'idle' }),
    profiles: [],
    jobProgress: null,
  });
  useProjectStore.setState({
    project: makeProject({
      layers: [],
      objects: [],
      start_from: 'absolute_coords',
      job_origin: 'top_left',
    }),
  });
};

describe('LaserPanel', () => {
  it('renders connection gradient bar', () => {
    useProjectStore.setState({
      project: makeProject({ layers: [], objects: [], start_from: 'absolute_coords', job_origin: 'top_left' }),
    });
    render(<LaserPanel />);
    expect(screen.getByTestId('connection-bar')).toBeDefined();
  });

  it('renders profile selection and opens device settings', () => {
    useProjectStore.setState({
      project: makeProject({ layers: [], objects: [], start_from: 'absolute_coords', job_origin: 'top_left' }),
    });
    render(<LaserPanel />);

    const devicesRow = screen.getByTestId('devices-row');
    expect(devicesRow).toBeDefined();
    expect(screen.getByTestId('profile-select')).toBeDefined();

    const devicesBtn = screen.getByTestId('devices-button');
    expect(devicesBtn.textContent).toBe('Manage Machine Profiles...');

    fireEvent.click(devicesBtn);
    expect(screen.getByTestId('devices-dialog')).toBeDefined();
  });

  it('renders large Pause/Stop/Start buttons when connected', () => {
    setConnectedWithProject();
    render(<LaserPanel />);

    expect(screen.getByTestId('job-buttons')).toBeDefined();
    expect(screen.getByTestId('pause-button')).toBeDefined();
    expect(screen.getByTestId('stop-button')).toBeDefined();
    expect(screen.getByTestId('start-button')).toBeDefined();
  });

  it('shows Resume button instead of Start when paused', () => {
    useMachineStore.setState({
      sessionState: 'paused',
      machineStatus: makeMachineStatus({ run_state: 'hold' }),
      profiles: [],
      jobProgress: makeJobProgress({ state: 'paused', total_lines: 100, acknowledged_lines: 50 }),
    });
    useProjectStore.setState({
      project: makeProject({ layers: [], objects: [], start_from: 'absolute_coords', job_origin: 'top_left' }),
    });

    render(<LaserPanel />);
    expect(screen.getByTestId('resume-button')).toBeDefined();
    expect(screen.queryByTestId('start-button')).toBeNull();
  });

  it('routes the single Stop button through the broader software reset path', async () => {
    setConnectedWithProject();
    useMachineStore.setState({
      jobProgress: makeJobProgress({ state: 'running', total_lines: 100, acknowledged_lines: 50 }),
      machineStatus: makeMachineStatus({ run_state: 'run' }),
    });
    render(<LaserPanel />);

    fireEvent.click(screen.getByTestId('stop-button'));

    await waitFor(() => {
      expect(machineService.emergencyStop).toHaveBeenCalled();
    });
    expect(machineService.cancelJob).not.toHaveBeenCalled();
  });

  it('hides job buttons when disconnected', () => {
    useMachineStore.setState({ sessionState: 'disconnected', profiles: [] });
    useProjectStore.setState({
      project: makeProject({ layers: [], objects: [], start_from: 'absolute_coords', job_origin: 'top_left' }),
    });

    render(<LaserPanel />);
    expect(screen.queryByTestId('job-buttons')).toBeNull();
  });

  it('omits machine-positioning controls that belong in the Move panel', () => {
    setConnectedWithProject();
    render(<LaserPanel />);

    expect(screen.queryByTestId('home-button')).toBeNull();
    expect(screen.queryByTestId('goto-origin-button')).toBeNull();
    expect(screen.queryByTestId('clear-user-origin-button')).toBeNull();
    expect(screen.queryByText('Set User Origin')).toBeNull();
  });

  it('keeps idle controls visible while the first status snapshot is pending', () => {
    setConnectedWithProject();
    useMachineStore.setState({ machineStatus: null });

    render(<LaserPanel />);

    expect(screen.getByText('Frame')).toBeDefined();
  });

  it('shows but disables machine-position controls when connected and not idle', () => {
    setConnectedWithProject();
    useMachineStore.setState({
      machineStatus: makeMachineStatus({ run_state: 'run' }),
    });

    render(<LaserPanel />);

    expect((screen.getByText('Frame') as HTMLButtonElement).disabled).toBe(true);
  });

  it('blocks frame controls when GRBL is alarm locked', async () => {
    setConnectedWithProject();
    useMachineStore.setState({
      sessionState: 'alarm',
      machineStatus: makeMachineStatus({ run_state: 'alarm' }),
    });

    render(<LaserPanel />);

    expect(screen.getByText('Alarm')).toBeDefined();
    expect((screen.getByText('Frame') as HTMLButtonElement).disabled).toBe(true);
    expect(screen.queryByTestId('unlock-button')).toBeNull();
  });

  it('confirming a frame records the frame job so polling continues streaming it', async () => {
    setConnectedWithProject();
    const progress = makeJobProgress({ state: 'running', sent_lines: 3, acknowledged_lines: 1 });
    vi.mocked(machineService.frameJob).mockResolvedValueOnce(progress);

    render(<LaserPanel />);

    fireEvent.click(screen.getByText('Frame'));
    expect(screen.getByText('Confirm Frame')).toBeDefined();
    expect(screen.getByText('Frame will move laser head around project bounds with the laser off. Ensure work area is clear.')).toBeDefined();

    fireEvent.click(screen.getByText('Confirm Frame'));

    await waitFor(() => {
      expect(machineService.frameJob).toHaveBeenCalledWith('rectangular', undefined, false, 1000);
    });
    expect(useMachineStore.getState().jobProgress).toEqual(progress);
    expect(screen.getByTestId('job-progress-bar')).toBeDefined();
  });

  it('shift-held frame shows laser-on state before confirmation and passes the one-shot override', async () => {
    setConnectedWithProject();
    vi.mocked(machineService.frameJob).mockResolvedValueOnce(makeJobProgress({ state: 'running' }));
    render(<LaserPanel />);

    const frameButton = screen.getByText('Frame');
    fireEvent.mouseMove(frameButton, { shiftKey: true });
    expect(screen.getByText('Frame: Laser On')).toBeDefined();

    fireEvent.click(screen.getByText('Frame: Laser On'), { shiftKey: true });
    expect(screen.getByText('Confirm Laser Frame')).toBeDefined();

    fireEvent.click(screen.getByText('Confirm Laser Frame'));

    await waitFor(() => {
      expect(machineService.frameJob).toHaveBeenCalledWith('rectangular', undefined, true, 1000);
    });
  });

  it('renders Save GCode button and calls previewService.exportGcode()', () => {
    setConnectedWithProject();
    render(<LaserPanel />);

    const saveBtn = screen.getByTestId('save-gcode-button');
    expect(saveBtn.textContent).toBe('Save GCode');
    fireEvent.click(saveBtn);
    expect(previewService.exportGcode).toHaveBeenCalled();
  });

  it('does not continue into preflight/start when preview bootstrap fails', async () => {
    const generatePreview = vi.fn().mockResolvedValue(false);
    const runPreflight = vi.fn().mockResolvedValue({ outcome: 'pass', checks: [] });
    const startJob = vi.fn().mockResolvedValue(undefined);

    setConnectedWithProject();
    useMachineStore.setState({ runPreflight, startJob });
    usePreviewStore.setState({ state: 'idle', generatePreview });

    render(<LaserPanel />);

    fireEvent.click(screen.getByTestId('start-button'));

    await waitFor(() => {
      expect(generatePreview).toHaveBeenCalled();
    });
    expect(runPreflight).not.toHaveBeenCalled();
    expect(startJob).not.toHaveBeenCalled();
  });

  it('disables Start while preview bootstrap is pending', async () => {
    let resolvePreview!: (value: boolean) => void;
    const generatePreview = vi.fn().mockImplementation(
      () => new Promise<boolean>((resolve) => {
        resolvePreview = resolve;
      }),
    );

    setConnectedWithProject();
    useMachineStore.setState({
      runPreflight: vi.fn().mockResolvedValue({ outcome: 'pass', checks: [] }),
      startJob: vi.fn().mockResolvedValue(undefined),
    });
    usePreviewStore.setState({ state: 'idle', generatePreview });

    render(<LaserPanel />);

    const startButton = screen.getByTestId('start-button');
    expect((startButton as HTMLButtonElement).disabled).toBe(false);

    fireEvent.click(startButton);

    await waitFor(() => {
      expect((startButton as HTMLButtonElement).disabled).toBe(true);
    });

    resolvePreview(true);
  });

  it('does not emit an error toast when Save GCode is cancelled', async () => {
    const push = vi.fn();
    const exportGcode = vi.fn().mockRejectedValueOnce(new Error('Export cancelled'));
    useNotificationStore.setState({ push });
    setConnectedWithProject();
    useProjectStore.setState({ exportGcode });

    render(<LaserPanel />);
    fireEvent.click(screen.getByTestId('save-gcode-button'));

    await waitFor(() => {
      expect(exportGcode).toHaveBeenCalled();
    });
    expect(push).not.toHaveBeenCalled();
  });

  it('hides shelved selection job options and clears stale session state', async () => {
    useMachineStore.setState({
      sessionState: 'ready',
      machineStatus: makeMachineStatus({ run_state: 'idle' }),
      profiles: [],
      jobProgress: null,
    });
    useUiStore.setState({
      jobOptions: { cut_selected_graphics: true, use_selection_origin: false },
    });
    useProjectStore.setState({
      project: makeProject({ layers: [], objects: [], start_from: 'current_position', job_origin: 'top_left' }),
    });

    render(<LaserPanel />);

    expect(screen.queryByTestId('cut-selected-graphics-checkbox')).toBeNull();
    expect(screen.queryByTestId('use-selection-origin-checkbox')).toBeNull();

    await waitFor(() => {
      expect(useUiStore.getState().jobOptions).toEqual({
        cut_selected_graphics: false,
        use_selection_origin: false,
      });
    });
  });

  it('optimization modal edits a local draft and persists only on OK', async () => {
    useMachineStore.setState({
      sessionState: 'ready',
      machineStatus: makeMachineStatus({ run_state: 'idle' }),
      profiles: [],
      jobProgress: null,
    });
    const setOptimization = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({
      project: makeProject({
        layers: [],
        objects: [],
        start_from: 'absolute_coords',
        job_origin: 'top_left',
        optimization: {
          enabled: true,
          ordering: ['layer', 'priority'],
          inner_first: false,
          direction_order: 'none',
          reduce_travel: false,
          hide_backlash: false,
          reduce_direction_changes: false,
          choose_best_start: false,
          choose_corners: false,
          choose_best_direction: false,
          remove_overlapping: false,
          remove_overlap_tolerance_mm: 0.05,
          start_point_x: null,
          start_point_y: null,
          finish_position: 'origin',
          finish_x: null,
          finish_y: null,
        },
      }),
      setOptimization,
    });

    render(<LaserPanel />);

    fireEvent.click(screen.getByTestId('optimization-settings-button'));
    const reduceTravel = screen.getByTestId('reduce-travel') as HTMLInputElement;
    expect(reduceTravel.checked).toBe(false);

    fireEvent.click(reduceTravel);
    expect(setOptimization).not.toHaveBeenCalled();
    fireEvent.click(screen.getByText('OK'));
    await waitFor(() => {
      expect(setOptimization).toHaveBeenCalledWith(
        expect.objectContaining({ reduce_travel: true }),
      );
    });
  });

  it('optimization modal converts custom start and finish points between display units and mm', async () => {
    useMachineStore.setState({
      sessionState: 'ready',
      machineStatus: makeMachineStatus({ run_state: 'idle' }),
      profiles: [],
      jobProgress: null,
    });
    const setOptimization = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({
      project: makeProject({
        layers: [],
        objects: [],
        start_from: 'absolute_coords',
        job_origin: 'top_left',
        optimization: {
          enabled: true,
          ordering: ['layer', 'priority'],
          inner_first: false,
          direction_order: 'none',
          reduce_travel: false,
          hide_backlash: false,
          reduce_direction_changes: false,
          choose_best_start: false,
          choose_corners: false,
          choose_best_direction: false,
          remove_overlapping: false,
          remove_overlap_tolerance_mm: 0.05,
          start_point_x: 25.4,
          start_point_y: 50.8,
          finish_position: 'custom_xy',
          finish_x: 76.2,
          finish_y: 0,
        },
      }),
      setOptimization,
    });
    useAppStore.setState({
      settings: { display_unit: 'inches', speed_time_unit: 'minutes' } as never,
    });

    render(<LaserPanel />);
    fireEvent.click(screen.getByTestId('optimization-settings-button'));

    // Backend mm values render converted to inches.
    expect((screen.getByTestId('start-point-x') as HTMLInputElement).value).toBe('1');
    expect((screen.getByTestId('start-point-y') as HTMLInputElement).value).toBe('2');
    expect((screen.getByTestId('finish-x') as HTMLInputElement).value).toBe('3');

    // Typed inches commit back as millimeters.
    fireEvent.change(screen.getByTestId('start-point-x'), { target: { value: '4' } });
    fireEvent.change(screen.getByTestId('finish-y'), { target: { value: '0.5' } });
    fireEvent.click(screen.getByText('OK'));

    await waitFor(() => {
      expect(setOptimization).toHaveBeenCalled();
    });
    const committed = setOptimization.mock.calls[0][0];
    expect(committed.start_point_x).toBeCloseTo(101.6);
    expect(committed.start_point_y).toBeCloseTo(50.8);
    expect(committed.finish_x).toBeCloseTo(76.2);
    expect(committed.finish_y).toBeCloseTo(12.7);
  });

  it('optimization modal Cancel discards draft changes', () => {
    useMachineStore.setState({
      sessionState: 'ready',
      machineStatus: makeMachineStatus({ run_state: 'idle' }),
      profiles: [],
      jobProgress: null,
    });
    const setOptimization = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({
      project: makeProject({ layers: [], objects: [], start_from: 'absolute_coords', job_origin: 'top_left' }),
      setOptimization,
    });

    render(<LaserPanel />);

    fireEvent.click(screen.getByTestId('optimization-settings-button'));
    fireEvent.click(screen.getByTestId('reduce-travel'));
    fireEvent.click(screen.getByText('Cancel'));

    expect(setOptimization).not.toHaveBeenCalled();
    expect(screen.queryByTestId('optimization-modal')).toBeNull();
  });

  it('Show Last Position button toggles uiStore state', () => {
    useProjectStore.setState({
      project: makeProject({ layers: [], objects: [], start_from: 'absolute_coords', job_origin: 'top_left' }),
    });

    render(<LaserPanel />);

    const btn = screen.getByTestId('show-last-position-button');
    expect(btn).toBeDefined();
    expect(useUiStore.getState().showLastPosition).toBe(false);

    fireEvent.click(btn);
    expect(useUiStore.getState().showLastPosition).toBe(true);

    fireEvent.click(btn);
    expect(useUiStore.getState().showLastPosition).toBe(false);
  });

  it('disables Job Origin in Absolute Coords without mutating the saved anchor', () => {
    useMachineStore.setState({
      sessionState: 'ready',
      machineStatus: makeMachineStatus({ run_state: 'idle' }),
      profiles: [],
      jobProgress: null,
    });
    useProjectStore.setState({
      project: makeProject({
        layers: [],
        objects: [],
        start_from: 'absolute_coords',
        job_origin: 'center',
      }),
    });

    render(<LaserPanel />);

    const centerAnchor = screen.getByText('C') as HTMLButtonElement;
    const bottomRightAnchor = screen.getByText('BR') as HTMLButtonElement;
    expect(centerAnchor.disabled).toBe(true);
    expect(bottomRightAnchor.disabled).toBe(true);

    fireEvent.click(bottomRightAnchor);
    expect(useProjectStore.getState().project?.job_origin).toBe('center');
  });

  it('renders the start-from status variants when start_from changes', () => {
    useMachineStore.setState({
      sessionState: 'ready',
      machineStatus: makeMachineStatus({ run_state: 'idle' }),
      profiles: [],
      jobProgress: null,
    });
    const project = makeProject({
      layers: [],
      objects: [],
      start_from: 'absolute_coords',
      job_origin: 'top_left',
    });
    useProjectStore.setState({ project });

    const { rerender } = render(<LaserPanel />);
    expect(screen.getByText('Job Origin is ignored. Artwork runs at its workspace coordinates.')).toBeDefined();

    act(() => {
      useProjectStore.setState({ project: { ...project, start_from: 'current_position' } });
    });
    rerender(<LaserPanel />);
    expect(screen.getByText('Job Origin anchors output relative to the current head position.')).toBeDefined();

    act(() => {
      useProjectStore.setState({ project: { ...project, start_from: 'user_origin' } });
    });
    rerender(<LaserPanel />);
    expect(screen.getByText('Job Origin anchors output relative to the stored user origin.')).toBeDefined();
  });

  it('shows the active profile in the profile selector', () => {
    useMachineStore.setState({
      sessionState: 'ready',
      profiles: [makeMachineProfile({ id: 'p1', name: 'Thunder Laser' })],
      activeProfileId: 'p1',
    });
    useProjectStore.setState({
      project: makeProject({ layers: [], objects: [], start_from: 'absolute_coords', job_origin: 'top_left' }),
    });

    render(<LaserPanel />);
    expect(screen.getByText('Thunder Laser')).toBeDefined();
  });

  it('lets disconnected users choose a machine profile from Laser Control', () => {
    const setActiveProfile = vi.fn().mockResolvedValue(undefined);
    useMachineStore.setState({
      sessionState: 'disconnected',
      profiles: [makeMachineProfile({ id: 'p1', name: 'Thunder Laser' })],
      activeProfileId: null,
      setActiveProfile,
    });
    useProjectStore.setState({
      project: makeProject({ layers: [], objects: [], start_from: 'absolute_coords', job_origin: 'top_left' }),
    });

    render(<LaserPanel />);
    fireEvent.change(screen.getByTestId('profile-select'), { target: { value: 'p1' } });

    expect(setActiveProfile).toHaveBeenCalledWith('p1');
  });
});
