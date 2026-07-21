import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  consumeSuppressedProfileEvent,
  resetSuppressedProfileEvents,
  suppressProfileEvent,
  useMachineStore,
} from '../machineStore';
import { useCameraStore } from '../cameraStore';
import { useNotificationStore } from '../notificationStore';
import { useProjectStore } from '../projectStore';

vi.mock('../../services/machineService', () => ({
  machineService: {
    listSerialPorts: vi.fn(),
    connect: vi.fn(),
    beginControllerConnection: vi.fn(),
    beginNetworkControllerConnection: vi.fn(),
    listLihuiyuUsbDevices: vi.fn(),
    beginUsbControllerConnection: vi.fn(),
    continueControllerConnection: vi.fn(),
    disconnect: vi.fn(),
    getMachineStatus: vi.fn(),
    getMachineCoordinatesValid: vi.fn(),
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
    emergencyStop: vi.fn(),
    setWorkOrigin: vi.fn(),
    resetWorkOrigin: vi.fn(),
    frameJob: vi.fn(),
  },
}));

vi.mock('../../services/discoveryService', () => ({
  discoveryService: {
    getDiscoveryState: vi.fn(),
    startDiscovery: vi.fn(),
    cancelDiscovery: vi.fn(),
    connectCandidate: vi.fn(),
    bootstrapProfile: vi.fn(),
  },
}));

vi.mock('../../services/appService', () => ({
  appService: {
    getSettings: vi.fn(),
  },
}));

vi.mock('../../services/previewService', () => ({
  previewService: {
    updateOptimizationSettings: vi.fn(),
    getOptimizationSettings: vi.fn(),
    generatePreview: vi.fn(),
    cancelPlanning: vi.fn(),
    exportGcode: vi.fn(),
  },
}));

import { appService } from '../../services/appService';
import { discoveryService } from '../../services/discoveryService';
import { machineService } from '../../services/machineService';
import { usePreviewStore } from '../previewStore';
import type { AppSettings } from '../../types/commands';
import type { MachineProfile } from '../../types/machine';
import { makeJobProgress, makeMachineStatus, makeProject } from '../../test-utils/projectFixtures';

const mockedApp = appService as unknown as {
  getSettings: ReturnType<typeof vi.fn>;
};
const mockedMachine = machineService as unknown as {
  beginControllerConnection: ReturnType<typeof vi.fn>;
  beginNetworkControllerConnection: ReturnType<typeof vi.fn>;
  listLihuiyuUsbDevices: ReturnType<typeof vi.fn>;
  beginUsbControllerConnection: ReturnType<typeof vi.fn>;
  continueControllerConnection: ReturnType<typeof vi.fn>;
  disconnect: ReturnType<typeof vi.fn>;
  cancelJob: ReturnType<typeof vi.fn>;
  frameJob: ReturnType<typeof vi.fn>;
  getMachineStatus: ReturnType<typeof vi.fn>;
  getMachineCoordinatesValid: ReturnType<typeof vi.fn>;
  getSessionState: ReturnType<typeof vi.fn>;
  startJob: ReturnType<typeof vi.fn>;
  runPreflightCheck: ReturnType<typeof vi.fn>;
  getMachineProfiles: ReturnType<typeof vi.fn>;
  saveMachineProfile: ReturnType<typeof vi.fn>;
  deleteMachineProfile: ReturnType<typeof vi.fn>;
  setActiveProfile: ReturnType<typeof vi.fn>;
};
const mockedDiscovery = discoveryService as unknown as {
  connectCandidate: ReturnType<typeof vi.fn>;
  bootstrapProfile: ReturnType<typeof vi.fn>;
};
const initialCameraState = useCameraStore.getState();

