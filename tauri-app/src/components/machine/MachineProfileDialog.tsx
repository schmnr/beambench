import { useCallback, useEffect, useState } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import { useMachineStore } from '../../stores/machineStore';
import { useNotificationStore } from '../../stores/notificationStore';
import { machineService } from '../../services/machineService';
import { DEFAULT_QUALITY_TEST_SETTINGS } from '../../types/machine';
import type { MachineProfile, ScanningOffsetEntry } from '../../types/machine';
import { TextInput } from '../shared/TextInput';
import { TextArea } from '../shared/TextArea';
import { NumberInput } from '../shared/NumberInput';
import { Toggle } from '../shared/Toggle';
import { Select } from '../shared/Select';
import { NumberStepper } from '../shared/NumberStepper';
import { MovableResizableDialogFrame } from '../shared/MovableResizableDialogFrame';
import { MachinePresetPanel } from './MachinePresetPanel';
import { mmToDisplay, displayToMm, roundDisplayLength, lengthStep, lengthUnitLabel, labelWithUnit } from '../../utils/lengthUnits';
import { speedInputValue, displaySpeedToMmMin, speedStepForUnit, speedUnitLabel } from '../../utils/speedUnits';
import { useAppStore } from '../../stores/appStore';
import { SERIAL_BAUD_RATE_OPTIONS } from '../../constants/serial';

interface MachineProfileDialogProps {
  onClose: () => void;
}
type PendingProfileAction =
  | { type: 'new' }
  | { type: 'select'; profile: MachineProfile }
  | { type: 'import'; path: string };

const START_END_GCODE_HELP_ID = 'machine-profile-start-end-gcode-help';

function cloneProfile(profile: MachineProfile): MachineProfile {
  return {
    ...profile,
    ...(profile.scanning_offsets
      ? { scanning_offsets: profile.scanning_offsets.map((entry) => ({ ...entry })) }
      : {}),
  };
}

function sameProfileSnapshot(a: MachineProfile, b: MachineProfile): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

function formatErrorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export function MachineProfileDialog({ onClose }: MachineProfileDialogProps) {
  const { t } = useTranslation();
  const displayUnit = useAppStore((s) => s.settings?.display_unit) ?? 'mm';
  const speedTimeUnit = useAppStore((s) => s.settings?.speed_time_unit) ?? 'minutes';
  const profiles = useMachineStore((s) => s.profiles);
  const activeProfileId = useMachineStore((s) => s.activeProfileId);
  const loadProfiles = useMachineStore((s) => s.loadProfiles);
  const saveProfile = useMachineStore((s) => s.saveProfile);
  const deleteProfile = useMachineStore((s) => s.deleteProfile);
  const setActiveProfile = useMachineStore((s) => s.setActiveProfile);

  const [editingProfile, setEditingProfile] = useState<MachineProfile | null>(null);
  const [isDirty, setIsDirty] = useState(false);
  const [pendingDiscardAction, setPendingDiscardAction] = useState<PendingProfileAction | null>(null);
  const [closePromptVisible, setClosePromptVisible] = useState(false);
  const editingProfileExists = editingProfile !== null
    && profiles.some((profile) => profile.id === editingProfile.id);

  useEffect(() => {
    loadProfiles();
  }, [loadProfiles]);

  useEffect(() => {
    if (!editingProfile) return;
    const refreshed = profiles.find((profile) => profile.id === editingProfile.id);
    if (!refreshed) {
      if (!isDirty) {
        setEditingProfile(null);
      }
      return;
    }
    if (!isDirty && !sameProfileSnapshot(editingProfile, refreshed)) {
      setEditingProfile(cloneProfile(refreshed));
    }
  }, [profiles, editingProfile, isDirty]);

  const createNewProfile = () => {
    const newProfile: MachineProfile = {
      id: crypto.randomUUID(),
      name: t('dialog.machine_profile.new_profile'),
      preset_id: null,
      preset_version: null,
      bed_width_mm: 400,
      bed_height_mm: 400,
      max_speed_mm_min: 3000,
      max_power_percent: 100,
      s_value_max: 1000,
      homing_enabled: true,
      default_baud_rate: 115200,
      firmware_type: 'GRBL',
      notes: '',
      origin: 'bottom_left',
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
      air_assist_on_gcode: 'M7',
      air_assist_off_gcode: 'M9',
      air_assist_on_delay_ms: 0,
      job_header_gcode: '',
      job_footer_gcode: '',
      transfer_mode: 'buffered',
      preferred_default_origin: null,
      enable_dot_width: false,
      dot_width_mm: 0,
      enable_scanning_offset: false,
      scanning_offsets: [],
      supports_z_moves: false,
      z_move_feed_mm_min: 300,
      ruida_table_axis: 'disabled',
      enable_laser_fire_button: false,
      default_fire_power_percent: 1,
      quality_test_settings: DEFAULT_QUALITY_TEST_SETTINGS,
    };
    setEditingProfile(newProfile);
    setIsDirty(true);
  };

  const importProfileFromPath = async (path: string) => {
    try {
      const importedProfile = await machineService.importMachineProfile(path);
      await loadProfiles();
      setEditingProfile(cloneProfile(importedProfile));
      setIsDirty(false);
      useNotificationStore.getState().push(
        `${t('menus.file.import')} ${t('dialog.device_settings.profile')}: ${importedProfile.name}`,
        'success',
      );
    } catch (error) {
      useNotificationStore.getState().push(
        `${t('menus.file.import')} ${t('dialog.device_settings.profile')}: ${formatErrorMessage(error)}`,
        'error',
      );
    }
  };

  const runDiscardAction = (action: PendingProfileAction) => {
    if (action.type === 'new') {
      createNewProfile();
      return;
    }
    if (action.type === 'import') {
      if (isDirty) {
        const savedProfile = editingProfile
          ? profiles.find((profile) => profile.id === editingProfile.id)
          : null;
        setEditingProfile(savedProfile ? cloneProfile(savedProfile) : null);
        setIsDirty(false);
      }
      void importProfileFromPath(action.path);
      return;
    }
    setEditingProfile(cloneProfile(action.profile));
    setIsDirty(false);
  };

  const requestDiscardableAction = (action: PendingProfileAction) => {
    if (!isDirty) {
      runDiscardAction(action);
      return;
    }
    setPendingDiscardAction(action);
  };

  const handleNewProfile = () => {
    requestDiscardableAction({ type: 'new' });
  };

  const handleImport = async () => {
    try {
      const path = await machineService.pickMachineProfileImportPath();
      if (path) requestDiscardableAction({ type: 'import', path });
    } catch (error) {
      useNotificationStore.getState().push(
        `${t('menus.file.import')} ${t('dialog.device_settings.profile')}: ${formatErrorMessage(error)}`,
        'error',
      );
    }
  };

  const handleExport = async () => {
    if (!editingProfile || !editingProfileExists || isDirty) return;
    try {
      const path = await machineService.pickMachineProfileExportPath(editingProfile.name);
      if (!path) return;
      await machineService.exportMachineProfile(editingProfile.id, path);
      useNotificationStore.getState().push(
        `${t('menus.file.export')} ${t('dialog.device_settings.profile')}: ${editingProfile.name}`,
        'success',
      );
    } catch (error) {
      useNotificationStore.getState().push(
        `${t('menus.file.export')} ${t('dialog.device_settings.profile')}: ${formatErrorMessage(error)}`,
        'error',
      );
    }
  };

  const handleSelectProfile = (profile: MachineProfile) => {
    if (editingProfile?.id === profile.id) {
      return;
    }
    requestDiscardableAction({ type: 'select', profile });
  };

  const handleConfirmDiscard = () => {
    const action = pendingDiscardAction;
    setPendingDiscardAction(null);
    if (action) runDiscardAction(action);
  };

  const handleSave = useCallback(async (): Promise<boolean> => {
    if (!editingProfile) return true;
    try {
      await saveProfile(editingProfile);
      setIsDirty(false);
      return true;
    } catch {
      // Store already surfaced the error; preserve the current edits.
      return false;
    }
  }, [editingProfile, saveProfile]);

  const handleRequestClose = useCallback(() => {
    if (isDirty) {
      setClosePromptVisible(true);
      return;
    }
    onClose();
  }, [isDirty, onClose]);

  const handleSaveAndClose = async () => {
    if (await handleSave()) {
      onClose();
    }
  };

  const handleDiscardAndClose = () => {
    setIsDirty(false);
    setClosePromptVisible(false);
    onClose();
  };

  const handleDelete = async () => {
    if (!editingProfile) return;
    if (!editingProfileExists || editingProfile.id === activeProfileId) return;
    try {
      await deleteProfile(editingProfile.id);
      setEditingProfile(null);
      setIsDirty(false);
    } catch {
      // Store already surfaced the error; keep the current selection intact.
    }
  };

  const handleSetActive = async () => {
    if (!editingProfile) return;
    if ((!editingProfileExists || isDirty) && !(await handleSave())) return;
    await setActiveProfile(editingProfile.id);
  };

  const handlePresetApplied = (profile: MachineProfile) => {
    setEditingProfile(cloneProfile(profile));
    setIsDirty(false);
    void loadProfiles();
  };

    const updateField = <K extends keyof MachineProfile>(
    field: K,
    value: MachineProfile[K]
    ) => {
      if (!editingProfile) return;
      setEditingProfile({ ...editingProfile, [field]: value });
      setIsDirty(true);
    };
  const originOptions = [
    { value: 'bottom_left', label: t('dialog.machine_profile.origin_bottom_left') },
    { value: 'top_left', label: t('dialog.machine_profile.origin_top_left') },
  ];
  const airCommandOptions = [
    { value: 'M7', label: 'M7' },
    { value: 'M8', label: 'M8' },
    { value: 'custom', label: t('dialog.machine_profile.custom') },
  ];
  const transferModeOptions = [
    { value: 'buffered', label: t('dialog.machine_profile.transfer_buffered') },
    { value: 'synchronous', label: t('dialog.machine_profile.transfer_synchronous') },
  ];
  const ruidaTableAxisOptions = [
    { value: 'disabled', label: t('dialog.machine_profile.none') },
    { value: 'z', label: 'Z' },
    { value: 'u', label: 'U' },
  ];
  const isRuidaProfile = editingProfile?.firmware_type.trim().toLowerCase() === 'ruida';

  return createPortal(
    <MovableResizableDialogFrame
      title={t('dialog.machine_profile.title')}
      titleId="machine-profiles-dialog-title"
      testId="machine-profile-dialog"
      initialWidth={820}
      initialHeight={680}
      minWidth={620}
      minHeight={520}
      onRequestClose={handleRequestClose}
      closeOnBackdropClick
      footer={
        <div className="flex gap-2 justify-end px-4 py-3">
          {editingProfile && (
            <>
              <button
                onClick={() => void handleSave()}
                className="px-3 py-1 text-xs font-medium rounded bg-bb-accent hover:bg-bb-accent-hover text-bb-on-accent transition-colors"
              >
                {t('common.save')}
              </button>
              <button
                onClick={handleDelete}
                disabled={!editingProfileExists || editingProfile.id === activeProfileId}
                className="px-3 py-1 text-xs font-medium rounded bg-bb-error hover:bg-bb-error-hover text-bb-on-error transition-colors disabled:opacity-60 disabled:cursor-not-allowed"
              >
                {t('dialog.machine_profile.delete')}
              </button>
              <button
                onClick={handleSetActive}
                disabled={editingProfile.id === activeProfileId}
                className="px-3 py-1 text-xs font-medium rounded bg-bb-success hover:bg-bb-success-hover text-bb-on-success transition-colors disabled:opacity-60 disabled:cursor-not-allowed"
              >
                {t('dialog.machine_profile.set_active')}
              </button>
            </>
          )}
          <button
            onClick={handleRequestClose}
            className="px-3 py-1 text-xs font-medium rounded bg-bb-bg hover:bg-bb-hover text-bb-text transition-colors"
          >
            {t('common.close')}
          </button>
        </div>
      }
    >
      <div className="flex min-h-0 flex-1 flex-col p-4">
        {closePromptVisible && (
          <div className="mb-3 rounded border border-bb-warning-border bg-bb-warning-bg p-3 text-xs text-bb-text">
            <div className="mb-2 text-bb-warning-fg">
              {t('dialog.machine_profile.save_before_closing')}
            </div>
            <div className="flex justify-end gap-2">
              <button
                className="rounded border border-bb-border px-2 py-1 text-bb-text-muted hover:bg-bb-hover"
                onClick={() => setClosePromptVisible(false)}
              >
                {t('dialog.machine_profile.keep_editing')}
              </button>
              <button
                className="rounded border border-bb-error-border px-2 py-1 text-bb-error-fg hover:bg-bb-error-bg"
                onClick={handleDiscardAndClose}
              >
                {t('dialog.machine_profile.discard')}
              </button>
              <button
                className="rounded bg-bb-accent px-2 py-1 text-bb-on-accent hover:bg-bb-accent-hover"
                onClick={() => void handleSaveAndClose()}
              >
                {t('dialog.machine_profile.save_and_close')}
              </button>
            </div>
          </div>
        )}
        {pendingDiscardAction && (
          <div className="mb-3 rounded border border-bb-warning-border bg-bb-warning-bg p-3 text-xs text-bb-text">
            <div className="mb-2 text-bb-warning-fg">
              {t('dialog.machine_profile.discard_unsaved')}
            </div>
            <div className="flex justify-end gap-2">
              <button
                className="rounded border border-bb-border px-2 py-1 text-bb-text-muted hover:bg-bb-hover"
                onClick={() => setPendingDiscardAction(null)}
              >
                {t('dialog.machine_profile.keep_editing')}
              </button>
              <button
                className="rounded bg-bb-warning px-2 py-1 font-medium text-bb-on-warning hover:bg-bb-warning-hover"
                onClick={handleConfirmDiscard}
              >
                {t('dialog.machine_profile.discard')}
              </button>
            </div>
          </div>
        )}
        <div className="flex min-h-0 flex-1 gap-3 overflow-hidden">
          {/* Left column: Profile list */}
          <div className="w-40 border-r border-bb-border pr-3 flex flex-col gap-2">
            <div className="overflow-y-auto flex-1 space-y-1">
              {profiles.map((profile) => (
                <div
                  key={profile.id}
                  onClick={() => handleSelectProfile(profile)}
                  className={`px-2 py-1 text-xs cursor-pointer rounded ${
                    editingProfile?.id === profile.id
                      ? 'bg-bb-accent text-bb-on-accent'
                      : 'text-bb-text-muted hover:bg-bb-hover'
                  }`}
                >
                  {profile.name}
                  {profile.id === activeProfileId && (
                      <span className="ml-1 text-xs">{t('dialog.machine_profile.active_marker')}</span>
                  )}
                </div>
              ))}
            </div>
            <button
              onClick={handleNewProfile}
              className="px-2 py-1 text-xs font-medium rounded bg-bb-accent hover:bg-bb-accent-hover text-bb-on-accent transition-colors"
            >
              {t('dialog.machine_profile.new')}
            </button>
            <div className="flex gap-1">
              <button
                onClick={() => void handleImport()}
                className="flex-1 rounded border border-bb-border bg-bb-bg px-2 py-1 text-xs font-medium text-bb-text transition-colors hover:bg-bb-hover"
              >
                {t('menus.file.import')}
              </button>
              <button
                onClick={() => void handleExport()}
                disabled={!editingProfileExists || isDirty}
                className="flex-1 rounded border border-bb-border bg-bb-bg px-2 py-1 text-xs font-medium text-bb-text transition-colors hover:bg-bb-hover disabled:cursor-not-allowed disabled:opacity-50"
              >
                {t('menus.file.export')}
              </button>
            </div>
          </div>

          {/* Right column: Edit form */}
          <div
            className="scrollbar-safe-edge flex-1 overflow-y-auto pl-3"
            data-testid="machine-profile-editor-scroll-region"
          >
            {editingProfile ? (
              <div className="space-y-2">
                <TextInput
                    label={t('dialog.machine_profile.name')}
                  value={editingProfile.name}
                  onChange={(v) => updateField('name', v)}
                />
                <NumberInput
                    label={labelWithUnit(t('dialog.machine_profile.width_mm'), lengthUnitLabel(displayUnit))}
                  value={roundDisplayLength(mmToDisplay(editingProfile.bed_width_mm, displayUnit), displayUnit)}
                  onChange={(v) => updateField('bed_width_mm', displayToMm(v, displayUnit))}
                  min={mmToDisplay(1, displayUnit)}
                  max={mmToDisplay(2000, displayUnit)}
                  step={lengthStep(displayUnit, 1, 0.05)}
                />
                <NumberInput
                    label={labelWithUnit(t('dialog.machine_profile.height_mm'), lengthUnitLabel(displayUnit))}
                  value={roundDisplayLength(mmToDisplay(editingProfile.bed_height_mm, displayUnit), displayUnit)}
                  onChange={(v) => updateField('bed_height_mm', displayToMm(v, displayUnit))}
                  min={mmToDisplay(1, displayUnit)}
                  max={mmToDisplay(2000, displayUnit)}
                  step={lengthStep(displayUnit, 1, 0.05)}
                />
                <NumberInput
                    label={labelWithUnit(t('dialog.machine_profile.max_speed'), speedUnitLabel(displayUnit, speedTimeUnit))}
                  value={speedInputValue(editingProfile.max_speed_mm_min, displayUnit, speedTimeUnit)}
                  onChange={(v) => updateField('max_speed_mm_min', displaySpeedToMmMin(v, displayUnit, speedTimeUnit))}
                  min={speedInputValue(100, displayUnit, speedTimeUnit)}
                  max={speedInputValue(50000, displayUnit, speedTimeUnit)}
                  step={speedStepForUnit(displayUnit, speedTimeUnit)}
                />
                <NumberInput
                    label={t('dialog.machine_profile.max_power')}
                  value={editingProfile.max_power_percent}
                  onChange={(v) => updateField('max_power_percent', v)}
                  min={1}
                  max={100}
                />
                <NumberInput
                    label={t('dialog.machine_profile.s_value_max')}
                  value={editingProfile.s_value_max ?? 1000}
                  onChange={(v) => updateField('s_value_max', Math.max(1, Math.round(v)))}
                  min={1}
                  step={1}
                />
                <div className="flex items-center justify-between gap-2 text-xs">
                    <span className="text-bb-text-muted shrink-0">{t('dialog.machine_profile.homing')}</span>
                  <Toggle
                    checked={editingProfile.homing_enabled}
                    onChange={(v) => updateField('homing_enabled', v)}
                  />
                </div>
                <Select
                    label={t('dialog.machine_profile.baud_rate')}
                  value={String(editingProfile.default_baud_rate)}
                  options={SERIAL_BAUD_RATE_OPTIONS}
                  onChange={(v) => updateField('default_baud_rate', Number(v))}
                />
                <TextInput
                    label={t('dialog.machine_profile.firmware')}
                  value={editingProfile.firmware_type}
                  onChange={(v) => updateField('firmware_type', v)}
                />
                <Select
                    label={t('dialog.machine_profile.origin')}
                  value={editingProfile.origin ?? 'bottom_left'}
                    options={originOptions}
                  onChange={(v) => updateField('origin', v as MachineProfile['origin'])}
                />
                <TextInput
                    label={t('dialog.machine_profile.notes')}
                  value={editingProfile.notes}
                  onChange={(v) => updateField('notes', v)}
                />
                <MachinePresetPanel
                  profile={editingProfile}
                  profileExists={profiles.some((profile) => profile.id === editingProfile.id)}
                  dirty={isDirty}
                  onApplied={handlePresetApplied}
                />
                <div className="border-t border-bb-border pt-2 mt-3">
                    <div className="text-xs font-semibold text-bb-text mb-2">{t('dialog.machine_profile.camera_metadata')}</div>
                  <div className="space-y-2">
                    <TextInput
                        label={t('dialog.machine_profile.selected_camera')}
                      value={editingProfile.selected_camera_id ?? ''}
                      onChange={(v) => updateField('selected_camera_id', v.trim() ? v.trim() : null)}
                    />
                    <CameraMetadataRow
                        label={t('dialog.machine_profile.calibration')}
                      present={editingProfile.camera_calibration !== null}
                      onClear={() => updateField('camera_calibration', null)}
                    />
                    <CameraMetadataRow
                        label={t('dialog.machine_profile.alignment')}
                      present={editingProfile.camera_alignment !== null}
                      onClear={() => updateField('camera_alignment', null)}
                    />
                  </div>
                </div>
                <div className="border-t border-bb-border pt-2 mt-3">
                    <div className="text-xs font-semibold text-bb-text mb-2">{t('dialog.machine_profile.output_policy')}</div>
                  <div className="space-y-2">
                    <div className="flex items-center justify-between gap-2 text-xs">
                        <span className="text-bb-text-muted shrink-0">{t('dialog.machine_profile.constant_power')}</span>
                      <Toggle
                        checked={editingProfile.use_constant_power ?? false}
                        onChange={(v) => updateField('use_constant_power', v)}
                      />
                    </div>
                    <div className="flex items-center justify-between gap-2 text-xs">
                        <span className="text-bb-text-muted shrink-0">{t('dialog.machine_profile.emit_s_every_g1')}</span>
                      <Toggle
                        checked={editingProfile.emit_s_every_g1 ?? false}
                        onChange={(v) => updateField('emit_s_every_g1', v)}
                      />
                    </div>
                    <div className="flex items-center justify-between gap-2 text-xs">
                        <span className="text-bb-text-muted shrink-0">{t('dialog.machine_profile.laser_on_framing')}</span>
                      <Toggle
                        checked={editingProfile.laser_on_when_framing ?? false}
                        onChange={(v) => updateField('laser_on_when_framing', v)}
                      />
                    </div>
                    <div className="flex items-center justify-between gap-2 text-xs">
                        <span className="text-bb-text-muted shrink-0">{t('dialog.machine_profile.enable_laser_fire_button')}</span>
                      <Toggle
                        checked={editingProfile.enable_laser_fire_button ?? false}
                        onChange={(v) => updateField('enable_laser_fire_button', v)}
                      />
                    </div>
                    {(editingProfile.enable_laser_fire_button ?? false) && (
                      <NumberInput
                          label={t('dialog.machine_profile.default_fire_power_percent')}
                        value={editingProfile.default_fire_power_percent ?? 1}
                        onChange={(v) => updateField('default_fire_power_percent', Math.min(100, Math.max(0.1, v)))}
                        min={0.1}
                        max={100}
                        step={0.1}
                      />
                    )}
                    <div className="flex items-center justify-between gap-2 text-xs">
                        <span className="text-bb-text-muted shrink-0">{t('dialog.machine_profile.use_g0_for_overscan')}</span>
                      <Toggle
                        checked={editingProfile.use_g0_for_overscan ?? true}
                        onChange={(v) => updateField('use_g0_for_overscan', v)}
                      />
                    </div>
                    <Select
                        label={t('dialog.machine_profile.air_command')}
                      value={(editingProfile.air_assist_on_gcode ?? 'M7').trim() === 'M8' ? 'M8' : (editingProfile.air_assist_on_gcode ?? 'M7').trim() === 'M7' ? 'M7' : 'custom'}
                        options={airCommandOptions}
                      onChange={(v) => {
                        if (v === 'M7' || v === 'M8') updateField('air_assist_on_gcode', v);
                      }}
                    />
                    <TextInput
                        label={t('dialog.machine_profile.air_on_gcode')}
                      value={editingProfile.air_assist_on_gcode ?? 'M7'}
                      onChange={(v) => updateField('air_assist_on_gcode', v)}
                    />
                    <TextInput
                        label={t('dialog.machine_profile.air_off_gcode')}
                      value={editingProfile.air_assist_off_gcode ?? 'M9'}
                      onChange={(v) => updateField('air_assist_off_gcode', v)}
                    />
                    <NumberInput
                        label={t('dialog.machine_profile.air_delay')}
                      value={editingProfile.air_assist_on_delay_ms ?? 0}
                      onChange={(v) => updateField('air_assist_on_delay_ms', Math.max(0, Math.round(v)))}
                      min={0}
                      step={50}
                    />
                    <TextArea
                        label={t('dialog.machine_profile.job_header')}
                      value={editingProfile.job_header_gcode ?? ''}
                      onChange={(v) => updateField('job_header_gcode', v)}
                      monospace
                      describedBy={START_END_GCODE_HELP_ID}
                    />
                    <TextArea
                        label={t('dialog.machine_profile.job_footer')}
                      value={editingProfile.job_footer_gcode ?? ''}
                      onChange={(v) => updateField('job_footer_gcode', v)}
                      monospace
                      describedBy={START_END_GCODE_HELP_ID}
                    />
                    <div id={START_END_GCODE_HELP_ID} className="text-[11px] text-bb-text-muted">
                        {t('dialog.machine_profile.header_footer_help')}
                    </div>
                    <Select
                        label={t('dialog.machine_profile.streaming')}
                      value={editingProfile.transfer_mode ?? 'buffered'}
                        options={transferModeOptions}
                      onChange={(v) => updateField('transfer_mode', v as MachineProfile['transfer_mode'])}
                    />
                    {(editingProfile.transfer_mode ?? 'buffered') === 'synchronous' && (
                      <div className="text-[11px] text-bb-warning-fg">
                          {t('dialog.machine_profile.synchronous_warning')}
                      </div>
                    )}
                  </div>
                </div>
                <div className="border-t border-bb-border pt-2 mt-3">
                    <div className="text-xs font-semibold text-bb-text mb-2">{t('dialog.machine_profile.capabilities')}</div>
                  <div className="space-y-2">
                    {isRuidaProfile ? (
                      <>
                        <Select
                          label={t('dialog.machine_profile.ruida_table_axis')}
                          value={editingProfile.ruida_table_axis ?? 'disabled'}
                          options={ruidaTableAxisOptions}
                          onChange={(v) => {
                            setEditingProfile({
                              ...editingProfile,
                              supports_z_moves: false,
                              ruida_table_axis: v as MachineProfile['ruida_table_axis'],
                            });
                            setIsDirty(true);
                          }}
                        />
                        {(editingProfile.ruida_table_axis ?? 'disabled') !== 'disabled' && (
                          <NumberInput
                            label={labelWithUnit(t('dialog.machine_profile.ruida_table_feed'), speedUnitLabel(displayUnit, speedTimeUnit))}
                            value={speedInputValue(editingProfile.z_move_feed_mm_min ?? 300, displayUnit, speedTimeUnit)}
                            onChange={(v) => updateField('z_move_feed_mm_min', Math.max(1, displaySpeedToMmMin(v, displayUnit, speedTimeUnit)))}
                            min={speedInputValue(1, displayUnit, speedTimeUnit)}
                            step={speedStepForUnit(displayUnit, speedTimeUnit)}
                          />
                        )}
                      </>
                    ) : (
                      <>
                        <div className="flex items-center justify-between gap-2 text-xs">
                        <span className="text-bb-text-muted shrink-0" title={t('dialog.machine_profile.supports_z_title')}>
                          {t('dialog.machine_profile.supports_z_moves')}
                        </span>
                        <Toggle
                          checked={editingProfile.supports_z_moves ?? false}
                          onChange={(v) => updateField('supports_z_moves', v)}
                        />
                        </div>
                        {(editingProfile.supports_z_moves ?? false) && (
                          <NumberInput
                            label={labelWithUnit(t('dialog.machine_profile.z_feed'), speedUnitLabel(displayUnit, speedTimeUnit))}
                            value={speedInputValue(editingProfile.z_move_feed_mm_min ?? 300, displayUnit, speedTimeUnit)}
                            onChange={(v) => updateField('z_move_feed_mm_min', Math.max(1, displaySpeedToMmMin(v, displayUnit, speedTimeUnit)))}
                            min={speedInputValue(1, displayUnit, speedTimeUnit)}
                            step={speedStepForUnit(displayUnit, speedTimeUnit)}
                          />
                        )}
                      </>
                    )}
                  </div>
                </div>
                <div className="border-t border-bb-border pt-2 mt-3">
                    <div className="text-xs font-semibold text-bb-text mb-2">{t('dialog.machine_profile.calibration')}</div>
                  <div className="space-y-2">
                    <Toggle
                      label={t('dialog.machine_profile.enable_dot_width')}
                      checked={editingProfile.enable_dot_width ?? false}
                      onChange={(v) => updateField('enable_dot_width', v)}
                      className="w-full"
                      labelFirst
                    />
                    {(editingProfile.enable_dot_width ?? false) && (
                      <NumberInput
                          label={labelWithUnit(t('dialog.machine_profile.dot_width'), lengthUnitLabel(displayUnit))}
                        value={roundDisplayLength(mmToDisplay(editingProfile.dot_width_mm ?? 0, displayUnit), displayUnit)}
                        onChange={(v) => updateField('dot_width_mm', displayToMm(v, displayUnit))}
                        min={0}
                        max={mmToDisplay(1, displayUnit)}
                        step={lengthStep(displayUnit, 0.01, 0.001)}
                      />
                    )}
                    <div className="flex items-center justify-between gap-2 text-xs">
                        <span className="text-bb-text-muted shrink-0">{t('dialog.machine_profile.enable_scanning_offset')}</span>
                      <Toggle
                        checked={editingProfile.enable_scanning_offset ?? false}
                        onChange={(v) => updateField('enable_scanning_offset', v)}
                      />
                    </div>
                    {(editingProfile.enable_scanning_offset ?? false) && (
                      <ScanningOffsetTable
                        entries={editingProfile.scanning_offsets ?? []}
                        onChange={(entries) => updateField('scanning_offsets', entries)}
                      />
                    )}
                  </div>
                </div>
              </div>
            ) : (
              <div className="text-xs text-bb-text-muted text-center mt-8">
                  {t('dialog.machine_profile.select_profile')}
              </div>
            )}
          </div>
        </div>
      </div>
    </MovableResizableDialogFrame>,
    document.body
  );
}

