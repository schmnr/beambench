import { invoke } from '@tauri-apps/api/core';
import { open, save } from '@tauri-apps/plugin-dialog';
import i18n from '../i18n';
import type {
  PortInfo,
  SessionState,
  MachineStatus,
  MachineRuntimeState,
  PreflightReport,
  JobProgress,
  MachineProfile,
  MachineProfilePreset,
  ProfileFieldDiff,
  ApplyPresetResult,
  PresetSuggestion,
  FrameMode,
  OverrideAction,
  ControllerConnectionResult,
  ControllerMismatchDecision,
  ControllerSelection,
  LihuiyuUsbDeviceInfo,
} from '../types/machine';
import type { SessionJobOptions } from '../types/jobOptions';
import type { ConsoleEntry } from '../types/console';
import type { SavedPosition } from '../types/commands';

export interface LaserFireStartResult {
  token: string;
  power_percent: number;
  s_value: number;
  keepalive_interval_ms: number;
  max_duration_ms: number;
}

const MAX_GRBL_SETTING_ID = 0xffff;
const MACHINE_PROFILE_EXTENSION = 'bbprofile';

function machineProfileDefaultPath(profileName: string): string {
  const safeName = profileName
    .trim()
    .replace(/[<>:"/\\|?*]/g, '_')
    .replace(/[. ]+$/g, '');
  return `${safeName || 'machine-profile'}.${MACHINE_PROFILE_EXTENSION}`;
}

function ensureMachineProfileExtension(path: string): string {
  return path.toLowerCase().endsWith(`.${MACHINE_PROFILE_EXTENSION}`)
    ? path
    : `${path}.${MACHINE_PROFILE_EXTENSION}`;
}

function validateGrblSettingWrite(key: number, value: number): void {
  if (!Number.isInteger(key) || key < 0 || key > MAX_GRBL_SETTING_ID) {
    throw new RangeError(`GRBL setting ID must be an integer from 0 to ${MAX_GRBL_SETTING_ID}`);
  }
  if (!Number.isFinite(value)) {
    throw new RangeError('GRBL setting value must be a finite number');
  }
}

export const machineService = {
  async listSerialPorts(): Promise<PortInfo[]> {
    return invoke<PortInfo[]>('list_serial_ports');
  },

  async connect(portName: string, baudRate?: number): Promise<SessionState> {
    return invoke<SessionState>('connect_machine', { portName, baudRate });
  },

  async beginControllerConnection(
    portName: string,
    baudRate: number | undefined,
    selection: ControllerSelection,
  ): Promise<ControllerConnectionResult> {
    return invoke<ControllerConnectionResult>('begin_controller_connection', {
      portName,
      baudRate,
      selection,
    });
  },

  async beginNetworkControllerConnection(
    host: string,
    port: number,
    selection: ControllerSelection,
  ): Promise<ControllerConnectionResult> {
    return invoke<ControllerConnectionResult>('begin_network_controller_connection', {
      host,
      port,
      selection,
    });
  },

  async listLihuiyuUsbDevices(): Promise<LihuiyuUsbDeviceInfo[]> {
    return invoke<LihuiyuUsbDeviceInfo[]>('list_lihuiyu_usb_devices');
  },

  async beginUsbControllerConnection(
    device: LihuiyuUsbDeviceInfo,
    selection: ControllerSelection,
  ): Promise<ControllerConnectionResult> {
    return invoke<ControllerConnectionResult>('begin_usb_controller_connection', {
      busId: device.bus_id,
      deviceAddress: device.device_address,
      portNumbers: device.port_numbers,
      selection,
    });
  },

  async continueControllerConnection(
    attemptId: string,
    selection: ControllerSelection,
    decision?: ControllerMismatchDecision,
  ): Promise<ControllerConnectionResult> {
    return invoke<ControllerConnectionResult>('continue_controller_connection', {
      attemptId,
      selection,
      decision,
    });
  },

  async disconnect(): Promise<void> {
    return invoke<void>('disconnect_machine');
  },

  async getMachineStatus(): Promise<MachineStatus> {
    return invoke<MachineStatus>('get_machine_status');
  },

  async getMachineCoordinatesValid(): Promise<boolean> {
    return invoke<boolean>('get_machine_coordinates_valid');
  },

  async getMachineRuntimeState(): Promise<MachineRuntimeState> {
    return invoke<MachineRuntimeState>('get_machine_runtime_state');
  },

  async getSessionState(): Promise<SessionState> {
    return invoke<SessionState>('get_session_state');
  },

  async home(): Promise<void> {
    return invoke<void>('machine_home');
  },

  async unlock(): Promise<void> {
    return invoke<void>('machine_unlock');
  },

  async jog(
    xMm: number,
    yMm: number,
    feedRate: number,
    zMm?: number | null,
    continuous = false
  ): Promise<void> {
    return invoke<void>('machine_jog', { xMm, yMm, zMm, feedRate, continuous });
  },

  async jogCancel(): Promise<void> {
    return invoke<void>('machine_jog_cancel');
  },

  async runPreflightCheck(jobOptions?: SessionJobOptions): Promise<PreflightReport> {
    return invoke<PreflightReport>('run_preflight_check', { jobOptions });
  },

  async startJob(jobOptions?: SessionJobOptions): Promise<JobProgress> {
    return invoke<JobProgress>('start_job', { jobOptions });
  },

  async getJobProgress(): Promise<JobProgress | null> {
    return invoke<JobProgress | null>('get_job_progress');
  },

  async pauseJob(): Promise<void> {
    return invoke<void>('pause_job');
  },

  async resumeJob(): Promise<void> {
    return invoke<void>('resume_job');
  },

  async cancelJob(): Promise<void> {
    return invoke<void>('cancel_job');
  },

  async getMachineProfiles(): Promise<MachineProfile[]> {
    return invoke<MachineProfile[]>('get_machine_profiles');
  },

  async saveMachineProfile(profile: MachineProfile): Promise<MachineProfile> {
    return invoke<MachineProfile>('save_machine_profile', { profile });
  },

  async pickMachineProfileImportPath(): Promise<string | null> {
    const selected = await open({
      title: `${i18n.t('menus.file.import')} ${i18n.t('dialog.device_settings.profile')}`,
      multiple: false,
      directory: false,
      filters: [{
        name: i18n.t('dialog.device_settings.profile'),
        extensions: [MACHINE_PROFILE_EXTENSION],
      }],
    });
    return typeof selected === 'string' ? selected : null;
  },

  async pickMachineProfileExportPath(profileName: string): Promise<string | null> {
    const selected = await save({
      title: `${i18n.t('menus.file.export')} ${i18n.t('dialog.device_settings.profile')}`,
      defaultPath: machineProfileDefaultPath(profileName),
      filters: [{
        name: i18n.t('dialog.device_settings.profile'),
        extensions: [MACHINE_PROFILE_EXTENSION],
      }],
    });
    return selected === null ? null : ensureMachineProfileExtension(selected);
  },

  async importMachineProfile(path: string): Promise<MachineProfile> {
    return invoke<MachineProfile>('import_machine_profile', { path });
  },

  async exportMachineProfile(profileId: string, path: string): Promise<void> {
    return invoke<void>('export_machine_profile', { profileId, path });
  },

  async deleteMachineProfile(profileId: string): Promise<void> {
    return invoke<void>('delete_machine_profile', { profileId });
  },

  async getMachineProfilePresets(): Promise<MachineProfilePreset[]> {
    return invoke<MachineProfilePreset[]>('get_machine_profile_presets');
  },

  async suggestMachineProfilePreset(): Promise<PresetSuggestion> {
    return invoke<PresetSuggestion>('suggest_machine_profile_preset');
  },

  async getMachineProfilePresetDiff(
    profileId: string,
    presetId: string
  ): Promise<ProfileFieldDiff[]> {
    return invoke<ProfileFieldDiff[]>('get_machine_profile_preset_diff', { profileId, presetId });
  },

  async applyMachineProfilePreset(
    profileId: string,
    presetId: string,
    confirmDiff: boolean
  ): Promise<ApplyPresetResult> {
    return invoke<ApplyPresetResult>('apply_machine_profile_preset', {
      profileId,
      presetId,
      confirmDiff,
    });
  },

  async setActiveProfile(profileId: string | null): Promise<void> {
    return invoke<void>('set_active_profile', { profileId });
  },

  async frameJob(
    frameMode?: FrameMode,
    selectedObjectIds?: string[],
    laserOnOverride = false,
    feedRate?: number
  ): Promise<JobProgress> {
    return invoke<JobProgress>('frame_job', { frameMode, selectedObjectIds, laserOnOverride, feedRate });
  },

  async setFeedOverride(action: OverrideAction): Promise<void> {
    return invoke<void>('set_feed_override', { action });
  },

  async setSpindleOverride(action: OverrideAction): Promise<void> {
    return invoke<void>('set_spindle_override', { action });
  },

  async resetAllOverrides(): Promise<void> {
    return invoke<void>('reset_all_overrides');
  },

  async emergencyStop(): Promise<void> {
    return invoke<void>('emergency_stop');
  },

  async setWorkOrigin(): Promise<[number, number]> {
    return invoke<[number, number]>('set_work_origin');
  },

  async resetWorkOrigin(): Promise<void> {
    return invoke<void>('reset_work_origin');
  },

  async sendGcodeLine(line: string): Promise<void> {
    return invoke<void>('send_gcode_line', { line });
  },

  async getConsoleLog(): Promise<ConsoleEntry[]> {
    return invoke<ConsoleEntry[]>('get_console_log', { limit: 200 });
  },

  async clearConsoleLog(): Promise<void> {
    return invoke<void>('clear_console_log');
  },

  async getControllerInfo(): Promise<Record<string, string>> {
    return invoke<Record<string, string>>('get_controller_info');
  },

  async moveLaserTo(x: number, y: number, feedRate: number, z?: number | null): Promise<void> {
    return invoke<void>('move_laser_to', { x, y, z, feedRate });
  },

  async moveLaserToMachine(x: number, y: number, feedRate: number, z?: number | null): Promise<void> {
    return invoke<void>('move_laser_to_machine', { x, y, z, feedRate });
  },

  async laserFireStart(powerPercent?: number): Promise<LaserFireStartResult> {
    return invoke<LaserFireStartResult>('laser_fire_start', { powerPercent });
  },

  async laserFireKeepalive(token: string): Promise<void> {
    return invoke<void>('laser_fire_keepalive', { token });
  },

  async laserFireStop(token: string): Promise<void> {
    return invoke<void>('laser_fire_stop', { token });
  },

  async getSavedPositions(): Promise<SavedPosition[]> {
    return invoke<SavedPosition[]>('get_saved_positions');
  },

  async savePosition(name: string, x: number, y: number, z?: number | null): Promise<SavedPosition[]> {
    return invoke<SavedPosition[]>('save_position', { name, x, y, z });
  },

  async deleteSavedPosition(id: string): Promise<SavedPosition[]> {
    return invoke<SavedPosition[]>('delete_saved_position', { id });
  },

  async getGrblSettings(): Promise<Record<string, string>> {
    return invoke<Record<string, string>>('get_grbl_settings');
  },

  async setGrblSetting(key: number, value: number): Promise<void> {
    validateGrblSettingWrite(key, value);
    return invoke<void>('set_grbl_setting', { key, value });
  },
};