function makeProfile(overrides: Partial<MachineProfile> = {}): MachineProfile {
  return {
    id: 'profile-1',
    name: 'Test Laser',
    bed_width_mm: 400,
    bed_height_mm: 300,
    max_speed_mm_min: 3000,
    max_power_percent: 100,
    s_value_max: 1000,
    homing_enabled: true,
    default_baud_rate: 115200,
    firmware_type: 'GRBL',
    notes: '',
    origin: 'top_left',
    laser_offset_x: 0,
    laser_offset_y: 0,
    enable_laser_offset: false,
    swap_xy: false,
    selected_camera_id: null,
    camera_calibration: null,
    camera_alignment: null,
    job_checklist: false,
    frame_continuously: false,
    laser_on_when_framing: false,
    tab_pulse_width_ms: 0,
    cnc_machine: false,
    use_constant_power: false,
    emit_s_every_g1: false,
    use_g0_for_overscan: false,
    scanning_offsets: [],
    enable_scanning_offset: false,
    dot_width_mm: 0,
    enable_dot_width: false,
    ...overrides,
  };
}

function makeSettings(overrides: Partial<AppSettings> = {}): AppSettings {
  return {
    display_unit: 'mm',
    autosave_enabled: true,
    autosave_interval_secs: 300,
    machine_profiles: [],
    active_profile_id: null,
    recent_files: [],
    api_enabled: false,
    api_port: 8080,
    api_localhost_only: false,
    ui_theme: 'dark',
    dark_mode: false,
    antialiasing: false,
    filled_rendering: false,
    reduce_motion: false,
    show_palette_labels: false,
    cursor_size: 'normal',
    toolbar_icon_size: 'normal',
    click_tolerance_px: 5,
    snap_threshold_px: 5,
    grid_spacing_mm: 10,
    nudge_step_mm: 5,
    nudge_step_fine_mm: 1,
    nudge_step_coarse_mm: 20,
    scroll_zoom: true,
    debug_log_enabled: false,
    panel_layout: null,
    saved_positions: [],
    last_radius_mm: 5,
    image_presets: [],
    custom_hotkeys: {},
    export_settings: { last_directory: null, last_format: 'svg', filename_stem: null },
    ...overrides,
  };
}

