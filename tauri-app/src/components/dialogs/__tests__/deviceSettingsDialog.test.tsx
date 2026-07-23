import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest';
import { act, render, screen, cleanup, fireEvent, waitFor, within } from '@testing-library/react';
import { DeviceSettingsDialog } from '../DeviceSettingsDialog';
import { useMachineStore } from '../../../stores/machineStore';
import { useNotificationStore } from '../../../stores/notificationStore';
import { useAppStore } from '../../../stores/appStore';
import type { MachineProfile } from '../../../types/machine';

const mockInvoke = vi.fn().mockResolvedValue(null);
const mockOpen = vi.fn().mockResolvedValue(null);
const mockSave = vi.fn().mockResolvedValue(null);
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => mockInvoke(...args) }));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockReturnValue(new Promise(() => {})),
}));
vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: (...args: unknown[]) => mockOpen(...args),
  save: (...args: unknown[]) => mockSave(...args),
}));

const initialMachineState = useMachineStore.getState();
const initialNotificationState = useNotificationStore.getState();
const initialAppState = useAppStore.getState();

function makeProfile(overrides: Partial<MachineProfile> = {}): MachineProfile {
  return {
    id: 'prof-1',
    name: 'Test Machine',
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
    use_g0_for_overscan: true,
    scanning_offsets: [],
    enable_scanning_offset: false,
    dot_width_mm: 0,
    enable_dot_width: false,
    ...overrides,
  };
}

beforeEach(() => {
  mockInvoke.mockReset();
  mockInvoke.mockResolvedValue(null);
  mockOpen.mockReset();
  mockOpen.mockResolvedValue(null);
  mockSave.mockReset();
  mockSave.mockResolvedValue(null);
});

afterEach(() => {
  cleanup();
  useMachineStore.setState(initialMachineState, true);
  useNotificationStore.setState(initialNotificationState, true);
  useAppStore.setState(initialAppState, true);
  mockInvoke.mockClear();
});