function ScanningOffsetTable({
  entries,
  onChange,
}: {
  entries: ScanningOffsetEntry[];
  onChange: (entries: ScanningOffsetEntry[]) => void;
}) {
  const { t } = useTranslation();
  const displayUnit = useAppStore((s) => s.settings?.display_unit) ?? 'mm';
  const speedTimeUnit = useAppStore((s) => s.settings?.speed_time_unit) ?? 'minutes';
  const handleAdd = () => {
    onChange([...entries, { speed_mm_min: 1000, offset_mm: 0.1 }]);
  };

  const handleRemove = (index: number) => {
    onChange(entries.filter((_, i) => i !== index));
  };

  const handleUpdate = (index: number, field: keyof ScanningOffsetEntry, value: number) => {
    const updated = entries.map((entry, i) =>
      i === index ? { ...entry, [field]: value } : entry
    );
    onChange(updated);
  };

  return (
    <div className="space-y-1" data-testid="machine-profile-scanning-offset-table">
      <div className="flex gap-2 text-xs text-bb-text-muted font-medium">
        <span className="flex-1">{labelWithUnit(t('dialog.machine_profile.speed'), speedUnitLabel(displayUnit, speedTimeUnit))}</span>
        <span className="flex-1">{labelWithUnit(t('dialog.machine_profile.offset'), lengthUnitLabel(displayUnit))}</span>
        <span className="w-8" />
      </div>
      {entries.map((entry, i) => (
        <div key={i} className="flex gap-2 items-center">
          <NumberStepper
            value={speedInputValue(entry.speed_mm_min, displayUnit, speedTimeUnit)}
            onChange={(e) => handleUpdate(i, 'speed_mm_min', displaySpeedToMmMin(parseFloat(e.target.value) || 0, displayUnit, speedTimeUnit))}
            className="flex-1 px-1.5 py-0.5 bg-bb-bg border border-bb-border rounded text-xs text-bb-text focus:outline-none focus:border-bb-accent w-0"
            min={0}
            step={speedStepForUnit(displayUnit, speedTimeUnit)}
          />
          <NumberStepper
            value={roundDisplayLength(mmToDisplay(entry.offset_mm, displayUnit), displayUnit)}
            onChange={(e) => handleUpdate(i, 'offset_mm', displayToMm(parseFloat(e.target.value) || 0, displayUnit))}
            className="flex-1 px-1.5 py-0.5 bg-bb-bg border border-bb-border rounded text-xs text-bb-text focus:outline-none focus:border-bb-accent w-0"
            step={lengthStep(displayUnit, 0.01, 0.001)}
          />
          <button
            onClick={() => handleRemove(i)}
            className="w-8 text-xs text-bb-error-fg hover:text-bb-error"
            title={t('dialog.machine_profile.remove_entry')}
          >
            ×
          </button>
        </div>
      ))}
      <button
        onClick={handleAdd}
        className="px-2 py-0.5 text-xs rounded bg-bb-bg border border-bb-border text-bb-text hover:bg-bb-hover"
      >
        {t('dialog.machine_profile.add_entry')}
      </button>
    </div>
  );
}

function CameraMetadataRow({
  label,
  present,
  onClear,
}: {
  label: string;
  present: boolean;
  onClear: () => void;
}) {
  const { t } = useTranslation();
  return (
    <div className="flex items-center justify-between gap-2 text-xs">
      <span className="text-bb-text-muted shrink-0">{label}</span>
      <div className="flex items-center gap-2">
        <span className={present ? 'text-bb-text' : 'text-bb-text-muted'}>
          {present ? t('dialog.machine_profile.saved') : t('dialog.machine_profile.none')}
        </span>
        {present && (
          <button
            className="rounded border border-bb-border px-2 py-0.5 text-bb-text hover:bg-bb-hover"
            onClick={onClear}
          >
            {t('dialog.machine_profile.clear')}
          </button>
        )}
      </div>
    </div>
  );
}