describe('machineStore frame-selected toggle', () => {
  // The former "optimization settings" suite in this block was removed
  // with the machine-scoped optimization state and actions.
  // Coverage now lives in `projectStore.test.ts::setOptimization*`
  // (store action) and `commands::project::tests::set_optimization_*`
  // (Rust IPC). Only the still-relevant frame-selected-only test stays.
  beforeEach(() => {
    vi.clearAllMocks();
    mockedMachine.getMachineCoordinatesValid.mockResolvedValue(false);
    resetSuppressedProfileEvents();
    usePreviewStore.setState({
      invalidate: vi.fn(),
    });
    useMachineStore.setState({
      frameSelectedOnly: false,
      loading: false,
      error: null,
      controllerSelection: { mode: 'known_driver', driver: 'grbl' },
      controllerConnectionChallenge: null,
    });
    useProjectStore.setState({
      ...useProjectStore.getState(),
      advanceAutoVariableText: vi.fn().mockResolvedValue(true),
    });
    useCameraStore.setState(initialCameraState, true);
    useNotificationStore.setState({ notifications: [] });
  });

  it('frameSelectedOnly renamed from cutSelectedOnly works correctly', () => {
    useMachineStore.getState().setFrameSelectedOnly(true);
    expect(useMachineStore.getState().frameSelectedOnly).toBe(true);

    useMachineStore.getState().setFrameSelectedOnly(false);
    expect(useMachineStore.getState().frameSelectedOnly).toBe(false);
  });

  it('runPreflight clears stale reports when the latest preflight fails', async () => {
    mockedMachine.runPreflightCheck.mockRejectedValue(new Error('preflight failed'));
    useMachineStore.setState({
      preflightReport: { outcome: 'pass', checks: [] },
      loading: false,
      error: null,
    });

    await expect(useMachineStore.getState().runPreflight()).resolves.toBeNull();

    const state = useMachineStore.getState();
    expect(state.preflightReport).toBeNull();
    expect(state.error).toBe('Error: preflight failed');
  });

  it('frameJob stores returned progress so polling can stream the frame', async () => {
    const progress = makeJobProgress({ state: 'running', sent_lines: 4, acknowledged_lines: 1 });
    mockedMachine.frameJob.mockResolvedValue(progress);
    useMachineStore.setState({ jobProgress: null, loading: false, error: null });

    const result = await useMachineStore.getState().frameJob('rectangular', ['obj-1'], true);

    expect(mockedMachine.frameJob).toHaveBeenCalledWith('rectangular', ['obj-1'], true, 1000);
    expect(result).toBe(progress);
    expect(useMachineStore.getState()).toMatchObject({
      jobProgress: progress,
      loading: false,
      error: null,
    });
  });

  it('connect hydrates the first machine status snapshot for connected controls', async () => {
    const status = makeMachineStatus({ run_state: 'idle', work_position: { x: 12, y: 34, z: 0 } });
    mockedMachine.beginControllerConnection.mockResolvedValue({
      status: 'connected',
      session_state: 'ready',
      endpoint: {
        type: 'serial',
        port_name: '/dev/cu.usbserial-210',
        baud_rate: 115200,
      },
      choice: {
        selection: { mode: 'known_driver', driver: 'grbl' },
        driver: 'grbl',
        source: 'known_driver_selection',
        detected_identity: null,
        requires_experimental_mode: false,
        mismatch: false,
        override_scope: null,
        requires_experimental_compatibility_handshake: false,
      },
    });
    mockedMachine.getMachineStatus.mockResolvedValue(status);
    useMachineStore.setState({
      machineStatus: null,
      jobProgress: makeJobProgress({ state: 'completed' }),
      loading: false,
      error: null,
    });

    await useMachineStore.getState().connect('/dev/cu.usbserial-210', 115200);

    expect(mockedMachine.beginControllerConnection).toHaveBeenCalledWith(
      '/dev/cu.usbserial-210',
      115200,
      { mode: 'known_driver', driver: 'grbl' },
    );
    expect(mockedMachine.getMachineStatus).toHaveBeenCalled();
    expect(useMachineStore.getState()).toMatchObject({
      sessionState: 'ready',
      connectedPort: '/dev/cu.usbserial-210',
      machineStatus: status,
      machineCoordinatesValid: false,
      jobProgress: null,
      loading: false,
    });
  });

  it('connectNetwork preserves a TCP endpoint without inventing serial fields', async () => {
    const status = makeMachineStatus({ run_state: 'idle' });
    mockedMachine.beginNetworkControllerConnection.mockResolvedValue({
      status: 'connected',
      session_state: 'ready',
      endpoint: { type: 'tcp', host: 'fluidnc.local', port: 23 },
      choice: {
        selection: { mode: 'known_driver', driver: 'fluid_nc' },
        driver: 'fluid_nc',
        source: 'known_driver_selection',
        detected_identity: null,
        requires_experimental_mode: true,
        mismatch: false,
        override_scope: null,
        requires_experimental_compatibility_handshake: true,
      },
    });
    mockedMachine.getMachineStatus.mockResolvedValue(status);

    await useMachineStore.getState().connectNetwork(
      'fluidnc.local',
      23,
      { mode: 'known_driver', driver: 'fluid_nc' },
    );

    expect(mockedMachine.beginNetworkControllerConnection).toHaveBeenCalledWith(
      'fluidnc.local',
      23,
      { mode: 'known_driver', driver: 'fluid_nc' },
    );
    expect(useMachineStore.getState()).toMatchObject({
      sessionState: 'ready',
      connectedPort: 'fluidnc.local:23',
      machineStatus: status,
      controllerConnectionChallenge: null,
      loading: false,
    });
  });

  it('connectUsb uses the explicit Lihuiyu driver and preserves the USB endpoint', async () => {
    const device = {
      bus_id: '20',
      device_address: 7,
      port_numbers: [1, 3],
      vendor_id: 0x1a86,
      product_id: 0x5512,
      manufacturer: 'Lihuiyu Studio Labs',
      product: 'M2 Nano',
      serial_number: null,
      has_required_bulk_endpoints: true,
      driver: null,
    };
    const status = makeMachineStatus({ run_state: 'idle' });
    mockedMachine.beginUsbControllerConnection.mockResolvedValue({
      status: 'connected',
      session_state: 'ready',
      endpoint: {
        type: 'usb',
        device_id: 'usb-bus-20-ports-1.3',
        vendor_id: 0x1a86,
        product_id: 0x5512,
      },
      choice: {
        selection: { mode: 'known_driver', driver: 'lihuiyu' },
        driver: 'lihuiyu',
        source: 'known_driver_selection',
        detected_identity: {
          family: 'dsp',
          model: 'lihuiyu_m2_nano',
          firmware_identity: 'Lihuiyu M2-compatible controller',
          firmware_version: null,
          evidence: ['Recognized controller status'],
        },
        requires_experimental_mode: true,
        mismatch: false,
        override_scope: null,
        requires_experimental_compatibility_handshake: false,
      },
    });
    mockedMachine.getMachineStatus.mockResolvedValue(status);

    await useMachineStore.getState().connectUsb(device);

    expect(mockedMachine.beginUsbControllerConnection).toHaveBeenCalledWith(device, {
      mode: 'known_driver',
      driver: 'lihuiyu',
    });
    expect(useMachineStore.getState()).toMatchObject({
      sessionState: 'ready',
      connectedPort: 'usb-bus-20-ports-1.3 (1a86:5512)',
      machineStatus: status,
      controllerSelection: { mode: 'known_driver', driver: 'lihuiyu' },
      controllerConnectionChallenge: null,
      loading: false,
    });
  });

  it('keeps a backend controller-choice challenge without reporting a connection', async () => {
    mockedMachine.beginControllerConnection.mockResolvedValue({
      status: 'challenge',
      attempt_id: 'attempt-1',
      endpoint: {
        type: 'serial',
        port_name: '/dev/cu.usbserial-210',
        baud_rate: 115200,
      },
      detected_identity: null,
      resolution: {
        outcome: 'selection_required',
        override_update: { action: 'keep' },
      },
    });

    await useMachineStore.getState().connect(
      '/dev/cu.usbserial-210',
      115200,
      { mode: 'auto_detect' },
    );

    expect(useMachineStore.getState()).toMatchObject({
      sessionState: 'disconnected',
      connectedPort: null,
      loading: false,
      controllerSelection: { mode: 'auto_detect' },
      controllerConnectionChallenge: {
        status: 'challenge',
        attempt_id: 'attempt-1',
      },
    });
  });

  it('continues only the backend-issued challenge token and hydrates a resolved session', async () => {
    const status = makeMachineStatus({ run_state: 'idle' });
    useMachineStore.setState({
      controllerSelection: { mode: 'known_driver', driver: 'grbl' },
      controllerConnectionChallenge: {
        status: 'challenge',
        attempt_id: 'attempt-2',
        endpoint: {
          type: 'serial',
          port_name: '/dev/cu.usbserial-210',
          baud_rate: 115200,
        },
        detected_identity: null,
        resolution: {
          outcome: 'selection_required',
          override_update: { action: 'keep' },
        },
      },
    });
    mockedMachine.continueControllerConnection.mockResolvedValue({
      status: 'connected',
      session_state: 'ready',
      endpoint: {
        type: 'serial',
        port_name: '/dev/cu.usbserial-210',
        baud_rate: 115200,
      },
      choice: {
        selection: { mode: 'known_driver', driver: 'grbl' },
        driver: 'grbl',
        source: 'known_driver_selection',
        detected_identity: null,
        requires_experimental_mode: false,
        mismatch: false,
        override_scope: null,
        requires_experimental_compatibility_handshake: false,
      },
    });
    mockedMachine.getMachineStatus.mockResolvedValue(status);

    await useMachineStore.getState().continueControllerConnection();

    expect(mockedMachine.continueControllerConnection).toHaveBeenCalledWith(
      'attempt-2',
      { mode: 'known_driver', driver: 'grbl' },
      undefined,
    );
    expect(useMachineStore.getState()).toMatchObject({
      sessionState: 'ready',
      connectedPort: '/dev/cu.usbserial-210',
      machineStatus: status,
      controllerConnectionChallenge: null,
      loading: false,
    });
  });

  it('clears an expired backend challenge so the user can start a new connection', async () => {
    useMachineStore.setState({
      controllerConnectionChallenge: {
        status: 'challenge',
        attempt_id: 'expired-attempt',
        endpoint: {
          type: 'serial',
          port_name: '/dev/cu.usbserial-210',
          baud_rate: 115200,
        },
        detected_identity: null,
        resolution: {
          outcome: 'selection_required',
          override_update: { action: 'keep' },
        },
      },
    });
    mockedMachine.continueControllerConnection.mockRejectedValue(
      new Error('No controller connection decision is pending'),
    );

    await useMachineStore.getState().continueControllerConnection();

    expect(useMachineStore.getState().controllerConnectionChallenge).toBeNull();
    expect(useMachineStore.getState().loading).toBe(false);
  });

  it('hydrates an existing backend session after frontend reload', async () => {
    const status = makeMachineStatus({ run_state: 'alarm' });
    mockedMachine.getSessionState.mockResolvedValue('alarm');
    mockedMachine.getMachineStatus.mockResolvedValue(status);
    mockedMachine.getMachineCoordinatesValid.mockResolvedValue(true);

    await useMachineStore.getState().hydrateSession();

    expect(mockedMachine.getSessionState).toHaveBeenCalled();
    expect(mockedMachine.getMachineStatus).toHaveBeenCalled();
    expect(useMachineStore.getState()).toMatchObject({
      sessionState: 'alarm',
      machineStatus: status,
      machineCoordinatesValid: true,
    });
  });

  it('refreshStatus mirrors current-session machine coordinate validity', async () => {
    const status = makeMachineStatus({ run_state: 'idle' });
    mockedMachine.getMachineStatus.mockResolvedValue(status);
    mockedMachine.getMachineCoordinatesValid.mockResolvedValue(true);
    useMachineStore.setState({
      sessionState: 'ready',
      machineStatus: null,
      machineCoordinatesValid: false,
      error: null,
    });

    await useMachineStore.getState().refreshStatus();

    expect(mockedMachine.getMachineStatus).toHaveBeenCalled();
    expect(mockedMachine.getMachineCoordinatesValid).toHaveBeenCalled();
    expect(useMachineStore.getState()).toMatchObject({
      sessionState: 'ready',
      machineStatus: status,
      machineCoordinatesValid: true,
      error: null,
    });
  });

  it('connect hydrates an existing backend session when the API is already connected', async () => {
    const status = makeMachineStatus({ run_state: 'idle' });
    mockedMachine.beginControllerConnection.mockRejectedValue(
      new Error('Already connected. Disconnect first.'),
    );
    mockedMachine.getSessionState.mockResolvedValue('ready');
    mockedMachine.getMachineStatus.mockResolvedValue(status);
    mockedMachine.getMachineCoordinatesValid.mockResolvedValue(true);
    useMachineStore.setState({
      sessionState: 'disconnected',
      machineStatus: null,
      loading: false,
      error: null,
    });

    await useMachineStore.getState().connect('/dev/cu.usbserial-210', 115200);

    expect(mockedMachine.beginControllerConnection).toHaveBeenCalledWith(
      '/dev/cu.usbserial-210',
      115200,
      { mode: 'known_driver', driver: 'grbl' },
    );
    expect(mockedMachine.getSessionState).toHaveBeenCalled();
    expect(mockedMachine.getMachineStatus).toHaveBeenCalled();
    expect(useMachineStore.getState()).toMatchObject({
      sessionState: 'ready',
      machineStatus: status,
      machineCoordinatesValid: true,
      loading: false,
      error: null,
    });
  });

  it('loadProfiles hydrates activeProfileId from app settings', async () => {
    const profile = makeProfile({ id: 'prof-1', name: 'Hydrated Profile' });
    mockedMachine.getMachineProfiles.mockResolvedValue([profile]);
    mockedApp.getSettings.mockResolvedValue(makeSettings({ active_profile_id: 'prof-1' }));

    await useMachineStore.getState().loadProfiles();

    const state = useMachineStore.getState();
    expect(state.profiles).toEqual([profile]);
    expect(state.activeProfileId).toBe('prof-1');
  });

  it('connectCandidate populates connectedPort from the discovery candidate when available', async () => {
    mockedDiscovery.connectCandidate.mockResolvedValue('ready');
    useMachineStore.setState({
      discoveryState: {
        phase: 'completed',
        status_text: 'Done',
        candidates: [
          {
            id: 'cand-1',
            controller_family: 'gcode',
            controller_model: 'grbl',
            transport_kind: 'serial',
            identity: { display_name: 'GRBL', port_name: '/dev/ttyUSB0' },
            confidence: 0.9,
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
            product_tier: null,
            evidence_state: null,
            status_text: 'Ready',
          },
        ],
        scanned_serial_count: 1,
        scanned_tcp_count: 0,
        scanned_usb_count: 0,
        started_at: null,
        completed_at: null,
      },
    });

    await useMachineStore.getState().connectCandidate('cand-1');

    expect(useMachineStore.getState().connectedPort).toBe('/dev/ttyUSB0');
  });

  it('loadProfiles still updates profiles if app settings fetch fails', async () => {
    const profile = makeProfile({ id: 'prof-5', name: 'Profiles Only' });
    useMachineStore.setState({ activeProfileId: 'prof-5' });
    mockedMachine.getMachineProfiles.mockResolvedValue([profile]);
    mockedApp.getSettings.mockRejectedValue(new Error('settings offline'));

    await useMachineStore.getState().loadProfiles();

    const state = useMachineStore.getState();
    expect(state.profiles).toEqual([profile]);
    expect(state.activeProfileId).toBe('prof-5');
    expect(state.error).toBeNull();
  });

  it('saveProfile refreshes active profile and invalidates preview', async () => {
    const profile = makeProfile({ id: 'prof-2', name: 'Saved Profile' });
    mockedMachine.saveMachineProfile.mockResolvedValue(profile);
    mockedMachine.getMachineProfiles.mockResolvedValue([profile]);
    mockedApp.getSettings.mockResolvedValue(makeSettings({ active_profile_id: 'prof-2' }));
    const invalidate = vi.fn();
    usePreviewStore.setState({ invalidate });

    await useMachineStore.getState().saveProfile(profile);

    expect(useMachineStore.getState().activeProfileId).toBe('prof-2');
    expect(invalidate).toHaveBeenCalledTimes(1);
  });

  it('saveProfile does not invalidate preview for inactive profile edits', async () => {
    const profile = makeProfile({ id: 'prof-2', name: 'Inactive Profile' });
    useMachineStore.setState({ activeProfileId: 'prof-active' });
    mockedMachine.saveMachineProfile.mockResolvedValue(profile);
    mockedMachine.getMachineProfiles.mockResolvedValue([
      makeProfile({ ...profile, id: 'prof-active', name: 'Active Profile' }),
      profile,
    ]);
    mockedApp.getSettings.mockResolvedValue(makeSettings({ active_profile_id: 'prof-active' }));
    const invalidate = vi.fn();
    usePreviewStore.setState({ invalidate });

    await useMachineStore.getState().saveProfile(profile);

    expect(useMachineStore.getState().activeProfileId).toBe('prof-active');
    expect(invalidate).not.toHaveBeenCalled();
  });

  it('setActiveProfile invalidates preview', async () => {
    mockedMachine.setActiveProfile.mockResolvedValue(undefined);
    const invalidate = vi.fn();
    const refreshDevices = vi.fn().mockResolvedValue(undefined);
    const refreshOverlayState = vi.fn().mockResolvedValue(undefined);
    const refreshCalibration = vi.fn().mockResolvedValue(undefined);
    const refreshAlignment = vi.fn().mockResolvedValue(undefined);
    usePreviewStore.setState({ invalidate });
    useCameraStore.setState({
      refreshDevices,
      refreshOverlayState,
      refreshCalibration,
      refreshAlignment,
    });

    await useMachineStore.getState().setActiveProfile('prof-3');

    expect(useMachineStore.getState().activeProfileId).toBe('prof-3');
    expect(refreshDevices).toHaveBeenCalledTimes(1);
    expect(refreshOverlayState).toHaveBeenCalledTimes(1);
    expect(refreshCalibration).toHaveBeenCalledTimes(1);
    expect(refreshAlignment).toHaveBeenCalledTimes(1);
    expect(invalidate).toHaveBeenCalledTimes(1);
  });

  it('setActiveProfile binds the open project so the workspace follows the active bed size', async () => {
    mockedMachine.setActiveProfile.mockResolvedValue(undefined);
    const bindMachineProfile = vi.fn().mockResolvedValue(undefined);
    const originalBindMachineProfile = useProjectStore.getState().bindMachineProfile;
    useProjectStore.setState({
      project: makeProject(),
      bindMachineProfile,
    });

    try {
      await useMachineStore.getState().setActiveProfile('prof-3');

      expect(bindMachineProfile).toHaveBeenCalledTimes(1);
    } finally {
      useProjectStore.setState({
        project: null,
        bindMachineProfile: originalBindMachineProfile,
      });
    }
  });

  it('deleteProfile does not invalidate preview for inactive profiles', async () => {
    useMachineStore.setState({ activeProfileId: 'prof-active' });
    mockedMachine.deleteMachineProfile.mockResolvedValue(undefined);
    mockedMachine.getMachineProfiles.mockResolvedValue([
      makeProfile({ id: 'prof-active', name: 'Active Profile' }),
    ]);
    mockedApp.getSettings.mockResolvedValue(makeSettings({ active_profile_id: 'prof-active' }));
    const invalidate = vi.fn();
    usePreviewStore.setState({ invalidate });

    await useMachineStore.getState().deleteProfile('prof-inactive');

    expect(useMachineStore.getState().activeProfileId).toBe('prof-active');
    expect(invalidate).not.toHaveBeenCalled();
  });

  it('bootstrapProfileFromCandidate syncs active profile and invalidates preview', async () => {
    const profile = makeProfile({ id: 'prof-4', name: 'Bootstrapped Profile' });
    mockedDiscovery.bootstrapProfile.mockResolvedValue(profile);
    mockedMachine.getMachineProfiles.mockResolvedValue([profile]);
    const invalidate = vi.fn();
    usePreviewStore.setState({ invalidate });

    await useMachineStore.getState().bootstrapProfileFromCandidate('candidate-1', 'Test');

    expect(useMachineStore.getState().activeProfileId).toBe('prof-4');
    expect(invalidate).toHaveBeenCalledTimes(1);
  });

  it('suppressed profile events are consumed once', () => {
    suppressProfileEvent('profile.saved', 'prof-1');

    expect(consumeSuppressedProfileEvent('profile.saved', 'prof-1')).toBe(true);
    expect(consumeSuppressedProfileEvent('profile.saved', 'prof-1')).toBe(false);
  });

  it('suppressed profile events do not swallow different profile ids', () => {
    suppressProfileEvent('profile.saved', 'prof-1');

    expect(consumeSuppressedProfileEvent('profile.saved', 'prof-2')).toBe(false);
    expect(consumeSuppressedProfileEvent('profile.saved', 'prof-1')).toBe(true);
  });

  // failed preflight must not leave a stale pass report that lets
  // Start flows proceed on old data.
  describe('runPreflight laser safety', () => {
    it('returns the fresh report on success and stores it', async () => {
      const passReport = {
        outcome: 'pass' as const,
        checks: [{ category: 'connection', description: 'ok', passed: true, message: '' }],
      };
      vi.mocked(machineService.runPreflightCheck).mockResolvedValue(passReport);

      const result = await useMachineStore.getState().runPreflight();

      expect(result).toEqual(passReport);
      expect(useMachineStore.getState().preflightReport).toEqual(passReport);
      expect(useMachineStore.getState().error).toBeNull();
    });

    it('clears a prior passing report and returns null when a later preflight fails', async () => {
      // Seed the store with a prior successful report, as the bug scenario requires.
      const passReport = {
        outcome: 'pass' as const,
        checks: [{ category: 'connection', description: 'ok', passed: true, message: '' }],
      };
      useMachineStore.setState({ preflightReport: passReport });

      vi.mocked(machineService.runPreflightCheck).mockRejectedValue(new Error('backend down'));

      const result = await useMachineStore.getState().runPreflight();

      expect(result).toBeNull();
      // Critical: no stale pass report survives a failed attempt.
      expect(useMachineStore.getState().preflightReport).toBeNull();
      expect(useMachineStore.getState().error).toBe('Error: backend down');
    });

    it('clears any prior report up-front even before the backend responds', async () => {
      const passReport = {
        outcome: 'pass' as const,
        checks: [{ category: 'connection', description: 'ok', passed: true, message: '' }],
      };
      useMachineStore.setState({ preflightReport: passReport });

      let resolveReport: (report: Awaited<ReturnType<typeof machineService.runPreflightCheck>>) => void = () => {};
      vi.mocked(machineService.runPreflightCheck).mockImplementation(
        () => new Promise((r) => { resolveReport = r; }),
      );

      const pending = useMachineStore.getState().runPreflight();

      // Snapshot while the request is still in-flight: stale report must already be gone.
      expect(useMachineStore.getState().preflightReport).toBeNull();

      const failReport = {
        outcome: 'fail' as const,
        checks: [{ category: 'connection', description: 'no', passed: false, message: 'no' }],
      };
      resolveReport(failReport);
      await pending;
    });
  });

  it('cancelJob keeps jobProgress so polling can show the cancelled terminal state', async () => {
    const progress = makeJobProgress({ state: 'running' });
    mockedMachine.cancelJob.mockResolvedValue(undefined);
    useMachineStore.setState({ jobProgress: progress, error: 'stale error' });

    await useMachineStore.getState().cancelJob();

    // useMachinePolling owns terminal-state cleanup (3s display window);
    // cancelJob must not blank the progress bar out from under it.
    expect(useMachineStore.getState().jobProgress).toBe(progress);
    expect(useMachineStore.getState().error).toBeNull();
  });

  it('disconnect clears connectedPort along with session state', async () => {
    mockedMachine.disconnect.mockResolvedValue(undefined);
    useMachineStore.setState({
      sessionState: 'ready',
      connectedPort: '/dev/cu.usbserial-210',
      machineStatus: makeMachineStatus({ run_state: 'idle' }),
      jobProgress: null,
      connectionPreview: false,
    });

    await useMachineStore.getState().disconnect();

    expect(useMachineStore.getState()).toMatchObject({
      sessionState: 'disconnected',
      connectedPort: null,
      machineStatus: null,
      jobProgress: null,
      error: null,
    });
  });

  it('startJob advances auto variable text before starting the machine job', async () => {
    const callOrder: string[] = [];
    const advanceAutoVariableText = vi.fn(async () => {
      callOrder.push('advance');
      return true;
    });
    useProjectStore.setState({
      ...useProjectStore.getState(),
      advanceAutoVariableText,
    });
    mockedMachine.startJob.mockImplementation(async () => {
      callOrder.push('start');
      return {
        state: 'running',
        sent_lines: 0,
        total_lines: 0,
        percent_complete: 0,
      };
    });

    await useMachineStore.getState().startJob();

    expect(advanceAutoVariableText).toHaveBeenCalledOnce();
    expect(mockedMachine.startJob).toHaveBeenCalledOnce();
    expect(callOrder).toEqual(['advance', 'start']);
  });
});
