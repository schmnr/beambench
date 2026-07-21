import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MovePanel } from '../MovePanel';
import { useMachineStore } from '../../../stores/machineStore';
import { useProjectStore } from '../../../stores/projectStore';
import { useUiStore } from '../../../stores/uiStore';
import { useNotificationStore } from '../../../stores/notificationStore';
import { useAppStore } from '../../../stores/appStore';
import { machineService } from '../../../services/machineService';
import { makeMachineProfile, makeMachineStatus, makeProject } from '../../../test-utils/projectFixtures';

vi.mock('../../../services/machineService', () => ({
  machineService: {
    getMachineProfiles: vi.fn().mockResolvedValue([]),
    getMachineStatus: vi.fn().mockResolvedValue({
      run_state: 'idle',
      machine_position: { x: 0, y: 0, z: 0 },
      work_position: { x: 0, y: 0, z: 0 },
      feed_rate: 0,
      spindle_speed: 0,
      feed_override: 100,
      spindle_override: 100,
      rapid_override: 100,
      pin_states: '',
    }),
    getMachineCoordinatesValid: vi.fn().mockResolvedValue(false),
    getSessionState: vi.fn().mockResolvedValue('ready'),
    home: vi.fn().mockResolvedValue(undefined),
    unlock: vi.fn().mockResolvedValue(undefined),
    getSavedPositions: vi.fn().mockResolvedValue([]),
    savePosition: vi.fn().mockResolvedValue([]),
    deleteSavedPosition: vi.fn().mockResolvedValue([]),
    moveLaserTo: vi.fn().mockResolvedValue(undefined),
    moveLaserToMachine: vi.fn().mockResolvedValue(undefined),
    jog: vi.fn().mockResolvedValue(undefined),
    jogCancel: vi.fn().mockResolvedValue(undefined),
    setWorkOrigin: vi.fn().mockResolvedValue([10, 20]),
    resetWorkOrigin: vi.fn().mockResolvedValue(undefined),
    laserFireStart: vi.fn().mockResolvedValue({
      token: 'fire-token',
      power_percent: 1,
      s_value: 10,
      keepalive_interval_ms: 250,
      max_duration_ms: 10000,
    }),
    laserFireKeepalive: vi.fn().mockResolvedValue(undefined),
    laserFireStop: vi.fn().mockResolvedValue(undefined),
    setFeedOverride: vi.fn().mockResolvedValue(undefined),
    setSpindleOverride: vi.fn().mockResolvedValue(undefined),
    resetAllOverrides: vi.fn().mockResolvedValue(undefined),
  },
}));

vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockReturnValue(new Promise(() => {})) }));

const initialMachineState = useMachineStore.getState();
const initialProjectState = useProjectStore.getState();
const initialUiState = useUiStore.getState();
const initialNotificationState = useNotificationStore.getState();
const initialAppState = useAppStore.getState();

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  useMachineStore.setState(initialMachineState, true);
  useProjectStore.setState(initialProjectState, true);
  useUiStore.setState(initialUiState, true);
  useNotificationStore.setState(initialNotificationState, true);
  useAppStore.setState(initialAppState, true);
});

function connectMachine() {
  useMachineStore.setState({
    sessionState: 'ready',
    machineStatus: makeMachineStatus({
      run_state: 'idle',
      work_position: { x: 12, y: 34, z: 0 },
    }),
    machineCoordinatesValid: false,
    // Mirrors what loadRuntimeCapabilities stores after a GRBL connect.
    capabilities: {
      can_home: true,
      can_jog: true,
      can_jog_continuous: true,
      can_unlock: true,
      can_pause_resume: true,
      can_set_origin: true,
      can_frame: true,
      can_run_job: true,
      reports_absolute_position: true,
      can_manual_fire: true,
      can_adjust_overrides: true,
      supports_rotary: false,
      supports_cylinder: false,
      supports_camera_alignment: false,
    },
    profiles: [makeMachineProfile()],
    activeProfileId: 'profile-1',
  });
}