describe('DeviceSettingsDialog', () => {
  it('converts max speed value and bounds on the Machine tab in inches mode', async () => {
    useMachineStore.setState({
      profiles: [makeProfile()],
      activeProfileId: 'prof-1',
    });
    useAppStore.setState({
      settings: { display_unit: 'inches', speed_time_unit: 'minutes' } as never,
    });

    render(<DeviceSettingsDialog onClose={vi.fn()} />);
    fireEvent.click(screen.getByTestId('tab-machine'));

    const maxSpeed = (await screen.findByLabelText('Max Speed (in/min)')) as HTMLInputElement;
    // 3000 mm/min -> 118.1 in/min
    expect(maxSpeed.value).toBe('118.1');
    // Bounds must be converted too: 100 mm/min -> 3.9 in/min, 50000 mm/min -> 1968.5 in/min.
    // Raw mm/min bounds would clamp typed inch values incorrectly.
    expect(maxSpeed.min).toBe('3.9');
    expect(maxSpeed.max).toBe('1968.5');
  });

  it('renders 4 tabs (Connection, Machine, GRBL Settings, Controller Info)', async () => {
    const onClose = vi.fn();
    render(<DeviceSettingsDialog onClose={onClose} />);

    // Wait for the ConnectionTab's useEffect (refreshPorts) to settle so the
    // async state update doesn't fire outside act().
    await waitFor(() => {
      expect(screen.getByTestId('connection-tab')).toBeDefined();
    });

    expect(screen.getByTestId('tab-connection')).toBeDefined();
    expect(screen.getByTestId('tab-machine')).toBeDefined();
    expect(screen.getByTestId('tab-grbl')).toBeDefined();
    expect(screen.getByTestId('tab-controller')).toBeDefined();
  });

  it('keeps an actionable connection error visible in the Connection tab', async () => {
    render(<DeviceSettingsDialog onClose={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getByTestId('connection-tab')).toBeDefined();
    });

    act(() => {
      useMachineStore.setState({
        error:
          'transport error: [serial_port_unavailable] Could not open COM5: Accès refusé. The port may be in use by another application or the controller may have been disconnected.',
      });
    });

    expect(screen.getByTestId('device-settings-connection-error').textContent).toBe(
      'Could not open COM5. Another application may be using this port, or the controller may have been disconnected. Close other laser or serial software, reconnect the controller, and try again.',
    );
  });

  it('GRBL tab calls getGrblSettings on mount', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_grbl_settings') {
        return Promise.resolve({ $0: '10', $1: '25' });
      }
      return Promise.resolve(null);
    });

    const onClose = vi.fn();
    render(<DeviceSettingsDialog onClose={onClose} />);

    // Switch to GRBL tab
    fireEvent.click(screen.getByTestId('tab-grbl'));

    await waitFor(() => {
      const grblCalls = mockInvoke.mock.calls.filter(
        (call: unknown[]) => call[0] === 'get_grbl_settings',
      );
      expect(grblCalls.length).toBeGreaterThanOrEqual(1);
    });
  });

  it('Machine tab save calls saveMachineProfile', async () => {
    const testProfile = makeProfile();

    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'save_machine_profile') {
        return Promise.resolve(testProfile);
      }
      if (cmd === 'get_machine_profiles') {
        return Promise.resolve([testProfile]);
      }
      return Promise.resolve(null);
    });

    useMachineStore.setState({
      profiles: [testProfile],
      activeProfileId: 'prof-1',
    });

    const onClose = vi.fn();
    render(<DeviceSettingsDialog onClose={onClose} />);

    // Switch to Machine tab
    fireEvent.click(screen.getByTestId('tab-machine'));

    // Wait for the Machine tab content to render with the profile
    await waitFor(() => {
      expect(screen.getByTestId('machine-save-btn')).toBeDefined();
    });

    // Click Save
    fireEvent.click(screen.getByTestId('machine-save-btn'));

    await waitFor(() => {
      const saveCalls = mockInvoke.mock.calls.filter(
        (call: unknown[]) => call[0] === 'save_machine_profile',
      );
      expect(saveCalls.length).toBeGreaterThanOrEqual(1);
    });
  });

  it('Machine tab exposes persisted origin and keeps unsupported offset/swap controls hidden', async () => {
    const testProfile = makeProfile();

    useMachineStore.setState({
      profiles: [testProfile],
      activeProfileId: 'prof-1',
    });

    render(<DeviceSettingsDialog onClose={vi.fn()} />);
    fireEvent.click(screen.getByTestId('tab-machine'));

    await waitFor(() => {
      expect(screen.getByTestId('machine-save-btn')).toBeDefined();
    });

    expect(screen.getByText('Origin')).toBeDefined();
    expect(screen.queryByText('Enable Laser Offset')).toBeNull();
    expect(screen.queryByText('Swap X/Y')).toBeNull();
    expect(screen.queryByText('Laser Offset X (mm)')).toBeNull();
    expect(screen.queryByText('Laser Offset Y (mm)')).toBeNull();
  });

  it('prompts to save dirty Machine tab profile edits before closing', async () => {
    const testProfile = makeProfile({ origin: 'top_left' });
    const saveProfile = vi.fn().mockResolvedValue(undefined);
    const onClose = vi.fn();

    useMachineStore.setState({
      profiles: [testProfile],
      activeProfileId: 'prof-1',
      saveProfile,
    });

    render(<DeviceSettingsDialog onClose={onClose} />);
    fireEvent.click(screen.getByTestId('tab-machine'));

    await waitFor(() => {
      expect(screen.getByTestId('machine-save-btn')).toBeDefined();
    });

    fireEvent.change(screen.getByLabelText('Origin'), { target: { value: 'bottom_left' } });

    await waitFor(() => {
      expect((screen.getByLabelText('Origin') as HTMLSelectElement).value).toBe('bottom_left');
    });

    fireEvent.click(screen.getByRole('button', { name: 'Close' }));

    expect(screen.getByText('Save machine profile changes before closing?')).toBeDefined();
    expect(onClose).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole('button', { name: 'Save & Close' }));

    await waitFor(() => {
      expect(saveProfile).toHaveBeenCalledWith(expect.objectContaining({ origin: 'bottom_left' }));
      expect(onClose).toHaveBeenCalledTimes(1);
    });
  });

  it('Machine tab renders output policy toggles', async () => {
    const testProfile = makeProfile({
      use_constant_power: false,
      emit_s_every_g1: false,
      use_g0_for_overscan: false,
    });

    useMachineStore.setState({
      profiles: [testProfile],
      activeProfileId: 'prof-1',
    });

    render(<DeviceSettingsDialog onClose={vi.fn()} />);
    fireEvent.click(screen.getByTestId('tab-machine'));

    await waitFor(() => {
      expect(screen.getByText('Output Policy')).toBeDefined();
    });

    expect(screen.getByText('Constant Power (M3)')).toBeDefined();
    expect(screen.getByText('Emit S on Every G1')).toBeDefined();
    expect(screen.getByText('S-value Max')).toBeDefined();
    expect(screen.getByText('Use G0 for Overscan')).toBeDefined();
  });

  it('Machine tab renders calibration section with dot width and scanning offset', async () => {
    const testProfile = makeProfile({
      enable_dot_width: true,
      dot_width_mm: 0.1,
      enable_scanning_offset: true,
      scanning_offsets: [{ speed_mm_min: 1000, offset_mm: 0.1 }],
    });

    useMachineStore.setState({
      profiles: [testProfile],
      activeProfileId: 'prof-1',
    });

    render(<DeviceSettingsDialog onClose={vi.fn()} />);
    fireEvent.click(screen.getByTestId('tab-machine'));

    await waitFor(() => {
      expect(screen.getAllByText('Calibration').length).toBeGreaterThan(0);
    });

    const scrollRegion = screen.getByTestId('device-settings-scroll-region');
    expect(scrollRegion.classList.contains('scrollbar-safe-edge')).toBe(true);
    const dotWidthToggle = screen.getByRole('checkbox', { name: 'Enable Dot Width Correction' });
    expect((dotWidthToggle as HTMLInputElement).checked).toBe(true);
    expect(screen.getByText('Dot Width (mm)')).toBeDefined();
    expect(screen.getByText('Enable Scanning Offset')).toBeDefined();
    expect(screen.getByTestId('scanning-offset-table')).toBeDefined();

    fireEvent.click(dotWidthToggle.closest('label')!);

    expect((dotWidthToggle as HTMLInputElement).checked).toBe(false);
    expect(screen.queryByText('Dot Width (mm)')).toBeNull();
  });

  it('Profiles tab imports with no selection, refreshes and selects without activating', async () => {
    const importedProfile = makeProfile({ id: 'device-imported-1', name: 'Device Shared Profile' });
    const loadProfiles = vi.fn().mockResolvedValue(undefined);
    const setActiveProfile = vi.fn().mockResolvedValue(undefined);
    useMachineStore.setState({
      profiles: [],
      activeProfileId: null,
      loadProfiles,
      saveProfile: vi.fn().mockResolvedValue(undefined),
      deleteProfile: vi.fn().mockResolvedValue(undefined),
      setActiveProfile,
    });
    mockOpen.mockResolvedValue('/tmp/device-shared.bbprofile');
    mockInvoke.mockImplementation((command: string) =>
      command === 'import_machine_profile'
        ? Promise.resolve(importedProfile)
        : Promise.resolve(null),
    );

    render(<DeviceSettingsDialog onClose={vi.fn()} />);
    fireEvent.click(screen.getByTestId('tab-profiles'));

    expect((screen.getByRole('button', { name: 'Export' }) as HTMLButtonElement).disabled).toBe(
      true,
    );
    fireEvent.click(screen.getByRole('button', { name: 'Import' }));

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('import_machine_profile', {
        path: '/tmp/device-shared.bbprofile',
      });
      expect((screen.getByLabelText('Name') as HTMLInputElement).value).toBe(
        'Device Shared Profile',
      );
    });
    expect(loadProfiles).toHaveBeenCalledTimes(2);
    expect(setActiveProfile).not.toHaveBeenCalled();
  });

  it('Profiles tab exports only the saved selection and disables export when dirty', async () => {
    const profile = makeProfile({ id: 'device-saved-1', name: 'Device/Saved' });
    useMachineStore.setState({
      profiles: [profile],
      activeProfileId: null,
      loadProfiles: vi.fn().mockResolvedValue(undefined),
      saveProfile: vi.fn().mockResolvedValue(undefined),
      deleteProfile: vi.fn().mockResolvedValue(undefined),
      setActiveProfile: vi.fn().mockResolvedValue(undefined),
    });
    mockSave.mockResolvedValue('/tmp/device-export');
    mockInvoke.mockResolvedValue(undefined);

    render(<DeviceSettingsDialog onClose={vi.fn()} />);
    fireEvent.click(screen.getByTestId('tab-profiles'));
    fireEvent.click(screen.getByText('Device/Saved'));

    const exportButton = screen.getByRole('button', { name: 'Export' }) as HTMLButtonElement;
    expect(exportButton.disabled).toBe(false);
    fireEvent.click(exportButton);

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('export_machine_profile', {
        profileId: 'device-saved-1',
        path: '/tmp/device-export.bbprofile',
      });
    });

    fireEvent.change(screen.getByLabelText('Name'), { target: { value: 'Device Unsaved' } });
    expect(exportButton.disabled).toBe(true);
    expect(mockSave).toHaveBeenCalledTimes(1);
  });

  it('Profiles tab holds dirty imports behind discard confirmation', async () => {
    const profile = makeProfile({ id: 'device-saved-2', name: 'Device Current' });
    const importedProfile = makeProfile({ id: 'device-imported-2', name: 'Device Imported' });
    const loadProfiles = vi.fn().mockResolvedValue(undefined);
    const setActiveProfile = vi.fn().mockResolvedValue(undefined);
    useMachineStore.setState({
      profiles: [profile],
      activeProfileId: null,
      loadProfiles,
      saveProfile: vi.fn().mockResolvedValue(undefined),
      deleteProfile: vi.fn().mockResolvedValue(undefined),
      setActiveProfile,
    });
    mockOpen.mockResolvedValue('/tmp/device-replacement.bbprofile');
    mockInvoke.mockImplementation((command: string) =>
      command === 'import_machine_profile'
        ? Promise.resolve(importedProfile)
        : Promise.resolve(null),
    );

    render(<DeviceSettingsDialog onClose={vi.fn()} />);
    fireEvent.click(screen.getByTestId('tab-profiles'));
    fireEvent.click(screen.getByText('Device Current'));
    fireEvent.change(screen.getByLabelText('Name'), { target: { value: 'Device Unsaved' } });
    fireEvent.click(screen.getByRole('button', { name: 'Import' }));

    await waitFor(() => {
      expect(screen.getByText('Discard unsaved machine profile changes?')).toBeDefined();
    });
    expect(mockInvoke.mock.calls.some(([command]) => command === 'import_machine_profile')).toBe(
      false,
    );
    expect((screen.getByLabelText('Name') as HTMLInputElement).value).toBe('Device Unsaved');

    fireEvent.click(screen.getByRole('button', { name: 'Discard' }));

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('import_machine_profile', {
        path: '/tmp/device-replacement.bbprofile',
      });
      expect((screen.getByLabelText('Name') as HTMLInputElement).value).toBe('Device Imported');
    });
    expect(setActiveProfile).not.toHaveBeenCalled();
  });

  it('Profiles tab exposes output policy and calibration controls for non-active profiles', async () => {
    const testProfile = makeProfile({
      id: 'prof-2',
      name: 'Saved Profile',
      use_constant_power: true,
      emit_s_every_g1: true,
      use_g0_for_overscan: true,
      enable_dot_width: true,
      dot_width_mm: 0.12,
      enable_scanning_offset: true,
      scanning_offsets: [{ speed_mm_min: 1200, offset_mm: 0.08 }],
    });

    useMachineStore.setState({
      profiles: [testProfile],
      activeProfileId: null,
      loadProfiles: vi.fn().mockResolvedValue(undefined),
    });

    render(<DeviceSettingsDialog onClose={vi.fn()} />);
    fireEvent.click(screen.getByTestId('tab-profiles'));
    fireEvent.click(screen.getByText('Saved Profile'));

    await waitFor(() => {
      expect(screen.getByText('Output Policy')).toBeDefined();
    });

    expect(screen.getByText('Constant Power (M3)')).toBeDefined();
    expect(screen.getByText('Emit S on Every G1')).toBeDefined();
    expect(screen.getByText('S-value Max')).toBeDefined();
    expect(screen.getByText('Use G0 for Overscan')).toBeDefined();
    expect(screen.getByText('Origin')).toBeDefined();
    expect(screen.getByText('Camera Metadata')).toBeDefined();
    expect(screen.getAllByText('Calibration').length).toBeGreaterThan(0);
    expect(screen.getByRole('checkbox', { name: 'Enable Dot Width Correction' })).toBeDefined();
    expect(screen.getByText('Enable Scanning Offset')).toBeDefined();
    expect(screen.getByTestId('scanning-offset-table')).toBeDefined();
  });

  it('Profiles tab edits and saves multiline start and end G-code', async () => {
    const saveProfile = vi.fn().mockResolvedValue(undefined);
    const testProfile = makeProfile({
      id: 'prof-gcode',
      name: 'G-code Profile',
      job_header_gcode: 'T10M6\nG54',
      job_footer_gcode: 'M5\nM30',
    });

    useMachineStore.setState({
      profiles: [testProfile],
      activeProfileId: null,
      loadProfiles: vi.fn().mockResolvedValue(undefined),
      saveProfile,
      deleteProfile: vi.fn().mockResolvedValue(undefined),
      setActiveProfile: vi.fn().mockResolvedValue(undefined),
    });

    render(<DeviceSettingsDialog onClose={vi.fn()} />);
    fireEvent.click(screen.getByTestId('tab-profiles'));
    fireEvent.click(screen.getByText('G-code Profile'));

    const startGcode = (await screen.findByLabelText('Start G-code')) as HTMLTextAreaElement;
    const endGcode = screen.getByLabelText('End G-code') as HTMLTextAreaElement;

    expect(startGcode.tagName).toBe('TEXTAREA');
    expect(startGcode.value).toBe('T10M6\nG54');
    expect(endGcode.tagName).toBe('TEXTAREA');
    expect(endGcode.value).toBe('M5\nM30');
    const help = screen.getByText(
      /End G-code runs after planned motion and Z return, but before Beam Bench's final M5/,
    );
    expect(startGcode.getAttribute('aria-describedby')).toBe(help.id);
    expect(endGcode.getAttribute('aria-describedby')).toBe(help.id);

    fireEvent.change(startGcode, { target: { value: 'T10M6\nG55\nG90' } });
    fireEvent.change(endGcode, { target: { value: 'M5\nG0 X0 Y0' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    await waitFor(() => {
      expect(saveProfile).toHaveBeenCalledWith(
        expect.objectContaining({
          job_header_gcode: 'T10M6\nG55\nG90',
          job_footer_gcode: 'M5\nG0 X0 Y0',
        }),
      );
    });
  });

  it('preserves per-profile drafts when switching the active profile from Connection', async () => {
    const profiles = [
      makeProfile({ id: 'prof-1', name: 'Profile One' }),
      makeProfile({ id: 'prof-2', name: 'Profile Two', bed_width_mm: 500, bed_height_mm: 320 }),
    ];

    const setActiveProfile = vi.fn(async (id: string | null) => {
      useMachineStore.setState({ activeProfileId: id });
    });

    useMachineStore.setState({
      profiles,
      activeProfileId: 'prof-1',
      setActiveProfile,
      refreshPorts: vi.fn().mockResolvedValue(undefined),
    });

    render(<DeviceSettingsDialog onClose={vi.fn()} />);
    fireEvent.click(screen.getByTestId('tab-machine'));

    await waitFor(() => {
      expect(screen.getByLabelText('Bed Width (mm)')).toBeDefined();
    });

    fireEvent.change(screen.getByLabelText('Bed Width (mm)'), { target: { value: '450' } });

    fireEvent.click(screen.getByTestId('tab-connection'));
    fireEvent.change(screen.getByLabelText('Profile'), { target: { value: 'prof-2' } });

    await waitFor(() => {
      expect(setActiveProfile).toHaveBeenCalledWith('prof-2');
    });

    fireEvent.click(screen.getByTestId('tab-machine'));

    await waitFor(() => {
      expect((screen.getByLabelText('Bed Width (mm)') as HTMLInputElement).value).toBe('500');
    });

    fireEvent.click(screen.getByTestId('tab-connection'));
    fireEvent.change(screen.getByLabelText('Profile'), { target: { value: 'prof-1' } });
    fireEvent.click(screen.getByTestId('tab-machine'));

    await waitFor(() => {
      expect((screen.getByLabelText('Bed Width (mm)') as HTMLInputElement).value).toBe('450');
    });
  });

  it('guards switching profiles in the Profiles tab when edits are dirty', async () => {
    const profiles = [
      makeProfile({ id: 'prof-1', name: 'Profile One' }),
      makeProfile({ id: 'prof-2', name: 'Profile Two', bed_width_mm: 500, bed_height_mm: 320 }),
    ];

    useMachineStore.setState({
      profiles,
      activeProfileId: null,
      loadProfiles: vi.fn().mockResolvedValue(undefined),
    });

    render(<DeviceSettingsDialog onClose={vi.fn()} />);
    fireEvent.click(screen.getByTestId('tab-profiles'));
    fireEvent.click(screen.getByText('Profile One'));

    await waitFor(() => {
      expect((screen.getByLabelText('Name') as HTMLInputElement).value).toBe('Profile One');
    });

    fireEvent.change(screen.getByLabelText('Name'), { target: { value: 'Dirty Name' } });
    fireEvent.click(screen.getByText('Profile Two'));

    expect(screen.getByText('Discard unsaved machine profile changes?')).toBeDefined();
    expect((screen.getByLabelText('Name') as HTMLInputElement).value).toBe('Dirty Name');

    fireEvent.click(screen.getByRole('button', { name: 'Keep Editing' }));
    expect(screen.queryByText('Discard unsaved machine profile changes?')).toBeNull();
    expect((screen.getByLabelText('Name') as HTMLInputElement).value).toBe('Dirty Name');

    fireEvent.click(screen.getByText('Profile Two'));
    fireEvent.click(screen.getByRole('button', { name: 'Discard' }));
    await waitFor(() => {
      expect((screen.getByLabelText('Name') as HTMLInputElement).value).toBe('Profile Two');
    });
  });

  it('Profiles tab preserves dirty edits when profile list refreshes', async () => {
    const staleProfile = makeProfile({
      id: 'prof-3',
      name: 'Needs Normalize',
      enable_scanning_offset: true,
      scanning_offsets: [
        { speed_mm_min: 3000, offset_mm: 0.3 },
        { speed_mm_min: 1000, offset_mm: 0.1 },
      ],
    });

    useMachineStore.setState({
      profiles: [staleProfile],
      activeProfileId: null,
      loadProfiles: vi.fn().mockResolvedValue(undefined),
    });

    render(<DeviceSettingsDialog onClose={vi.fn()} />);
    fireEvent.click(screen.getByTestId('tab-profiles'));
    fireEvent.click(screen.getByText('Needs Normalize'));

    await waitFor(() => {
      expect(screen.getByTestId('scanning-offset-table')).toBeDefined();
    });

    fireEvent.change(screen.getByLabelText('Name'), { target: { value: 'Unsaved Name' } });

    await act(async () => {
      useMachineStore.setState({
        profiles: [
          {
            ...staleProfile,
            scanning_offsets: [
              { speed_mm_min: 1000, offset_mm: 0.1 },
              { speed_mm_min: 3000, offset_mm: 0.3 },
            ],
          },
        ],
      });
    });

    expect((screen.getByLabelText('Name') as HTMLInputElement).value).toBe('Unsaved Name');
    const inputs = within(screen.getByTestId('scanning-offset-table')).getAllByRole('spinbutton');
    expect((inputs[0] as HTMLInputElement).value).toBe('3000');
  });

  it('Profiles tab defaults new profiles to bottom-left origin', async () => {
    useMachineStore.setState({
      profiles: [],
      activeProfileId: null,
      loadProfiles: vi.fn().mockResolvedValue(undefined),
    });

    render(<DeviceSettingsDialog onClose={vi.fn()} />);
    fireEvent.click(screen.getByTestId('tab-profiles'));
    fireEvent.click(screen.getByText('New'));

    await waitFor(() => {
      expect((screen.getByLabelText('Origin') as HTMLSelectElement).value).toBe('bottom_left');
    });
    expect((screen.getByRole('button', { name: 'Export' }) as HTMLButtonElement).disabled).toBe(
      true,
    );
  });

  it('Profiles tab saves a new profile before activating it and keeps delete disabled', async () => {
    const callOrder: string[] = [];
    const saveProfile = vi.fn(async (_profile: MachineProfile) => {
      callOrder.push('save');
    });
    const setActiveProfile = vi.fn(async (_profileId: string | null) => {
      callOrder.push('activate');
    });
    const deleteProfile = vi.fn().mockResolvedValue(undefined);
    useMachineStore.setState({
      profiles: [],
      activeProfileId: null,
      loadProfiles: vi.fn().mockResolvedValue(undefined),
      saveProfile,
      deleteProfile,
      setActiveProfile,
    });

    render(<DeviceSettingsDialog onClose={vi.fn()} />);
    fireEvent.click(screen.getByTestId('tab-profiles'));
    fireEvent.click(screen.getByText('New'));

    const deleteButton = await screen.findByRole('button', { name: 'Delete' });
    expect((deleteButton as HTMLButtonElement).disabled).toBe(true);

    fireEvent.click(screen.getByRole('button', { name: 'Set Active' }));

    await waitFor(() => {
      expect(saveProfile).toHaveBeenCalledTimes(1);
      expect(setActiveProfile).toHaveBeenCalledTimes(1);
    });
    const savedProfile = saveProfile.mock.calls[0][0];
    expect(setActiveProfile).toHaveBeenCalledWith(savedProfile.id);
    expect(callOrder).toEqual(['save', 'activate']);
    expect(deleteProfile).not.toHaveBeenCalled();
  });

  it('Profiles tab does not activate a new profile when saving fails', async () => {
    const saveProfile = vi.fn().mockRejectedValue(new Error('save failed'));
    const setActiveProfile = vi.fn().mockResolvedValue(undefined);
    useMachineStore.setState({
      profiles: [],
      activeProfileId: null,
      loadProfiles: vi.fn().mockResolvedValue(undefined),
      saveProfile,
      deleteProfile: vi.fn().mockResolvedValue(undefined),
      setActiveProfile,
    });

    render(<DeviceSettingsDialog onClose={vi.fn()} />);
    fireEvent.click(screen.getByTestId('tab-profiles'));
    fireEvent.click(screen.getByText('New'));
    fireEvent.click(await screen.findByRole('button', { name: 'Set Active' }));

    await waitFor(() => {
      expect(saveProfile).toHaveBeenCalledTimes(1);
    });
    expect(setActiveProfile).not.toHaveBeenCalled();
    expect(screen.getByLabelText('Name')).toBeDefined();
  });

  it('scanning offset table add/remove entries', async () => {
    const testProfile = makeProfile({
      enable_scanning_offset: true,
      scanning_offsets: [] as { speed_mm_min: number; offset_mm: number }[],
    });

    useMachineStore.setState({
      profiles: [testProfile],
      activeProfileId: 'prof-1',
    });

    render(<DeviceSettingsDialog onClose={vi.fn()} />);
    fireEvent.click(screen.getByTestId('tab-machine'));

    await waitFor(() => {
      expect(screen.getByTestId('scanning-offset-table')).toBeDefined();
    });

    // Add an entry
    fireEvent.click(screen.getByText('Add Entry'));

    await waitFor(() => {
      // Should now have a remove button for the new entry
      expect(screen.getByTitle('Remove entry')).toBeDefined();
    });

    // Remove it
    fireEvent.click(screen.getByTitle('Remove entry'));

    await waitFor(() => {
      expect(screen.queryByTitle('Remove entry')).toBeNull();
    });
  });

  it('Machine tab save passes output-policy and calibration fields', async () => {
    const testProfile = makeProfile({
      use_constant_power: true,
      emit_s_every_g1: true,
      use_g0_for_overscan: false,
      enable_dot_width: false,
      dot_width_mm: 0,
      enable_scanning_offset: false,
      scanning_offsets: [],
    });

    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'save_machine_profile') {
        return Promise.resolve(testProfile);
      }
      return Promise.resolve(null);
    });

    useMachineStore.setState({
      profiles: [testProfile],
      activeProfileId: 'prof-1',
    });

    render(<DeviceSettingsDialog onClose={vi.fn()} />);
    fireEvent.click(screen.getByTestId('tab-machine'));

    await waitFor(() => {
      expect(screen.getByTestId('machine-save-btn')).toBeDefined();
    });

    fireEvent.click(screen.getByTestId('machine-save-btn'));

    await waitFor(() => {
      const saveCalls = mockInvoke.mock.calls.filter(
        (call: unknown[]) => call[0] === 'save_machine_profile',
      );
      expect(saveCalls.length).toBeGreaterThanOrEqual(1);
      const savedArgs = saveCalls[0][1] as { profile: Record<string, unknown> };
      expect(savedArgs.profile.use_constant_power).toBe(true);
      expect(savedArgs.profile.emit_s_every_g1).toBe(true);
    });
  });

  it('Machine tab preserves dirty edits when the active profile refreshes', async () => {
    const profile = makeProfile({ id: 'prof-4', name: 'Active Profile' });

    useMachineStore.setState({
      profiles: [profile],
      activeProfileId: 'prof-4',
    });

    render(<DeviceSettingsDialog onClose={vi.fn()} />);
    fireEvent.click(screen.getByTestId('tab-machine'));

    await waitFor(() => {
      expect((screen.getByLabelText('Bed Width (mm)') as HTMLInputElement).value).toBe('400');
    });

    fireEvent.change(screen.getByLabelText('Bed Width (mm)'), { target: { value: '450' } });

    await act(async () => {
      useMachineStore.setState({
        profiles: [{ ...profile, bed_width_mm: 420 }],
        activeProfileId: 'prof-4',
      });
    });

    expect((screen.getByLabelText('Bed Width (mm)') as HTMLInputElement).value).toBe('450');
  });

  it('Profiles tab keeps the editor open when delete fails', async () => {
    const deleteProfile = vi.fn().mockRejectedValue(new Error('delete failed'));
    const profile = makeProfile({ id: 'prof-5', name: 'Delete Fail' });

    useMachineStore.setState({
      profiles: [profile],
      activeProfileId: null,
      deleteProfile,
      loadProfiles: vi.fn().mockResolvedValue(undefined),
    });

    render(<DeviceSettingsDialog onClose={vi.fn()} />);
    fireEvent.click(screen.getByTestId('tab-profiles'));
    fireEvent.click(screen.getByText('Delete Fail'));

    await waitFor(() => {
      expect((screen.getByLabelText('Name') as HTMLInputElement).value).toBe('Delete Fail');
    });

    fireEvent.click(screen.getByText('Delete'));

    await waitFor(() => {
      expect(deleteProfile).toHaveBeenCalledWith('prof-5');
    });
    expect((screen.getByLabelText('Name') as HTMLInputElement).value).toBe('Delete Fail');
  });

  it('GRBL tab shows backend errors instead of a fake empty state', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_grbl_settings') {
        return Promise.reject(new Error('No active machine session'));
      }
      return Promise.resolve(null);
    });

    render(<DeviceSettingsDialog onClose={vi.fn()} />);
    fireEvent.click(screen.getByTestId('tab-grbl'));

    await waitFor(() => {
      expect(screen.getByTestId('grbl-error').textContent).toContain('No active machine session');
    });
    expect(screen.queryByText('No GRBL settings available')).toBeNull();
  });

  it('applies live GRBL power and speed values to the active profile', async () => {
    const profile = makeProfile({
      id: 'prof-apply',
      name: 'Active Profile',
      s_value_max: 1000,
      max_speed_mm_min: 3000,
    });
    const saveProfile = vi.fn().mockResolvedValue(undefined);
    const push = vi.fn();
    useMachineStore.setState({
      profiles: [profile],
      activeProfileId: 'prof-apply',
      saveProfile,
    });
    useNotificationStore.setState({ push });
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_grbl_settings') {
        return Promise.resolve({ $30: '255', $110: '8000', $111: '6000' });
      }
      return Promise.resolve(null);
    });

    render(<DeviceSettingsDialog onClose={vi.fn()} />);
    fireEvent.click(screen.getByTestId('tab-grbl'));

    await screen.findByText('$30');
    fireEvent.click(screen.getByText('Apply to Active Profile'));

    await waitFor(() => {
      expect(saveProfile).toHaveBeenCalledWith(
        expect.objectContaining({
          id: 'prof-apply',
          s_value_max: 255,
          max_speed_mm_min: 6000,
        }),
      );
    });
    expect(push).toHaveBeenCalledWith('Applied GRBL settings to active profile', 'success');
  });

  it('Controller tab shows backend errors instead of a fake empty state', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_controller_info') {
        return Promise.reject(new Error('Only GRBL sessions support controller info query'));
      }
      return Promise.resolve(null);
    });

    render(<DeviceSettingsDialog onClose={vi.fn()} />);
    fireEvent.click(screen.getByTestId('tab-controller'));

    await waitFor(() => {
      expect(screen.getByTestId('controller-error').textContent).toContain(
        'Only GRBL sessions support controller info query',
      );
    });
    expect(screen.queryByText('No controller info available')).toBeNull();
  });

  it('allows retrying connection from transient states', async () => {
    const connect = vi.fn();

    useMachineStore.setState({
      sessionState: 'connecting',
      // full PortInfo shape matches backend serialization.
      availablePorts: [
        { port_name: '/dev/ttyUSB0', description: '', manufacturer: '', vid: null, pid: null },
      ],
      profiles: [],
      activeProfileId: null,
      loading: false,
      connect,
      disconnect: vi.fn(),
      refreshPorts: vi.fn(),
      setActiveProfile: vi.fn(),
    });

    render(<DeviceSettingsDialog onClose={vi.fn()} />);

    fireEvent.change(screen.getByLabelText('Port'), { target: { value: '/dev/ttyUSB0' } });
    fireEvent.click(screen.getByText('Connect'));

    await waitFor(() => {
      expect(connect).toHaveBeenCalledWith('/dev/ttyUSB0', 115200);
    });
  });

  it('connects a network controller from the Devices connection tab', async () => {
    const connectNetwork = vi.fn();
    useMachineStore.setState({
      sessionState: 'disconnected',
      connectedPort: null,
      availablePorts: [],
      profiles: [],
      activeProfileId: null,
      loading: false,
      controllerConnectionChallenge: null,
      controllerSelection: { mode: 'known_driver', driver: 'grbl' },
      connect: vi.fn(),
      connectNetwork,
      disconnect: vi.fn(),
      refreshPorts: vi.fn().mockResolvedValue(undefined),
      setActiveProfile: vi.fn().mockResolvedValue(undefined),
    });

    render(<DeviceSettingsDialog onClose={vi.fn()} />);

    fireEvent.change(screen.getByLabelText('Connection'), { target: { value: 'tcp' } });
    fireEvent.change(screen.getByLabelText('Host'), { target: { value: 'grblhal.local' } });
    fireEvent.change(screen.getByLabelText('TCP port'), { target: { value: '23' } });
    fireEvent.click(screen.getByText('Connect'));

    expect(connectNetwork).toHaveBeenCalledWith('grblhal.local', 23);
  });

  it('uses the Ruida UDP endpoint defaults without an extra confirmation', async () => {
    const connectNetwork = vi.fn();
    useMachineStore.setState({
      sessionState: 'disconnected',
      connectedPort: null,
      availablePorts: [],
      profiles: [],
      activeProfileId: null,
      loading: false,
      controllerConnectionChallenge: null,
      controllerSelection: { mode: 'known_driver', driver: 'grbl' },
      connect: vi.fn(),
      connectNetwork,
      disconnect: vi.fn(),
      refreshPorts: vi.fn().mockResolvedValue(undefined),
      setActiveProfile: vi.fn().mockResolvedValue(undefined),
    });

    render(<DeviceSettingsDialog onClose={vi.fn()} />);

    fireEvent.change(screen.getByLabelText('Connection'), { target: { value: 'tcp' } });
    fireEvent.change(screen.getByLabelText('Controller'), { target: { value: 'ruida' } });
    await waitFor(() => {
      expect((screen.getByLabelText('UDP port') as HTMLInputElement).value).toBe('50200');
    });
    fireEvent.change(screen.getByLabelText('Host'), { target: { value: '192.168.1.100' } });
    fireEvent.click(screen.getByText('Connect'));

    expect(connectNetwork).toHaveBeenCalledWith('192.168.1.100', 50200);
  });

  it('uses the LaserPecker LX2 TCP endpoint defaults without an extra confirmation', async () => {
    const connectNetwork = vi.fn();
    useMachineStore.setState({
      sessionState: 'disconnected',
      connectedPort: null,
      availablePorts: [],
      profiles: [],
      activeProfileId: null,
      loading: false,
      controllerConnectionChallenge: null,
      controllerSelection: { mode: 'known_driver', driver: 'grbl' },
      connect: vi.fn(),
      connectNetwork,
      disconnect: vi.fn(),
      refreshPorts: vi.fn().mockResolvedValue(undefined),
      setActiveProfile: vi.fn().mockResolvedValue(undefined),
    });

    render(<DeviceSettingsDialog onClose={vi.fn()} />);

    fireEvent.change(screen.getByLabelText('Connection'), { target: { value: 'tcp' } });
    fireEvent.change(screen.getByLabelText('Controller'), { target: { value: 'laser_pecker' } });
    await waitFor(() => {
      expect((screen.getByLabelText('TCP port') as HTMLInputElement).value).toBe('8888');
    });
    const host = screen.getByLabelText('Host') as HTMLInputElement;
    expect(host.placeholder).toBe('192.168.253.1');
    expect(host.value).toBe('192.168.253.1');
    fireEvent.click(screen.getByText('Connect'));

    expect(connectNetwork).toHaveBeenCalledWith('192.168.253.1', 8888);
  });

  it('filters obvious non-machine ports in the Devices connection tab', async () => {
    useMachineStore.setState({
      sessionState: 'disconnected',
      connectedPort: null,
      availablePorts: [
        {
          port_name: '/dev/cu.debug-console',
          description: '',
          manufacturer: '',
          vid: null,
          pid: null,
        },
        {
          port_name: '/dev/cu.Bluetooth-Incoming-Port',
          description: '',
          manufacturer: '',
          vid: null,
          pid: null,
        },
        {
          port_name: '/dev/cu.usbmodem1101',
          description: 'GRBL Controller',
          manufacturer: 'OpenBuilds',
          vid: null,
          pid: null,
        },
      ],
      profiles: [],
      activeProfileId: null,
      loading: false,
      connect: vi.fn(),
      disconnect: vi.fn(),
      refreshPorts: vi.fn().mockResolvedValue(undefined),
      setActiveProfile: vi.fn().mockResolvedValue(undefined),
    });

    render(<DeviceSettingsDialog onClose={vi.fn()} />);

    const portSelect = screen.getByLabelText('Port');
    await waitFor(() => {
      expect(
        within(portSelect).getByRole('option', {
          name: /\/dev\/cu\.usbmodem1101/,
        }),
      ).toBeDefined();
    });

    expect(within(portSelect).queryByRole('option', { name: /debug-console/ })).toBeNull();
    expect(
      within(portSelect).queryByRole('option', { name: /Bluetooth-Incoming-Port/ }),
    ).toBeNull();

    fireEvent.click(screen.getByLabelText('Show all ports (2 hidden)'));

    expect(within(portSelect).getByRole('option', { name: /debug-console/ })).toBeDefined();
    expect(
      within(portSelect).getByRole('option', { name: /Bluetooth-Incoming-Port/ }),
    ).toBeDefined();
  });

  it('renders the connected port from machineStore in the dialog connection tab', async () => {
    useMachineStore.setState({
      sessionState: 'ready',
      connectedPort: '/dev/ttyUSB0',
      // full PortInfo shape matches backend serialization.
      availablePorts: [
        { port_name: '/dev/ttyUSB0', description: '', manufacturer: '', vid: null, pid: null },
      ],
      profiles: [],
      activeProfileId: null,
      loading: false,
      connect: vi.fn(),
      disconnect: vi.fn(),
      refreshPorts: vi.fn().mockResolvedValue(undefined),
      setActiveProfile: vi.fn().mockResolvedValue(undefined),
    });

    render(<DeviceSettingsDialog onClose={vi.fn()} />);

    await waitFor(() => {
      expect((screen.getByLabelText('Port') as HTMLSelectElement).value).toBe('/dev/ttyUSB0');
    });
  });

  it('surfaces GRBL fetch errors instead of the fake empty state', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_grbl_settings') {
        return Promise.reject(new Error('No active GRBL session'));
      }
      return Promise.resolve(null);
    });

    render(<DeviceSettingsDialog onClose={vi.fn()} />);
    fireEvent.click(screen.getByTestId('tab-grbl'));

    expect((await screen.findByTestId('grbl-error')).textContent).toContain(
      'No active GRBL session',
    );
    expect(screen.queryByText('No GRBL settings available')).toBeNull();
  });

  it('surfaces controller fetch errors instead of the fake empty state', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_controller_info') {
        return Promise.reject(new Error('Controller info only available for GRBL sessions'));
      }
      return Promise.resolve(null);
    });

    render(<DeviceSettingsDialog onClose={vi.fn()} />);
    fireEvent.click(screen.getByTestId('tab-controller'));

    expect((await screen.findByTestId('controller-error')).textContent).toContain(
      'Controller info only available for GRBL sessions',
    );
    expect(screen.queryByText('No controller info available')).toBeNull();
  });

  it('saves an extended GRBL setting ID without truncating the command payload', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_grbl_settings') {
        return Promise.resolve({ $65535: '1' });
      }
      return Promise.resolve(null);
    });

    render(<DeviceSettingsDialog onClose={vi.fn()} />);
    fireEvent.click(screen.getByTestId('tab-grbl'));

    const key = await screen.findByText('$65535');
    const row = key.parentElement;
    expect(row).not.toBeNull();
    fireEvent.click(within(row as HTMLElement).getByText('1'));
    fireEvent.change(screen.getByDisplayValue('1'), { target: { value: '2' } });
    fireEvent.click(screen.getByText('OK'));

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('set_grbl_setting', {
        key: 65535,
        value: 2,
      });
    });
  });

  it('does not coerce a partially numeric GRBL setting value', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_grbl_settings') {
        return Promise.resolve({ $376: '1' });
      }
      return Promise.resolve(null);
    });

    render(<DeviceSettingsDialog onClose={vi.fn()} />);
    fireEvent.click(screen.getByTestId('tab-grbl'));

    const key = await screen.findByText('$376');
    const row = key.parentElement;
    expect(row).not.toBeNull();
    fireEvent.click(within(row as HTMLElement).getByText('1'));
    fireEvent.change(screen.getByDisplayValue('1'), { target: { value: '2junk' } });
    fireEvent.click(screen.getByText('OK'));

    await screen.findByTestId('grbl-save-error');
    expect(mockInvoke.mock.calls.some(([command]) => command === 'set_grbl_setting')).toBe(false);
  });

  it('keeps the GRBL row editable and notifies when saving fails', async () => {
    const push = vi.fn();
    useNotificationStore.setState({ push });
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_grbl_settings') {
        return Promise.resolve({ $0: '10' });
      }
      if (cmd === 'set_grbl_setting') {
        return Promise.reject(new Error('No active GRBL session'));
      }
      return Promise.resolve(null);
    });

    render(<DeviceSettingsDialog onClose={vi.fn()} />);
    fireEvent.click(screen.getByTestId('tab-grbl'));

    await screen.findByText('$0');
    fireEvent.click(screen.getByText('10'));
    fireEvent.change(screen.getByDisplayValue('10'), { target: { value: '20' } });
    fireEvent.click(screen.getByText('OK'));

    await waitFor(() => {
      expect(push).toHaveBeenCalledWith('No active GRBL session', 'error');
    });
    expect(screen.getByDisplayValue('20')).toBeDefined();
    expect(screen.getByTestId('grbl-save-error').textContent).toContain('No active GRBL session');
  });

  it('prevents duplicate discovery bootstrap clicks while a candidate request is pending', async () => {
    const bootstrapProfileFromCandidate = vi.fn(() => new Promise<void>(() => {}));

    useMachineStore.setState({
      discoveryState: {
        phase: 'completed',
        status_text: 'Found Mock Laser',
        scanned_serial_count: 1,
        scanned_tcp_count: 0,
        scanned_usb_count: 0,
        started_at: '2026-03-20T12:00:00Z',
        completed_at: '2026-03-20T12:00:01Z',
        candidates: [
          {
            id: 'cand-1',
            controller_family: 'gcode',
            controller_model: 'grbl',
            transport_kind: 'serial',
            identity: {
              display_name: 'Mock Laser',
              manufacturer: null,
              serial_number: null,
              port_name: '/dev/tty.mock',
            },
            confidence: 0.95,
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
              supports_camera_alignment: true,
            },
            product_tier: null,
            evidence_state: null,
            status_text: 'Ready to connect',
            unsupported_reason: null,
          },
        ],
      },
      bootstrapProfileFromCandidate,
      connectCandidate: vi.fn().mockResolvedValue(undefined),
      refreshDiscoveryState: vi.fn().mockResolvedValue(undefined),
      startDiscovery: vi.fn().mockResolvedValue(undefined),
      cancelDiscovery: vi.fn().mockResolvedValue(undefined),
      loading: false,
    });

    render(<DeviceSettingsDialog onClose={vi.fn()} />);
    fireEvent.click(screen.getByTestId('tab-discovery'));

    const bootstrapButton = await screen.findByText('Bootstrap Profile');
    fireEvent.click(bootstrapButton);
    fireEvent.click(bootstrapButton);

    expect(bootstrapProfileFromCandidate).toHaveBeenCalledTimes(1);
  });

  it('disables Connect for an unavailable discovery candidate but keeps draft profile bootstrap available', async () => {
    const unsupportedReason =
      "Beam Bench can't identify a controller from serial-port metadata alone.";
    const connectCandidate = vi.fn().mockResolvedValue(undefined);
    const bootstrapProfileFromCandidate = vi.fn().mockResolvedValue(undefined);

    useMachineStore.setState({
      discoveryState: {
        phase: 'completed',
        status_text: 'Listed 1 connection candidate(s)',
        scanned_serial_count: 1,
        scanned_tcp_count: 0,
        scanned_usb_count: 0,
        started_at: '2026-03-20T12:00:00Z',
        completed_at: '2026-03-20T12:00:01Z',
        candidates: [
          {
            id: 'unknown-serial',
            controller_family: 'unknown',
            controller_model: 'unknown',
            transport_kind: 'serial',
            identity: {
              display_name: 'USB Serial Device',
              port_name: '/dev/ttyUSB0',
            },
            confidence: 0,
            capabilities: {
              can_home: false,
              can_jog: false,

              can_jog_continuous: false,
              can_unlock: false,
              can_pause_resume: false,
              can_set_origin: false,
              can_frame: false,
              can_run_job: false,

              reports_absolute_position: false,

              can_manual_fire: false,

              can_adjust_overrides: false,
              supports_rotary: false,
              supports_cylinder: false,
              supports_camera_alignment: false,
            },
            product_tier: 'unavailable',
            evidence_state: 'emulated',
            status_text: 'Detected serial port /dev/ttyUSB0 (controller unverified)',
            unsupported_reason: unsupportedReason,
          },
        ],
      },
      connectCandidate,
      bootstrapProfileFromCandidate,
      refreshDiscoveryState: vi.fn().mockResolvedValue(undefined),
      startDiscovery: vi.fn().mockResolvedValue(undefined),
      cancelDiscovery: vi.fn().mockResolvedValue(undefined),
      loading: false,
    });

    render(<DeviceSettingsDialog onClose={vi.fn()} />);
    fireEvent.click(screen.getByTestId('tab-discovery'));

    const connectButton = (await screen.findByRole('button', {
      name: 'Connect',
    })) as HTMLButtonElement;
    const bootstrapButton = screen.getByRole('button', {
      name: 'Bootstrap Profile',
    }) as HTMLButtonElement;
    const reason = screen.getByText(unsupportedReason);

    expect(connectButton.disabled).toBe(true);
    expect(connectButton.getAttribute('aria-describedby')).toBe(reason.id);
    expect(bootstrapButton.disabled).toBe(false);
    expect(screen.queryByText('0%')).toBeNull();

    fireEvent.click(connectButton);
    fireEvent.click(bootstrapButton);

    expect(connectCandidate).not.toHaveBeenCalled();
    await waitFor(() => {
      expect(bootstrapProfileFromCandidate).toHaveBeenCalledWith(
        'unknown-serial',
        'USB Serial Device',
      );
    });
  });
});
