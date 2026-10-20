import { describe, it, expect, afterEach, beforeEach, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { MachineProfileDialog } from '../MachineProfileDialog';
import { useMachineStore } from '../../../stores/machineStore';
import { useNotificationStore } from '../../../stores/notificationStore';
import { useAppStore } from '../../../stores/appStore';
import type { MachineProfile } from '../../../types/machine';
import { MachinePresetPanel } from '../MachinePresetPanel';
import { FEEDBACK_REPORT_OPEN_EVENT } from '../../../feedbackEvents';

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

  it.each([
    { presetId: 'sculpfun_s30_pro_10w', name: 'Sculpfun S30 Pro 10W', airOn: 'M8', width: 380, height: 385, homing: false, constantPower: false, usbEvidence: false },
    { presetId: 'sculpfun_s30_pro_max_20w', name: 'Sculpfun S30 Pro Max 20W', airOn: 'M8', width: 370, height: 360, homing: true, constantPower: false, usbEvidence: false },
    { presetId: 'sculpfun_s9', name: 'Sculpfun S9 (5.5W)', airOn: '', width: 410, height: 415, homing: false, constantPower: false, usbEvidence: false },
    { presetId: 'creality_falcon2_pro_22w', name: 'Creality Falcon2 Pro 22W', airOn: '', width: 400, height: 415, homing: false, constantPower: false, usbEvidence: false },
    { presetId: 'creality_falcon2_pro_60w', name: 'Creality Falcon2 Pro 60W', airOn: '', width: 400, height: 400, homing: false, constantPower: false, usbEvidence: false },
    { presetId: 'longer_ray5_10w', name: 'Longer RAY5 10W', airOn: '', width: 400, height: 400, homing: false, constantPower: false, usbEvidence: true },
    { presetId: 'longer_ray5_5w', name: 'Longer RAY5 5W', airOn: '', width: 400, height: 400, homing: false, constantPower: false, usbEvidence: true },
    { presetId: 'longer_ray5_20w', name: 'Longer RAY5 20W', airOn: '', width: 375, height: 365, homing: false, constantPower: false, usbEvidence: true },
    { presetId: 'longer_ray5_40w', name: 'Longer RAY5 40W', airOn: '', width: 385, height: 385, homing: false, constantPower: true, usbEvidence: true },
    { presetId: 'longer_b1_20w', name: 'Longer Laser B1 20W', airOn: 'M8', width: 450, height: 440, homing: true, constantPower: false, usbEvidence: true },
    { presetId: 'longer_b1_30w', name: 'Longer Laser B1 30W', airOn: 'M8', width: 450, height: 440, homing: true, constantPower: false, usbEvidence: true },
    { presetId: 'longer_b1_40w', name: 'Longer Laser B1 40W', airOn: 'M8', width: 450, height: 440, homing: true, constantPower: false, usbEvidence: true },
    { presetId: 'ortur_lm2_pro_s2', name: 'Ortur Laser Master 2 Pro S2 (LU2-2 / LU2-4)', airOn: '', width: 400, height: 400, homing: false, constantPower: false, usbEvidence: true },
    { presetId: 'ortur_lm3_10w', name: 'Ortur Laser Master 3 10W (LU2-10A)', airOn: '', width: 400, height: 400, homing: false, constantPower: false, usbEvidence: true },
    { presetId: 'ortur_lm3_20w', name: 'Ortur Laser Master 3 20W (LU3-20A)', airOn: '', width: 400, height: 380, homing: false, constantPower: false, usbEvidence: true },
    { presetId: 'ortur_lm3_40w', name: 'Ortur Laser Master 3 40W (LU3-40A)', airOn: '', width: 400, height: 380, homing: false, constantPower: false, usbEvidence: true },
    { presetId: 'ortur_lm3_le_10w', name: 'Ortur Laser Master 3 LE 10W (LU2-10A)', airOn: '', width: 400, height: 400, homing: false, constantPower: false, usbEvidence: true },
    { presetId: 'aufero_laser_1', name: 'Aufero Laser 1 (LU2-2 / LU2-4)', airOn: '', width: 180, height: 180, homing: false, constantPower: false, usbEvidence: true },
    { presetId: 'aufero_laser_2', name: 'Aufero Laser 2 (LU2-2 / LU2-4)', airOn: '', width: 390, height: 390, homing: false, constantPower: false, usbEvidence: true },
    { presetId: 'atomstack_a10_pro_v2', name: 'AtomStack A10 Pro V2', airOn: '', width: 400, height: 365, homing: false, constantPower: false, usbEvidence: true },
    { presetId: 'atomstack_a20_pro_v2', name: 'AtomStack A20 Pro V2', airOn: '', width: 400, height: 365, homing: false, constantPower: false, usbEvidence: true },
    { presetId: 'atomstack_a40_pro_v2', name: 'AtomStack A40 Pro V2', airOn: '', width: 385, height: 400, homing: false, constantPower: false, usbEvidence: true },
    { presetId: 'creality_falcon2_12w', name: 'Creality Falcon2 12W', airOn: '', width: 400, height: 415, homing: false, constantPower: false, usbEvidence: true },
    { presetId: 'creality_falcon2_22w', name: 'Creality Falcon2 22W', airOn: '', width: 400, height: 415, homing: false, constantPower: false, usbEvidence: true },
    { presetId: 'creality_falcon2_40w', name: 'Creality Falcon2 40W', airOn: '', width: 400, height: 415, homing: false, constantPower: false, usbEvidence: true },
    { presetId: 'sculpfun_s10', name: 'Sculpfun S10 10W', airOn: '', width: 385, height: 385, homing: false, constantPower: true, usbEvidence: true },
    { presetId: 'sculpfun_s30_5w', name: 'Sculpfun S30 5W', airOn: 'M8', width: 380, height: 385, homing: false, constantPower: false, usbEvidence: true },
    { presetId: 'sculpfun_s30_ultra_11w', name: 'Sculpfun S30 Ultra 11W', airOn: 'M8', width: 590, height: 595, homing: false, constantPower: false, usbEvidence: true },
    { presetId: 'sculpfun_s30_ultra_22w', name: 'Sculpfun S30 Ultra 22W', airOn: 'M8', width: 590, height: 595, homing: false, constantPower: false, usbEvidence: true },
    { presetId: 'sculpfun_s30_ultra_33w', name: 'Sculpfun S30 Ultra 33W', airOn: 'M8', width: 590, height: 595, homing: false, constantPower: false, usbEvidence: true },
    { presetId: 'twotrees_tts_55', name: 'Two Trees TTS-55 (original)', airOn: '', width: 300, height: 300, homing: false, constantPower: true, usbEvidence: true },
    { presetId: 'twotrees_tts_55_pro', name: 'Two Trees TTS-55 Pro', airOn: '', width: 300, height: 300, homing: false, constantPower: true, usbEvidence: true },
    { presetId: 'twotrees_tts_10_pro', name: 'Two Trees TTS-10 Pro', airOn: '', width: 300, height: 300, homing: false, constantPower: true, usbEvidence: true },
    { presetId: 'twotrees_tts_20_pro', name: 'Two Trees TTS-20 Pro', airOn: '', width: 418, height: 418, homing: false, constantPower: true, usbEvidence: true },
    { presetId: 'twotrees_ts2_10w', name: 'Two Trees TS2 10W', airOn: '', width: 450, height: 450, homing: false, constantPower: true, usbEvidence: true },
    { presetId: 'twotrees_ts2_20w', name: 'Two Trees TS2 20W', airOn: '', width: 410, height: 410, homing: false, constantPower: true, usbEvidence: true },
  ])('previews and applies $presetId with explicit confirmation', async ({ presetId, name, airOn, width, height, homing, constantPower, usbEvidence }) => {
    const profile = makeProfile({ id: 'prof-preset', name: 'Preset Profile' });
    const appliedProfile = {
      ...profile,
      preset_id: presetId,
      preset_version: 1,
      air_assist_on_gcode: airOn,
      air_assist_off_gcode: airOn ? 'M9' : '',
      bed_width_mm: width,
      bed_height_mm: height,
      homing_enabled: homing,
      use_constant_power: constantPower,
    };

    let storedProfile = profile;
    mockInvoke.mockImplementation((cmd: string, args?: unknown) => {
      if (cmd === 'get_machine_profile_presets') {
        return Promise.resolve([
          {
            id: presetId,
            version: 1,
            name,
            description: 'Documented stock defaults',
            advisory_text: 'Use a wall outlet if air assist resets the controller.',
            origin: 'bottom_left',
            firmware_type: 'grbl',
            default_baud_rate: 115200,
            bed_width_mm: width,
            bed_height_mm: height,
            max_speed_mm_min: 6000,
            max_power_percent: 100,
            s_value_max: 1000,
            homing_enabled: homing,
            use_constant_power: constantPower,
            emit_s_every_g1: false,
            use_g0_for_overscan: true,
            air_assist_on_gcode: airOn,
            air_assist_off_gcode: airOn ? 'M9' : '',
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
          presetId,
        });
        return Promise.resolve([{ field: 'air_assist_on_gcode', old: 'M7', new: airOn }]);
      }
      if (cmd === 'apply_machine_profile_preset') {
        storedProfile = appliedProfile;
        expect(args).toMatchObject({
          profileId: 'prof-preset',
          presetId,
          confirmDiff: true,
        });
        return Promise.resolve({
          applied: true,
          profile: appliedProfile,
          diff: [{ field: 'air_assist_on_gcode', old: 'M7', new: airOn }],
        });
      }
      return Promise.resolve([]);
    });

    const loadProfiles = vi.fn(async () => {
      useMachineStore.setState({ profiles: [storedProfile] });
    });
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
      expect(screen.getByRole('option', { name })).toBeDefined();
    });

    if (presetId === 'sculpfun_s9') {
      expect(screen.getByText(/Hardware testing is pending/)).toBeDefined();
      expect(screen.getByText(/Homing and automatic air assist are off/)).toBeDefined();
    }
    if (presetId === 'sculpfun_s30_pro_10w') {
      expect(screen.getByText(/Hardware testing is pending/)).toBeDefined();
      expect(screen.getByText(/380 x 385 mm workspace, S1000, M8\/M9 air assist/)).toBeDefined();
    }
    if (presetId.startsWith('creality_falcon2_pro_')) {
      expect(screen.getByText(/Physical testing pending/)).toBeDefined();
    }
    if (usbEvidence) {
      expect(screen.getByText(/Documentation-based GRBL 1.1 USB preset/)).toBeDefined();
      expect(screen.getByText(airOn !== ''
        ? /Automatic homing is off by default/
        : /Homing and automatic air assist are off/)).toBeDefined();
      if (constantPower) {
        expect(screen.getByText(/\. Constant Power \(M3\)\./)).toBeDefined();
      }
    }
    expect((screen.getByTestId('machine-preset-apply') as HTMLButtonElement).disabled).toBe(true);

    fireEvent.click(screen.getByTestId('machine-preset-preview'));

    await waitFor(() => {
      expect(screen.getByText('air_assist_on_gcode')).toBeDefined();
    });

    fireEvent.click(screen.getByTestId('machine-preset-apply'));

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('apply_machine_profile_preset', {
        profileId: 'prof-preset',
        presetId,
        confirmDiff: true,
      });
      expect((screen.getByLabelText('Air On G-code') as HTMLInputElement).value).toBe(airOn);
      expect((screen.getByLabelText('Width (mm)') as HTMLInputElement).value).toBe(String(width));
      expect((screen.getByLabelText('Height (mm)') as HTMLInputElement).value).toBe(String(height));
      expect((screen.getByText('Constant Power (M3)', { exact: true }).parentElement?.querySelector('input') as HTMLInputElement).checked).toBe(constantPower);
      expect(loadProfiles).toHaveBeenCalled();
    });
  });
});