describe('MovePanel', () => {
  it('renders without an open project and does not show object movement controls', async () => {
    render(<MovePanel />);

    await waitFor(() => {
      expect(machineService.getSavedPositions).toHaveBeenCalled();
    });
    expect(screen.getByText('Machine Positioning')).toBeDefined();
    expect(screen.getByText('Machine disconnected')).toBeDefined();
    expect(screen.queryByRole('button', { name: 'TL' })).toBeNull();
  });

  it('can preview connected UI without a backend connection', async () => {
    render(<MovePanel />);

    fireEvent.click(screen.getByRole('checkbox', { name: /Preview connected UI/ }));

    await waitFor(() => {
      expect(useMachineStore.getState().connectionPreview).toBe(true);
      expect(screen.getByText('Machine connected')).toBeDefined();
    });
    expect((screen.getByTestId('goto-button') as HTMLButtonElement).disabled).toBe(false);
  });

  it('sends Go through the machine move command using the shared feed rate', async () => {
    connectMachine();
    useUiStore.setState({ moveWindowJogFeedRateMmMin: 1500 });

    render(<MovePanel />);
    fireEvent.click(screen.getByTestId('goto-button'));

    await waitFor(() => {
      expect(machineService.moveLaserTo).toHaveBeenCalledWith(12, 34, 1500, null);
    });
  });

  it('homes from the machine-positioning header when ready', async () => {
    connectMachine();

    render(<MovePanel />);
    fireEvent.click(screen.getByRole('button', { name: 'Home' }));

    await waitFor(() => {
      expect(machineService.home).toHaveBeenCalled();
    });
  });

  it('requires current-session homing before moving to machine zero', async () => {
    connectMachine();

    render(<MovePanel />);

    await waitFor(() => {
      expect(machineService.getSavedPositions).toHaveBeenCalled();
    });
    expect(screen.getByText('Home the machine first to use machine zero.')).toBeDefined();
    expect((screen.getByTestId('goto-machine-zero-button') as HTMLButtonElement).disabled).toBe(true);
    expect(machineService.moveLaserToMachine).not.toHaveBeenCalled();
  });

  it('moves to machine zero after current-session homing is valid', async () => {
    connectMachine();
    useMachineStore.setState({ machineCoordinatesValid: true });
    useUiStore.setState({ moveWindowJogFeedRateMmMin: 1500 });

    render(<MovePanel />);
    fireEvent.click(screen.getByTestId('goto-machine-zero-button'));

    await waitFor(() => {
      expect(machineService.moveLaserToMachine).toHaveBeenCalledWith(0, 0, 1500, null);
    });
  });

  it('unlocks from the machine-positioning header when alarm locked', async () => {
    connectMachine();
    useMachineStore.setState({
      sessionState: 'alarm',
      machineStatus: makeMachineStatus({ run_state: 'alarm' }),
    });

    render(<MovePanel />);
    fireEvent.click(screen.getByRole('button', { name: 'Unlock' }));

    await waitFor(() => {
      expect(machineService.unlock).toHaveBeenCalled();
    });
  });

  it('converts Go coordinates from display units to millimeters', async () => {
    connectMachine();
    useMachineStore.setState({
      machineStatus: makeMachineStatus({
        run_state: 'idle',
        work_position: { x: 25.4, y: 50.8, z: 0 },
      }),
    });
    useAppStore.setState({
      settings: { display_unit: 'inches', speed_time_unit: 'minutes' } as never,
    });
    useUiStore.setState({ moveWindowJogFeedRateMmMin: 1500 });

    render(<MovePanel />);
    fireEvent.change(screen.getByLabelText('X (in)'), { target: { value: '3' } });
    fireEvent.click(screen.getByTestId('goto-button'));

    await waitFor(() => {
      expect(machineService.moveLaserTo).toHaveBeenCalled();
    });
    const calls = vi.mocked(machineService.moveLaserTo).mock.calls;
    const [x, y, feedRate, z] = calls[calls.length - 1] ?? [];
    expect(x).toBeCloseTo(76.2);
    expect(y).toBeCloseTo(50.8);
    expect(feedRate).toBe(1500);
    expect(z).toBeNull();
  });

  it('shows the Go toast coordinates in display units', async () => {
    connectMachine();
    useMachineStore.setState({
      machineStatus: makeMachineStatus({
        run_state: 'idle',
        work_position: { x: 25.4, y: 50.8, z: 0 },
      }),
    });
    useAppStore.setState({
      settings: { display_unit: 'inches', speed_time_unit: 'minutes' } as never,
    });

    render(<MovePanel />);
    fireEvent.click(screen.getByTestId('goto-button'));

    await waitFor(() => {
      expect(machineService.moveLaserTo).toHaveBeenCalled();
    });
    await waitFor(() => {
      const messages = useNotificationStore.getState().notifications.map((n) => n.message);
      // 25.4 mm -> 1 in, 50.8 mm -> 2 in; raw mm would render as (25.4, 50.8)
      expect(messages.some((m) => m.includes('(1, 2)'))).toBe(true);
    });
  });

  it('shows a Stop Jog button while jogging that cancels the jog', async () => {
    connectMachine();
    useMachineStore.setState({
      machineStatus: makeMachineStatus({
        run_state: 'jog',
        work_position: { x: 12, y: 34, z: 0 },
      }),
    });

    render(<MovePanel />);
    fireEvent.click(screen.getByTestId('stop-jog-button'));

    await waitFor(() => {
      expect(machineService.jogCancel).toHaveBeenCalled();
    });
  });

  it('hides the Stop Jog button when the machine is idle', async () => {
    connectMachine();

    render(<MovePanel />);
    await waitFor(() => {
      expect(machineService.getSavedPositions).toHaveBeenCalled();
    });
    expect(screen.queryByTestId('stop-jog-button')).toBeNull();
  });

  it('jogs the configured Ruida lift-table channel in finite profile-speed steps', async () => {
    connectMachine();
    useUiStore.setState({ moveWindowJogDistanceMm: 2.5 });
    useMachineStore.setState({
      profiles: [makeMachineProfile({
        firmware_type: 'ruida',
        ruida_table_axis: 'u',
        z_move_feed_mm_min: 240,
      })],
      loadProfiles: vi.fn().mockResolvedValue(undefined),
    });

    render(<MovePanel />);
    expect(screen.getByText('Lift table (U axis)')).toBeDefined();

    fireEvent.click(screen.getByTestId('ruida-table-jog-positive'));
    await waitFor(() => {
      expect(machineService.jog).toHaveBeenCalledWith(0, 0, 240, 2.5);
    });

    fireEvent.click(screen.getByTestId('ruida-table-jog-negative'));
    await waitFor(() => {
      expect(machineService.jog).toHaveBeenCalledWith(0, 0, 240, -2.5);
    });
  });

  it('keeps Ruida lift-table controls hidden until a Z or U channel is selected', async () => {
    connectMachine();
    useMachineStore.setState({
      profiles: [makeMachineProfile({ firmware_type: 'ruida', ruida_table_axis: 'disabled' })],
      loadProfiles: vi.fn().mockResolvedValue(undefined),
    });

    render(<MovePanel />);
    await waitFor(() => {
      expect(machineService.getSavedPositions).toHaveBeenCalled();
    });
    expect(screen.queryByTestId('ruida-table-jog-positive')).toBeNull();
  });

  it('keeps Move Laser to Selection as machine motion', async () => {
    connectMachine();
    useProjectStore.setState({
      project: makeProject({
        workspace: { bed_width_mm: 400, bed_height_mm: 400, origin: 'bottom_left' },
      }),
      selectedObjectIds: ['obj-1'],
    });

    render(<MovePanel />);
    fireEvent.click(screen.getByRole('button', { name: 'Laser to Selection' }));

    await waitFor(() => {
      expect(machineService.moveLaserTo).toHaveBeenCalled();
    });
  });

  it('shows Fire only when the active profile opts in and stops on release', async () => {
    connectMachine();
    useMachineStore.setState({
      profiles: [makeMachineProfile({ enable_laser_fire_button: true, default_fire_power_percent: 1 })],
      activeProfileId: 'profile-1',
    });

    render(<MovePanel />);
    const fireButton = screen.getByRole('button', { name: 'Hold to Fire' });
    await act(async () => {
      fireEvent.pointerDown(fireButton, { pointerId: 1 });
    });

    await waitFor(() => {
      expect(machineService.laserFireStart).toHaveBeenCalledWith(1);
    });
    await Promise.resolve();

    await act(async () => {
      window.dispatchEvent(new Event('blur'));
    });

    await waitFor(() => {
      expect(machineService.laserFireStop).toHaveBeenCalledWith('fire-token');
    });
  });

  it('offers Lihuiyu only capability-backed controls instead of erroring buttons', () => {
    connectMachine();
    useMachineStore.setState({
      controllerSelection: { mode: 'known_driver', driver: 'lihuiyu' },
      capabilities: {
        can_home: true,
        can_jog: true,
        can_jog_continuous: false,
        can_unlock: true,
        can_pause_resume: true,
        can_set_origin: false,
        can_frame: true,
        can_run_job: true,
        reports_absolute_position: false,
        can_manual_fire: false,
        can_adjust_overrides: false,
        supports_rotary: false,
        supports_cylinder: false,
        supports_camera_alignment: false,
      },
      profiles: [makeMachineProfile({ enable_laser_fire_button: true, default_fire_power_percent: 1 })],
      activeProfileId: 'profile-1',
    });

    render(<MovePanel />);

    // The backend rejects manual fire and continuous jog on Lihuiyu; the
    // controls must not render as always-erroring buttons.
    expect(screen.queryByRole('button', { name: 'Hold to Fire' })).toBeNull();
    expect(screen.queryByText('Click for one step. Hold to jog continuously.')).toBeNull();
    expect((screen.getByTestId('goto-machine-zero-button') as HTMLButtonElement).disabled).toBe(true);
  });

  it('disables Home and jog when the controller lacks those capabilities', () => {
    connectMachine();
    useMachineStore.setState({
      // Mirrors experimental_acknowledged_gcode (Marlin/Smoothieware).
      capabilities: {
        can_home: false,
        can_jog: false,
        can_jog_continuous: false,
        can_unlock: false,
        can_pause_resume: false,
        can_set_origin: false,
        can_frame: true,
        can_run_job: true,
        reports_absolute_position: false,
        can_manual_fire: false,
        can_adjust_overrides: false,
        supports_rotary: false,
        supports_cylinder: false,
        supports_camera_alignment: false,
      },
    });

    render(<MovePanel />);

    // The backend rejects home/jog for these controllers; the buttons must
    // not present as live controls that always error.
    expect((screen.getByRole('button', { name: 'Home' }) as HTMLButtonElement).disabled).toBe(true);
    const jogUp = screen.getByTitle('Jog Up');
    expect((jogUp as HTMLButtonElement).disabled).toBe(true);
  });

  it('offers Ruida only the manual controls implemented by its native adapter', async () => {
    connectMachine();
    useMachineStore.setState({
      controllerSelection: { mode: 'known_driver', driver: 'ruida' },
      // Mirrors the Ruida adapter's DeviceCapabilities.
      capabilities: {
        can_home: true,
        can_jog: true,
        can_jog_continuous: false,
        can_unlock: false,
        can_pause_resume: true,
        can_set_origin: false,
        can_frame: true,
        can_run_job: true,
        reports_absolute_position: false,
        can_manual_fire: false,
        can_adjust_overrides: false,
        supports_rotary: false,
        supports_cylinder: false,
        supports_camera_alignment: false,
      },
      profiles: [makeMachineProfile({ enable_laser_fire_button: true })],
      activeProfileId: 'profile-1',
    });
    useProjectStore.setState({
      project: makeProject({ user_origin: [10, 20] }),
      selectedObjectIds: ['obj-1'],
    });

    render(<MovePanel />);

    await waitFor(() => {
      expect(machineService.getSavedPositions).toHaveBeenCalled();
    });

    expect((screen.getByRole('button', { name: 'Home' }) as HTMLButtonElement).disabled).toBe(false);
    expect((screen.getByTestId('goto-button') as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByTestId('goto-machine-zero-button') as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByRole('button', { name: 'Set Origin' }) as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByRole('button', { name: 'Go Origin' }) as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByRole('button', { name: 'Laser to Selection' }) as HTMLButtonElement).disabled).toBe(true);
    expect(screen.queryByRole('button', { name: 'Hold to Fire' })).toBeNull();
    expect(screen.queryByText('Hold a direction to jog continuously.')).toBeNull();
  });
});
