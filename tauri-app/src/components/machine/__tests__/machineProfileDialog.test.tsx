import { describe, it, expect, afterEach, beforeEach, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { MachineProfileDialog } from '../MachineProfileDialog';
import { useMachineStore } from '../../../stores/machineStore';
import { useNotificationStore } from '../../../stores/notificationStore';
import { useAppStore } from '../../../stores/appStore';
import type { MachineProfile } from '../../../types/machine';

const mockInvoke = vi.fn().mockResolvedValue([]);
const mockOpen = vi.fn().mockResolvedValue(null);
const mockSave = vi.fn().mockResolvedValue(null);
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => mockInvoke(...args) }));
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
    name: 'Dialog Profile',
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
  mockInvoke.mockResolvedValue([]);
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
});

describe('MachineProfileDialog', () => {
  it('offers Ruida Z and U lift-table channels without enabling automated Z moves', async () => {
    const profile = makeProfile({
      firmware_type: 'ruida',
      supports_z_moves: true,
      ruida_table_axis: 'disabled',
    });
    const saveProfile = vi.fn().mockResolvedValue(undefined);
    useMachineStore.setState({
      profiles: [profile],
      activeProfileId: profile.id,
      loadProfiles: vi.fn().mockResolvedValue(undefined),
      saveProfile,
      deleteProfile: vi.fn().mockResolvedValue(undefined),
      setActiveProfile: vi.fn().mockResolvedValue(undefined),
    });

    render(<MachineProfileDialog onClose={vi.fn()} />);
    fireEvent.click(screen.getByText('Dialog Profile'));
    fireEvent.change(screen.getByLabelText('Ruida lift table axis'), { target: { value: 'u' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    await waitFor(() => {
      expect(saveProfile).toHaveBeenCalledWith(expect.objectContaining({
        ruida_table_axis: 'u',
        supports_z_moves: false,
      }));
    });
    expect(screen.getByLabelText('Lift table feed (mm/min)')).toBeDefined();
    expect(screen.queryByText('Supports Z moves')).toBeNull();
  });

  it('imports a profile with no selection, refreshes and selects it without activating it', async () => {
    const importedProfile = makeProfile({ id: 'imported-1', name: 'Shared Profile' });
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
    mockOpen.mockResolvedValue('/tmp/shared.bbprofile');
    mockInvoke.mockImplementation((command: string) =>
      command === 'import_machine_profile' ? Promise.resolve(importedProfile) : Promise.resolve([]),
    );

    render(<MachineProfileDialog onClose={vi.fn()} />);

    expect((screen.getByRole('button', { name: 'Export' }) as HTMLButtonElement).disabled).toBe(
      true,
    );
    fireEvent.click(screen.getByRole('button', { name: 'Import' }));

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('import_machine_profile', {
        path: '/tmp/shared.bbprofile',
      });
      expect((screen.getByLabelText('Name') as HTMLInputElement).value).toBe('Shared Profile');
    });
    expect(loadProfiles).toHaveBeenCalledTimes(2);
    expect(setActiveProfile).not.toHaveBeenCalled();
  });

  it('exports only the selected saved profile and disables export for dirty edits', async () => {
    const profile = makeProfile({ id: 'saved-1', name: 'Saved/Profile' });
    useMachineStore.setState({
      profiles: [profile],
      activeProfileId: null,
      loadProfiles: vi.fn().mockResolvedValue(undefined),
      saveProfile: vi.fn().mockResolvedValue(undefined),
      deleteProfile: vi.fn().mockResolvedValue(undefined),
      setActiveProfile: vi.fn().mockResolvedValue(undefined),
    });
    mockSave.mockResolvedValue('/tmp/exported-profile');
    mockInvoke.mockResolvedValue(undefined);

    render(<MachineProfileDialog onClose={vi.fn()} />);
    fireEvent.click(screen.getByText('Saved/Profile'));

    const exportButton = screen.getByRole('button', { name: 'Export' }) as HTMLButtonElement;
    expect(exportButton.disabled).toBe(false);
    fireEvent.click(exportButton);

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('export_machine_profile', {
        profileId: 'saved-1',
        path: '/tmp/exported-profile.bbprofile',
      });
    });

    fireEvent.change(screen.getByLabelText('Name'), { target: { value: 'Unsaved Name' } });
    expect(exportButton.disabled).toBe(true);
    expect(mockSave).toHaveBeenCalledTimes(1);
  });

  it('holds a dirty import behind the existing discard confirmation', async () => {
    const profile = makeProfile({ id: 'saved-2', name: 'Current Profile' });
    const importedProfile = makeProfile({ id: 'imported-2', name: 'Imported After Discard' });
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
    mockOpen.mockResolvedValue('/tmp/replacement.bbprofile');
    mockInvoke.mockImplementation((command: string) =>
      command === 'import_machine_profile' ? Promise.resolve(importedProfile) : Promise.resolve([]),
    );

    render(<MachineProfileDialog onClose={vi.fn()} />);
    fireEvent.click(screen.getByText('Current Profile'));
    fireEvent.change(screen.getByLabelText('Name'), { target: { value: 'Unsaved Name' } });
    fireEvent.click(screen.getByRole('button', { name: 'Import' }));

    await waitFor(() => {
      expect(screen.getByText('Discard unsaved machine profile changes?')).toBeDefined();
    });
    expect(mockInvoke.mock.calls.some(([command]) => command === 'import_machine_profile')).toBe(
      false,
    );
    expect((screen.getByLabelText('Name') as HTMLInputElement).value).toBe('Unsaved Name');

    fireEvent.click(screen.getByRole('button', { name: 'Discard' }));

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('import_machine_profile', {
        path: '/tmp/replacement.bbprofile',
      });
      expect((screen.getByLabelText('Name') as HTMLInputElement).value).toBe(
        'Imported After Discard',
      );
    });
    expect(setActiveProfile).not.toHaveBeenCalled();
  });

  it('converts max speed value and bounds to display units in inches mode', async () => {
    useMachineStore.setState({
      profiles: [makeProfile()],
      activeProfileId: null,
      loadProfiles: vi.fn().mockResolvedValue(undefined),
      saveProfile: vi.fn().mockResolvedValue(undefined),
      deleteProfile: vi.fn().mockResolvedValue(undefined),
      setActiveProfile: vi.fn().mockResolvedValue(undefined),
    });
    useAppStore.setState({
      settings: { display_unit: 'inches', speed_time_unit: 'minutes' } as never,
    });

    render(<MachineProfileDialog onClose={vi.fn()} />);
    fireEvent.click(screen.getByText('Dialog Profile'));

    const maxSpeed = (await screen.findByLabelText('Max Speed (in/min)')) as HTMLInputElement;
    // 3000 mm/min -> 118.1 in/min
    expect(maxSpeed.value).toBe('118.1');
    // Bounds must be converted too: 100 mm/min -> 3.9 in/min, 50000 mm/min -> 1968.5 in/min.
    // Raw mm/min bounds would clamp typed inch values incorrectly.
    expect(maxSpeed.min).toBe('3.9');
    expect(maxSpeed.max).toBe('1968.5');
  });

  it('renders output policy and calibration controls for saved profiles', async () => {
    const profile = makeProfile({
      use_constant_power: true,
      emit_s_every_g1: true,
      use_g0_for_overscan: true,
      enable_dot_width: true,
      dot_width_mm: 0.1,
      enable_scanning_offset: true,
      scanning_offsets: [{ speed_mm_min: 1000, offset_mm: 0.1 }],
    });

    useMachineStore.setState({
      profiles: [profile],
      activeProfileId: null,
      loadProfiles: vi.fn().mockResolvedValue(undefined),
      saveProfile: vi.fn().mockResolvedValue(undefined),
      deleteProfile: vi.fn().mockResolvedValue(undefined),
      setActiveProfile: vi.fn().mockResolvedValue(undefined),
    });

    render(<MachineProfileDialog onClose={vi.fn()} />);
    fireEvent.click(screen.getByText('Dialog Profile'));

    await waitFor(() => {
      expect(screen.getByText('Output Policy')).toBeDefined();
    });

    expect(screen.getByText('Constant Power (M3)')).toBeDefined();
    expect(screen.getByText('Emit S on Every G1')).toBeDefined();
    expect(screen.getByText('Use G0 for Overscan')).toBeDefined();
    expect(screen.getByText('Origin')).toBeDefined();
    expect(screen.getByText('Camera Metadata')).toBeDefined();
    expect(screen.getAllByText('Calibration').length).toBeGreaterThan(0);
    const scrollRegion = screen.getByTestId('machine-profile-editor-scroll-region');
    expect(scrollRegion.classList.contains('scrollbar-safe-edge')).toBe(true);
    const dotWidthToggle = screen.getByRole('checkbox', { name: 'Enable Dot Width Correction' });
    expect((dotWidthToggle as HTMLInputElement).checked).toBe(true);
    expect(screen.getByText('Dot Width (mm)')).toBeDefined();
    expect(screen.getByText('Enable Scanning Offset')).toBeDefined();
    expect(screen.getByText('Add Entry')).toBeDefined();

    fireEvent.click(dotWidthToggle.closest('label')!);

    expect((dotWidthToggle as HTMLInputElement).checked).toBe(false);
    expect(screen.queryByText('Dot Width (mm)')).toBeNull();
  });

  it('edits and saves multiline start and end G-code', async () => {
    const saveProfile = vi.fn().mockResolvedValue(undefined);
    const profile = makeProfile({
      job_header_gcode: 'T10M6\nG54',
      job_footer_gcode: 'M5\nM30',
    });

    useMachineStore.setState({
      profiles: [profile],
      activeProfileId: null,
      loadProfiles: vi.fn().mockResolvedValue(undefined),
      saveProfile,
      deleteProfile: vi.fn().mockResolvedValue(undefined),
      setActiveProfile: vi.fn().mockResolvedValue(undefined),
    });

    render(<MachineProfileDialog onClose={vi.fn()} />);
    fireEvent.click(screen.getByText('Dialog Profile'));

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

  it('preserves dirty editor state when profile data refreshes in the background', async () => {
    const profile = makeProfile({
      id: 'prof-2',
      name: 'Sync Profile',
      enable_scanning_offset: true,
      scanning_offsets: [
        { speed_mm_min: 3000, offset_mm: 0.3 },
        { speed_mm_min: 1000, offset_mm: 0.1 },
      ],
    });

    useMachineStore.setState({
      profiles: [profile],
      activeProfileId: null,
      loadProfiles: vi.fn().mockResolvedValue(undefined),
      saveProfile: vi.fn().mockResolvedValue(undefined),
      deleteProfile: vi.fn().mockResolvedValue(undefined),
      setActiveProfile: vi.fn().mockResolvedValue(undefined),
    });

    render(<MachineProfileDialog onClose={vi.fn()} />);
    fireEvent.click(screen.getByText('Sync Profile'));

    await waitFor(() => {
      expect(screen.getByTestId('machine-profile-scanning-offset-table')).toBeDefined();
    });

    fireEvent.change(screen.getByLabelText('Name'), { target: { value: 'Dirty Name' } });

    await act(async () => {
      useMachineStore.setState({
        profiles: [
          {
            ...profile,
            scanning_offsets: [
              { speed_mm_min: 1000, offset_mm: 0.1 },
              { speed_mm_min: 3000, offset_mm: 0.3 },
            ],
          },
        ],
      });
    });

    expect((screen.getByLabelText('Name') as HTMLInputElement).value).toBe('Dirty Name');
    const inputs = within(screen.getByTestId('machine-profile-scanning-offset-table')).getAllByRole(
      'spinbutton',
    );
    expect((inputs[0] as HTMLInputElement).value).toBe('3000');
  });

  it('keeps the selected profile open when delete fails', async () => {
    const profile = makeProfile({ id: 'prof-3', name: 'Delete Me' });

    useMachineStore.setState({
      profiles: [profile],
      activeProfileId: null,
      loadProfiles: vi.fn().mockResolvedValue(undefined),
      saveProfile: vi.fn().mockResolvedValue(undefined),
      deleteProfile: vi.fn().mockRejectedValue(new Error('delete failed')),
      setActiveProfile: vi.fn().mockResolvedValue(undefined),
    });

    render(<MachineProfileDialog onClose={vi.fn()} />);
    fireEvent.click(screen.getByText('Delete Me'));

    await waitFor(() => {
      expect((screen.getByLabelText('Name') as HTMLInputElement).value).toBe('Delete Me');
    });

    fireEvent.click(screen.getByText('Delete'));

    await waitFor(() => {
      expect((screen.getByLabelText('Name') as HTMLInputElement).value).toBe('Delete Me');
    });
  });

  it('guards switching profiles when the current edit is dirty', async () => {
    const profiles = [
      makeProfile({ id: 'prof-1', name: 'Profile One' }),
      makeProfile({ id: 'prof-2', name: 'Profile Two', bed_width_mm: 500 }),
    ];

    useMachineStore.setState({
      profiles,
      activeProfileId: null,
      loadProfiles: vi.fn().mockResolvedValue(undefined),
      saveProfile: vi.fn().mockResolvedValue(undefined),
      deleteProfile: vi.fn().mockResolvedValue(undefined),
      setActiveProfile: vi.fn().mockResolvedValue(undefined),
    });

    render(<MachineProfileDialog onClose={vi.fn()} />);
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
  });

  it('prompts to save dirty edits before closing', async () => {
    const profile = makeProfile({ id: 'prof-close', name: 'Close Profile' });
    const onClose = vi.fn();
    const saveProfile = vi.fn().mockResolvedValue(undefined);

    useMachineStore.setState({
      profiles: [profile],
      activeProfileId: null,
      loadProfiles: vi.fn().mockResolvedValue(undefined),
      saveProfile,
      deleteProfile: vi.fn().mockResolvedValue(undefined),
      setActiveProfile: vi.fn().mockResolvedValue(undefined),
    });

    render(<MachineProfileDialog onClose={onClose} />);
    fireEvent.click(screen.getByText('Close Profile'));

    await waitFor(() => {
      expect((screen.getByLabelText('Name') as HTMLInputElement).value).toBe('Close Profile');
    });

    fireEvent.change(screen.getByLabelText('Name'), { target: { value: 'Saved Name' } });
    fireEvent.click(screen.getByRole('button', { name: 'Close' }));

    expect(screen.getByText('Save machine profile changes before closing?')).toBeDefined();
    expect(onClose).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole('button', { name: 'Save & Close' }));

    await waitFor(() => {
      expect(saveProfile).toHaveBeenCalledWith(expect.objectContaining({ name: 'Saved Name' }));
      expect(onClose).toHaveBeenCalledTimes(1);
    });
  });

  it('defaults new profiles to bottom-left origin', async () => {
    useMachineStore.setState({
      profiles: [],
      activeProfileId: null,
      loadProfiles: vi.fn().mockResolvedValue(undefined),
      saveProfile: vi.fn().mockResolvedValue(undefined),
      deleteProfile: vi.fn().mockResolvedValue(undefined),
      setActiveProfile: vi.fn().mockResolvedValue(undefined),
    });

    render(<MachineProfileDialog onClose={vi.fn()} />);
    fireEvent.click(screen.getByText('New'));

    await waitFor(() => {
      expect((screen.getByLabelText('Origin') as HTMLSelectElement).value).toBe('bottom_left');
    });
    expect((screen.getByRole('button', { name: 'Export' }) as HTMLButtonElement).disabled).toBe(
      true,
    );
  });

  it('saves a new profile before activating it and keeps delete disabled', async () => {
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

    render(<MachineProfileDialog onClose={vi.fn()} />);
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

  it('does not activate a new profile when saving fails', async () => {
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

    render(<MachineProfileDialog onClose={vi.fn()} />);
    fireEvent.click(screen.getByText('New'));
    fireEvent.click(await screen.findByRole('button', { name: 'Set Active' }));

    await waitFor(() => {
      expect(saveProfile).toHaveBeenCalledTimes(1);
    });
    expect(setActiveProfile).not.toHaveBeenCalled();
    expect(screen.getByLabelText('Name')).toBeDefined();
  });

  it('previews and applies machine presets with explicit confirmation', async () => {
    const profile = makeProfile({ id: 'prof-preset', name: 'Preset Profile' });
    const appliedProfile = {
      ...profile,
      preset_id: 'sculpfun_s30_pro_max_20w',
      preset_version: 1,
      air_assist_on_gcode: 'M8',
    };

    mockInvoke.mockImplementation((cmd: string, args?: unknown) => {
      if (cmd === 'get_machine_profile_presets') {
        return Promise.resolve([
          {
            id: 'sculpfun_s30_pro_max_20w',
            version: 1,
            name: 'Sculpfun S30 Pro Max 20W',
            description: 'Sculpfun defaults',
            advisory_text: 'Use a wall outlet if air assist resets the controller.',
            origin: 'bottom_left',
            firmware_type: 'grbl',
            default_baud_rate: 115200,
            bed_width_mm: 370,
            bed_height_mm: 360,
            max_speed_mm_min: 6000,
            max_power_percent: 100,
            s_value_max: 1000,
            homing_enabled: true,
            use_constant_power: false,
            emit_s_every_g1: false,
            use_g0_for_overscan: true,
            air_assist_on_gcode: 'M8',
            air_assist_off_gcode: 'M9',
            air_assist_on_delay_ms: 0,
            job_header_gcode: '',
            job_footer_gcode: '',
            transfer_mode: 'buffered',
            preferred_default_origin: 'bottom_left',
          },
        ]);
      }
      if (cmd === 'get_machine_profile_preset_diff') {
        expect(args).toMatchObject({
          profileId: 'prof-preset',
          presetId: 'sculpfun_s30_pro_max_20w',
        });
        return Promise.resolve([{ field: 'air_assist_on_gcode', old: 'M7', new: 'M8' }]);
      }
      if (cmd === 'apply_machine_profile_preset') {
        expect(args).toMatchObject({
          profileId: 'prof-preset',
          presetId: 'sculpfun_s30_pro_max_20w',
          confirmDiff: true,
        });
        return Promise.resolve({
          applied: true,
          profile: appliedProfile,
          diff: [{ field: 'air_assist_on_gcode', old: 'M7', new: 'M8' }],
        });
      }
      return Promise.resolve([]);
    });

    const loadProfiles = vi.fn().mockResolvedValue(undefined);
    useMachineStore.setState({
      profiles: [profile],
      activeProfileId: null,
      loadProfiles,
      saveProfile: vi.fn().mockResolvedValue(undefined),
      deleteProfile: vi.fn().mockResolvedValue(undefined),
      setActiveProfile: vi.fn().mockResolvedValue(undefined),
    });

    render(<MachineProfileDialog onClose={vi.fn()} />);
    fireEvent.click(screen.getByText('Preset Profile'));

    await waitFor(() => {
      expect(screen.getByTestId('machine-preset-select')).toBeDefined();
    });

    fireEvent.click(screen.getByTestId('machine-preset-preview'));

    await waitFor(() => {
      expect(screen.getByText('air_assist_on_gcode')).toBeDefined();
    });

    fireEvent.click(screen.getByTestId('machine-preset-apply'));

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('apply_machine_profile_preset', {
        profileId: 'prof-preset',
        presetId: 'sculpfun_s30_pro_max_20w',
        confirmDiff: true,
      });
      expect((screen.getByLabelText('Air On G-code') as HTMLInputElement).value).toBe('M8');
      expect(loadProfiles).toHaveBeenCalled();
    });
  });
});