describe('Machine preset community reports', () => {
  it('offers requests with unsaved profiles and only shares settings from a saved active profile', async () => {
    const profile = makeProfile({ name: 'Sculpfun S9' });
    useMachineStore.setState({ activeProfileId: null });
    const listener = vi.fn();
    window.addEventListener(FEEDBACK_REPORT_OPEN_EVENT, listener);
    try {
      const { rerender } = render(<MachinePresetPanel profile={profile} profileExists={false} dirty onApplied={vi.fn()} />);
      await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith('get_machine_profile_presets'));
      fireEvent.click(screen.getByRole('button', { name: 'Request a machine preset' }));
      expect(listener.mock.calls[0][0].detail).toEqual({
        kind: 'connectivity', presentation: 'machine_request',
        sourceContext: { source: 'machine_preset_panel', feature: 'machine_preset_request' },
      });
      const share = () => screen.getByRole('button', { name: 'Share machine test results' }) as HTMLButtonElement;
      expect(share().disabled).toBe(true);
      rerender(<MachinePresetPanel profile={profile} profileExists dirty={false} onApplied={vi.fn()} />);
      expect(share().disabled).toBe(true);
      act(() => useMachineStore.setState({ activeProfileId: profile.id }));
      expect(share().disabled).toBe(false);
      fireEvent.click(share());
      expect(listener.mock.calls[1][0].detail).toEqual({
        kind: 'connectivity', presentation: 'machine_test', title: 'Sculpfun S9',
        sourceContext: { source: 'machine_preset_panel', feature: 'machine_test_report' },
      });
      rerender(<MachinePresetPanel profile={profile} profileExists dirty onApplied={vi.fn()} />);
      expect(share().disabled).toBe(true);
    } finally {
      window.removeEventListener(FEEDBACK_REPORT_OPEN_EVENT, listener);
    }
  });
});
