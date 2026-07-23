import { useCallback, useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import { useMachineStore } from '../../stores/machineStore';
import { useNotificationStore } from '../../stores/notificationStore';
import { machineService } from '../../services/machineService';
import type { MachineProfile, ScanningOffsetEntry, SessionState } from '../../types/machine';
import { TextInput } from '../shared/TextInput';
import { TextArea } from '../shared/TextArea';
import { NumberInput } from '../shared/NumberInput';
import { Toggle } from '../shared/Toggle';
import { Select } from '../shared/Select';
import { NumberStepper } from '../shared/NumberStepper';
import { MovableResizableDialogFrame } from '../shared/MovableResizableDialogFrame';
import { MachinePresetPanel } from '../machine/MachinePresetPanel';
import { ControllerChoiceControls } from '../machine/ControllerChoiceControls';
import { SESSION_STATE_DOT_CLASSES } from '../machine/stateColors';
import {
  ACTIVE_CONNECTION_STATES,
  GCODE_DEFAULT_PORT,
  GCODE_HOST_PLACEHOLDER,
  LASERPECKER_HOST_PLACEHOLDER,
  NETWORK_TRANSPORT,
  RUIDA_HOST_PLACEHOLDER,
  SERIAL_TRANSPORT,
  USB_TRANSPORT,
  connectionEndpointMissing,
  defaultPortForDriverSwitch,
  type ConnectionTransportKind,
} from '../../utils/controllerConnection';
import {
  hiddenSerialPortCount,
  preferredSerialPortName,
  serialPortLabel,
  visibleSerialPorts,
} from '../../utils/serialPorts';
import {
  mmToDisplay,
  displayToMm,
  roundDisplayLength,
  lengthStep,
  lengthUnitLabel,
  labelWithUnit,
} from '../../utils/lengthUnits';
import {
  speedInputValue,
  displaySpeedToMmMin,
  speedStepForUnit,
  speedUnitLabel,
} from '../../utils/speedUnits';
import { useAppStore } from '../../stores/appStore';
import { SERIAL_BAUD_RATE_OPTIONS } from '../../constants/serial';
import { lihuiyuUsbDeviceId, lihuiyuUsbDeviceLabel } from '../../utils/lihuiyuUsb';
import { wrapBackendError } from '../../i18n/errors';

interface DeviceSettingsDialogProps {
  onClose: () => void;
}
const START_END_GCODE_HELP_ID = 'device-settings-start-end-gcode-help';

type TabId = 'connection' | 'machine' | 'grbl' | 'controller' | 'discovery' | 'profiles';
type CloseGuardResult = Promise<boolean>;
type CandidateAction = Promise<void>;
type PendingProfileAction =
  | { type: 'new' }
  | { type: 'select'; profile: MachineProfile }
  | { type: 'import'; path: string };

type CloseGuard = {
  save: () => CloseGuardResult;
  discard: () => void;
};

const REFRESH_SYMBOL = '↻';

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

function normalizeStringRecord(payload: unknown): Record<string, string> {
  if (!payload || typeof payload !== 'object' || Array.isArray(payload)) {
    return {};
  }

  return Object.fromEntries(Object.entries(payload).map(([key, value]) => [key, String(value)]));
}

export function DeviceSettingsDialog({ onClose }: DeviceSettingsDialogProps) {
  const { t } = useTranslation();
  const [activeTab, setActiveTab] = useState<TabId>('connection');
  const [machineCloseGuard, setMachineCloseGuard] = useState<CloseGuard | null>(null);
  const [closePromptVisible, setClosePromptVisible] = useState(false);

  const handleRequestClose = useCallback(() => {
    if (machineCloseGuard) {
      setClosePromptVisible(true);
      return;
    }
    onClose();
  }, [machineCloseGuard, onClose]);

  const handleSaveAndClose = async () => {
    const guard = machineCloseGuard;
    if (!guard) {
      onClose();
      return;
    }
    if (await guard.save()) {
      onClose();
    }
  };

  const handleDiscardAndClose = () => {
    machineCloseGuard?.discard();
    setClosePromptVisible(false);
    onClose();
  };

  const tabs: { id: TabId; label: string }[] = [
    { id: 'connection', label: t('dialog.device_settings.tab_connection') },
    { id: 'machine', label: t('dialog.device_settings.tab_machine') },
    { id: 'grbl', label: t('dialog.device_settings.tab_grbl') },
    { id: 'controller', label: t('dialog.device_settings.tab_controller') },
    { id: 'discovery', label: t('dialog.device_settings.tab_discovery') },
    { id: 'profiles', label: t('dialog.device_settings.tab_profiles') },
  ];

  return createPortal(
    <MovableResizableDialogFrame
      title={t('dialog.device_settings.title')}
      titleId="dialog-title"
      testId="device-settings-dialog"
      initialWidth={900}
      initialHeight={680}
      minWidth={680}
      minHeight={520}
      onRequestClose={handleRequestClose}
      closeOnBackdropClick
      footer={
        <div className="flex justify-end px-4 py-3">
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
        {closePromptVisible && machineCloseGuard && (
          <div className="mb-3 rounded border border-bb-warning-border bg-bb-warning-bg p-3 text-xs text-bb-text">
            <div className="mb-2 text-bb-warning-fg">
              {t('dialog.device_settings.save_before_closing')}
            </div>
            <div className="flex justify-end gap-2">
              <button
                className="rounded border border-bb-border px-2 py-1 text-bb-text-muted hover:bg-bb-hover"
                onClick={() => setClosePromptVisible(false)}
              >
                {t('dialog.device_settings.keep_editing')}
              </button>
              <button
                className="rounded border border-bb-error-border px-2 py-1 text-bb-error-fg hover:bg-bb-error-bg"
                onClick={handleDiscardAndClose}
              >
                {t('dialog.device_settings.discard')}
              </button>
              <button
                className="rounded bg-bb-accent px-2 py-1 text-bb-on-accent hover:bg-bb-accent-hover"
                onClick={() => void handleSaveAndClose()}
              >
                {t('dialog.device_settings.save_and_close')}
              </button>
            </div>
          </div>
        )}
        <div className="flex shrink-0 gap-1 border-b border-bb-border mb-3" data-testid="tab-bar">
          {tabs.map((tab) => (
            <button
              key={tab.id}
              data-testid={`tab-${tab.id}`}
              onClick={() => setActiveTab(tab.id)}
              className={`px-3 py-1.5 text-xs font-medium rounded-t transition-colors ${
                activeTab === tab.id
                  ? 'bg-bb-bg text-bb-text border border-b-0 border-bb-border -mb-px'
                  : 'text-bb-text-muted hover:text-bb-text'
              }`}
            >
              {tab.label}
            </button>
          ))}
        </div>

        <div
          className="scrollbar-safe-edge min-h-0 flex-1 overflow-y-auto"
          data-testid="device-settings-scroll-region"
        >
          <ConnectionTab active={activeTab === 'connection'} />
          <MachineTab active={activeTab === 'machine'} onCloseGuardChange={setMachineCloseGuard} />
          {activeTab === 'grbl' && <GrblTab />}
          {activeTab === 'controller' && <ControllerTab />}
          {activeTab === 'discovery' && <DiscoveryTab />}
          {activeTab === 'profiles' && <ProfilesTab />}
        </div>
      </div>
    </MovableResizableDialogFrame>,
    document.body,
  );
}

function ConnectionTab({ active }: { active: boolean }) {
  const { t } = useTranslation();
  const sessionState = useMachineStore((s) => s.sessionState);
  const availablePorts = useMachineStore((s) => s.availablePorts);
  const availableLihuiyuUsbDevices = useMachineStore((s) => s.availableLihuiyuUsbDevices);
  const connectedPort = useMachineStore((s) => s.connectedPort);
  const profiles = useMachineStore((s) => s.profiles);
  const activeProfileId = useMachineStore((s) => s.activeProfileId);
  const loading = useMachineStore((s) => s.loading);
  const error = useMachineStore((s) => s.error);
  const controllerSelection = useMachineStore((s) => s.controllerSelection);
  const controllerConnectionChallenge = useMachineStore((s) => s.controllerConnectionChallenge);
  const connect = useMachineStore((s) => s.connect);
  const connectNetwork = useMachineStore((s) => s.connectNetwork);
  const connectUsb = useMachineStore((s) => s.connectUsb);
  const disconnect = useMachineStore((s) => s.disconnect);
  const refreshPorts = useMachineStore((s) => s.refreshPorts);
  const refreshLihuiyuUsbDevices = useMachineStore((s) => s.refreshLihuiyuUsbDevices);
  const setActiveProfile = useMachineStore((s) => s.setActiveProfile);

  const activeProfile = (profiles ?? []).find((p) => p.id === activeProfileId);
  const defaultBaudRate = activeProfile?.default_baud_rate ?? 115200;

  const [selectedPort, setSelectedPort] = useState(connectedPort ?? '');
  const [baudRate, setBaudRate] = useState(defaultBaudRate);
  const [showAllPorts, setShowAllPorts] = useState(false);
  const [transportKind, setTransportKind] = useState<ConnectionTransportKind>(SERIAL_TRANSPORT);
  const [networkHost, setNetworkHost] = useState('');
  const [networkPort, setNetworkPort] = useState(GCODE_DEFAULT_PORT);
  const [selectedUsbDeviceId, setSelectedUsbDeviceId] = useState('');
  const ruidaSelected =
    controllerSelection.mode === 'known_driver' && controllerSelection.driver === 'ruida';
  const laserPeckerSelected =
    controllerSelection.mode === 'known_driver' && controllerSelection.driver === 'laser_pecker';

  useEffect(() => {
    refreshPorts();
  }, [refreshPorts]);

  useEffect(() => {
    if (transportKind === USB_TRANSPORT) {
      void refreshLihuiyuUsbDevices();
    }
  }, [refreshLihuiyuUsbDevices, transportKind]);

  useEffect(() => {
    setBaudRate(defaultBaudRate);
  }, [defaultBaudRate]);

  useEffect(() => {
    if (connectedPort) {
      setSelectedPort(connectedPort);
    }
  }, [connectedPort]);

  useEffect(() => {
    if (transportKind !== NETWORK_TRANSPORT) return;
    const controller = ruidaSelected ? 'ruida' : laserPeckerSelected ? 'laserpecker' : 'gcode';
    setNetworkPort((current) => defaultPortForDriverSwitch(current, controller));
    setNetworkHost((current) => {
      const host = current.trim();
      if (laserPeckerSelected && (host === '' || host === RUIDA_HOST_PLACEHOLDER)) {
        return LASERPECKER_HOST_PLACEHOLDER;
      }
      if (!laserPeckerSelected && host === LASERPECKER_HOST_PLACEHOLDER) {
        return '';
      }
      return current;
    });
  }, [laserPeckerSelected, ruidaSelected, transportKind]);

  const handleConnect = () => {
    if (!ACTIVE_CONNECTION_STATES.includes(sessionState)) {
      if (transportKind === NETWORK_TRANSPORT) {
        connectNetwork(networkHost, networkPort);
      } else if (transportKind === USB_TRANSPORT) {
        const device = availableLihuiyuUsbDevices.find(
          (candidate) => lihuiyuUsbDeviceId(candidate) === selectedUsbDeviceId,
        );
        if (device) connectUsb(device);
      } else {
        connect(selectedPort, baudRate);
      }
    } else {
      disconnect();
    }
  };

  const isConnected = ACTIVE_CONNECTION_STATES.includes(sessionState);
  const connectionPending = controllerConnectionChallenge !== null;
  const endpointMissing = connectionEndpointMissing(
    transportKind,
    networkHost,
    networkPort,
    selectedUsbDeviceId,
    selectedPort,
  );
  const isConnectDisabled = endpointMissing || loading || connectionPending;

  const visiblePorts = visibleSerialPorts(availablePorts ?? [], showAllPorts, selectedPort);
  const hiddenPortCount = hiddenSerialPortCount(availablePorts ?? [], selectedPort);

  useEffect(() => {
    if (isConnected || selectedPort !== '') return;
    const preferredPort = preferredSerialPortName(visiblePorts);
    if (preferredPort) setSelectedPort(preferredPort);
  }, [isConnected, selectedPort, visiblePorts]);

  useEffect(() => {
    if (isConnected || selectedUsbDeviceId !== '') return;
    const first = availableLihuiyuUsbDevices[0];
    if (first) setSelectedUsbDeviceId(lihuiyuUsbDeviceId(first));
  }, [availableLihuiyuUsbDevices, isConnected, selectedUsbDeviceId]);

  const stateLabels: Record<SessionState, string> = {
    disconnected: t('dialog.device_settings.state_disconnected'),
    connecting: t('dialog.device_settings.state_connecting'),
    transport_open: t('dialog.device_settings.state_opening'),
    waiting_for_banner: t('dialog.device_settings.state_waiting'),
    validating: t('dialog.device_settings.state_validating'),
    ready: t('dialog.device_settings.state_ready'),
    running: t('dialog.device_settings.state_running'),
    paused: t('dialog.device_settings.state_paused'),
    alarm: t('dialog.device_settings.state_alarm'),
    error: t('dialog.device_settings.state_error'),
  };

  const portOptions = [
    {
      value: '',
      label:
        visiblePorts.length === 0
          ? t('dialog.device_settings.no_laser_ports')
          : t('dialog.device_settings.select_port'),
    },
    ...visiblePorts.map((port) => ({
      value: port.port_name,
      label: serialPortLabel(port),
    })),
  ];

  const profileOptions = (profiles ?? []).map((profile) => ({
    value: profile.id,
    label: profile.name,
  }));

  const usbDeviceOptions = [
    {
      value: '',
      label:
        availableLihuiyuUsbDevices.length === 0
          ? t('controller_choice.no_usb_devices')
          : t('controller_choice.select_usb_device'),
    },
    ...availableLihuiyuUsbDevices.map((device) => ({
      value: lihuiyuUsbDeviceId(device),
      label: lihuiyuUsbDeviceLabel(device),
    })),
  ];

  if (!active) {
    return null;
  }

  return (
    <div className="space-y-3" data-testid="connection-tab">
      <Select
        label={t('controller_choice.transport')}
        value={transportKind}
        options={[
          { value: SERIAL_TRANSPORT, label: t('controller_choice.transport_serial') },
          { value: NETWORK_TRANSPORT, label: t('controller_choice.transport_network') },
          { value: USB_TRANSPORT, label: t('controller_choice.transport_usb') },
        ]}
        onChange={(value) => setTransportKind(value as ConnectionTransportKind)}
        disabled={isConnected || connectionPending}
      />

      {transportKind === SERIAL_TRANSPORT ? (
        <>
          <div className="flex items-center gap-2">
            <Select
              label={t('dialog.device_settings.port')}
              value={selectedPort}
              options={portOptions}
              onChange={setSelectedPort}
              disabled={isConnected || connectionPending}
              selectClassName="w-36"
            />
            <button
              onClick={refreshPorts}
              disabled={isConnected || connectionPending}
              className="px-2 py-1 text-xs bg-bb-bg border border-bb-border rounded hover:bg-bb-accent-hover hover:text-bb-on-accent disabled:opacity-60 text-bb-text"
              title={t('dialog.device_settings.refresh_ports')}
            >
              {REFRESH_SYMBOL}
            </button>
          </div>
          {hiddenPortCount > 0 && (
            <label className="flex items-center justify-between gap-2 text-xs text-bb-text-muted">
              <span>{t('dialog.device_settings.show_all_ports', { count: hiddenPortCount })}</span>
              <input
                type="checkbox"
                checked={showAllPorts}
                disabled={isConnected || connectionPending}
                onChange={(event) => setShowAllPorts(event.target.checked)}
                className="h-3.5 w-3.5 accent-bb-accent disabled:opacity-60"
              />
            </label>
          )}

          <Select
            label={t('dialog.device_settings.baud_rate')}
            value={String(baudRate)}
            options={SERIAL_BAUD_RATE_OPTIONS}
            onChange={(v) => setBaudRate(Number(v))}
            disabled={isConnected || connectionPending}
          />
        </>
      ) : transportKind === NETWORK_TRANSPORT ? (
        <div className="grid grid-cols-[minmax(0,1fr)_6rem] gap-2 text-xs">
          <label className="flex min-w-0 flex-col gap-1 text-bb-text">
            <span>{t('controller_choice.network_host')}</span>
            <input
              type="text"
              value={networkHost}
              onChange={(event) => setNetworkHost(event.target.value)}
              disabled={isConnected || connectionPending}
              placeholder={
                ruidaSelected
                  ? RUIDA_HOST_PLACEHOLDER
                  : laserPeckerSelected
                    ? LASERPECKER_HOST_PLACEHOLDER
                    : GCODE_HOST_PLACEHOLDER
              }
              className="min-w-0 rounded border border-bb-border bg-bb-bg px-2 py-1 text-bb-text disabled:opacity-60"
              data-testid="device-settings-network-host"
            />
          </label>
          <label className="flex flex-col gap-1 text-bb-text">
            <span>
              {t(ruidaSelected ? 'controller_choice.udp_port' : 'controller_choice.tcp_port')}
            </span>
            <input
              type="number"
              min={1}
              max={65535}
              value={networkPort}
              onChange={(event) => setNetworkPort(Number(event.target.value))}
              disabled={isConnected || connectionPending}
              className="min-w-0 rounded border border-bb-border bg-bb-bg px-2 py-1 text-bb-text disabled:opacity-60"
              data-testid="device-settings-network-port"
            />
          </label>
        </div>
      ) : (
        <div className="flex items-center gap-2 text-xs">
          <Select
            label={t('controller_choice.usb_device')}
            value={selectedUsbDeviceId}
            options={usbDeviceOptions}
            onChange={setSelectedUsbDeviceId}
            disabled={isConnected || connectionPending}
            selectClassName="min-w-0 flex-1"
          />
          <button
            type="button"
            onClick={() => void refreshLihuiyuUsbDevices()}
            disabled={isConnected || connectionPending || loading}
            className="mt-4 px-2 py-1 text-xs bg-bb-bg border border-bb-border rounded hover:bg-bb-accent-hover hover:text-bb-on-accent disabled:opacity-60 text-bb-text"
            title={t('controller_choice.refresh_usb_devices')}
          >
            {REFRESH_SYMBOL}
          </button>
        </div>
      )}

      <ControllerChoiceControls disabled={isConnected} transportKind={transportKind} />

      <Select
        label={t('dialog.device_settings.profile')}
        value={activeProfileId ?? ''}
        options={profileOptions}
        onChange={(v) => setActiveProfile(v || null)}
        disabled={isConnected || connectionPending || (profiles ?? []).length === 0}
      />

      <button
        onClick={handleConnect}
        disabled={!isConnected && isConnectDisabled}
        className="w-full px-3 py-1.5 text-xs bg-bb-accent hover:bg-bb-accent-hover rounded text-bb-on-accent disabled:opacity-60 disabled:cursor-not-allowed"
      >
        {isConnected ? t('dialog.device_settings.disconnect') : t('dialog.device_settings.connect')}
      </button>

      <div className="flex items-center gap-2 text-xs">
        <div className={`w-2 h-2 rounded-full ${SESSION_STATE_DOT_CLASSES[sessionState]}`} />
        <span className="text-bb-text-muted">{stateLabels[sessionState]}</span>
      </div>

      {error && (
        <div
          role="alert"
          data-testid="device-settings-connection-error"
          className="rounded border border-bb-error/40 bg-bb-error/10 px-3 py-2 text-xs text-bb-error-fg"
        >
          {wrapBackendError(error)}
        </div>
      )}
    </div>
  );
}

function MachineTab({
  active,
  onCloseGuardChange,
}: {
  active: boolean;
  onCloseGuardChange: (guard: CloseGuard | null) => void;
}) {
  const { t } = useTranslation();
  const displayUnit = useAppStore((s) => s.settings?.display_unit) ?? 'mm';
  const speedTimeUnit = useAppStore((s) => s.settings?.speed_time_unit) ?? 'minutes';
  const profiles = useMachineStore((s) => s.profiles);
  const activeProfileId = useMachineStore((s) => s.activeProfileId);
  const saveProfile = useMachineStore((s) => s.saveProfile);
  const activeProfile = profiles.find((p) => p.id === activeProfileId);

  const [draftsByProfileId, setDraftsByProfileId] = useState<
    Record<
      string,
      {
        draft: MachineProfile;
        dirty: boolean;
      }
    >
  >({});

  useEffect(() => {
    setDraftsByProfileId((current) => {
      const next: Record<string, { draft: MachineProfile; dirty: boolean }> = {};
      let changed = Object.keys(current).length !== profiles.length;

      for (const profile of profiles) {
        const existing = current[profile.id];
        if (!existing) {
          next[profile.id] = { draft: cloneProfile(profile), dirty: false };
          changed = true;
          continue;
        }

        if (!existing.dirty && !sameProfileSnapshot(existing.draft, profile)) {
          next[profile.id] = { draft: cloneProfile(profile), dirty: false };
          changed = true;
          continue;
        }

        next[profile.id] = existing;
      }

      return changed ? next : current;
    });
  }, [profiles]);

  const currentDraft = activeProfileId ? draftsByProfileId[activeProfileId] : undefined;
  const editProfile = currentDraft?.draft ?? (activeProfile ? cloneProfile(activeProfile) : null);

  const updateField = <K extends keyof MachineProfile>(field: K, value: MachineProfile[K]) => {
    if (!activeProfileId || !editProfile) return;
    setDraftsByProfileId((current) => ({
      ...current,
      [activeProfileId]: {
        draft: { ...editProfile, [field]: value },
        dirty: true,
      },
    }));
  };

  const handleSave = useCallback(async (): CloseGuardResult => {
    if (!editProfile) return true;
    try {
      await saveProfile(editProfile);
      setDraftsByProfileId((current) => ({
        ...current,
        [editProfile.id]: {
          draft: cloneProfile(editProfile),
          dirty: false,
        },
      }));
      return true;
    } catch {
      // Store already surfaced the error; preserve the current edits.
      return false;
    }
  }, [editProfile, saveProfile]);

  const handleDiscard = useCallback(() => {
    if (!activeProfileId || !activeProfile) return;
    setDraftsByProfileId((current) => ({
      ...current,
      [activeProfileId]: {
        draft: cloneProfile(activeProfile),
        dirty: false,
      },
    }));
  }, [activeProfile, activeProfileId]);

  useEffect(() => {
    if (currentDraft?.dirty) {
      onCloseGuardChange({
        save: handleSave,
        discard: handleDiscard,
      });
      return () => onCloseGuardChange(null);
    }
    onCloseGuardChange(null);
    return undefined;
  }, [currentDraft?.dirty, handleDiscard, handleSave, onCloseGuardChange]);

  const originOptions = [
    { value: 'bottom_left', label: t('dialog.device_settings.origin_bottom_left') },
    { value: 'top_left', label: t('dialog.device_settings.origin_top_left') },
  ];

  if (!active) {
    return null;
  }

  if (!editProfile) {
    return (
      <div className="text-xs text-bb-text-muted text-center mt-8" data-testid="machine-tab">
        {t('dialog.device_settings.no_active_profile')}
      </div>
    );
  }

  return (
    <div className="space-y-2" data-testid="machine-tab">
      <NumberInput
        label={labelWithUnit(t('dialog.device_settings.bed_width'), lengthUnitLabel(displayUnit))}
        value={roundDisplayLength(mmToDisplay(editProfile.bed_width_mm, displayUnit), displayUnit)}
        onChange={(v) => updateField('bed_width_mm', displayToMm(v, displayUnit))}
        min={mmToDisplay(1, displayUnit)}
        max={mmToDisplay(2000, displayUnit)}
        step={lengthStep(displayUnit, 1, 0.05)}
      />
      <NumberInput
        label={labelWithUnit(t('dialog.device_settings.bed_height'), lengthUnitLabel(displayUnit))}
        value={roundDisplayLength(mmToDisplay(editProfile.bed_height_mm, displayUnit), displayUnit)}
        onChange={(v) => updateField('bed_height_mm', displayToMm(v, displayUnit))}
        min={mmToDisplay(1, displayUnit)}
        max={mmToDisplay(2000, displayUnit)}
        step={lengthStep(displayUnit, 1, 0.05)}
      />
      <NumberInput
        label={labelWithUnit(
          t('dialog.device_settings.max_speed'),
          speedUnitLabel(displayUnit, speedTimeUnit),
        )}
        value={speedInputValue(editProfile.max_speed_mm_min, displayUnit, speedTimeUnit)}
        onChange={(v) =>
          updateField('max_speed_mm_min', displaySpeedToMmMin(v, displayUnit, speedTimeUnit))
        }
        min={speedInputValue(100, displayUnit, speedTimeUnit)}
        max={speedInputValue(50000, displayUnit, speedTimeUnit)}
        step={speedStepForUnit(displayUnit, speedTimeUnit)}
      />
      <NumberInput
        label={t('dialog.device_settings.max_power')}
        value={editProfile.max_power_percent}
        onChange={(v) => updateField('max_power_percent', v)}
        min={1}
        max={100}
      />
      <div className="flex items-center justify-between gap-2 text-xs">
        <span className="text-bb-text-muted shrink-0">{t('dialog.device_settings.homing')}</span>
        <Toggle
          checked={editProfile.homing_enabled}
          onChange={(v) => updateField('homing_enabled', v)}
        />
      </div>
      <Select
        label={t('dialog.device_settings.origin')}
        value={editProfile.origin ?? 'bottom_left'}
        options={originOptions}
        onChange={(v) => updateField('origin', v as MachineProfile['origin'])}
      />
      <div className="flex items-center justify-between gap-2 text-xs">
        <span className="text-bb-text-muted shrink-0">
          {t('dialog.device_settings.job_checklist')}
        </span>
        <Toggle
          checked={editProfile.job_checklist ?? false}
          onChange={(v) => updateField('job_checklist', v)}
        />
      </div>
      <div className="flex items-center justify-between gap-2 text-xs">
        <span className="text-bb-text-muted shrink-0">
          {t('dialog.device_settings.frame_continuously')}
        </span>
        <Toggle
          checked={editProfile.frame_continuously ?? false}
          onChange={(v) => updateField('frame_continuously', v)}
        />
      </div>
      <div className="flex items-center justify-between gap-2 text-xs">
        <span className="text-bb-text-muted shrink-0">
          {t('dialog.device_settings.laser_on_framing')}
        </span>
        <Toggle
          checked={editProfile.laser_on_when_framing ?? false}
          onChange={(v) => updateField('laser_on_when_framing', v)}
        />
      </div>
      <div className="flex items-center justify-between gap-2 text-xs">
        <span className="text-bb-text-muted shrink-0">
          {t('dialog.device_settings.enable_laser_fire_button')}
        </span>
        <Toggle
          checked={editProfile.enable_laser_fire_button ?? false}
          onChange={(v) => updateField('enable_laser_fire_button', v)}
        />
      </div>
      {(editProfile.enable_laser_fire_button ?? false) && (
        <NumberInput
          label={t('dialog.device_settings.default_fire_power_percent')}
          value={editProfile.default_fire_power_percent ?? 1}
          onChange={(v) =>
            updateField('default_fire_power_percent', Math.min(100, Math.max(0.1, v)))
          }
          min={0.1}
          max={100}
          step={0.1}
        />
      )}
      <NumberInput
        label={t('dialog.device_settings.tab_pulse_width')}
        value={editProfile.tab_pulse_width_ms ?? 0}
        onChange={(v) => updateField('tab_pulse_width_ms', v)}
        min={0}
        max={10000}
        step={1}
      />
      <div className="flex items-center justify-between gap-2 text-xs">
        <span className="text-bb-text-muted shrink-0">
          {t('dialog.device_settings.cnc_machine')}
        </span>
        <Toggle
          checked={editProfile.cnc_machine ?? false}
          onChange={(v) => updateField('cnc_machine', v)}
        />
      </div>

      <div className="border-t border-bb-border pt-2 mt-3">
        <div className="text-xs font-semibold text-bb-text mb-2">
          {t('dialog.device_settings.camera_metadata')}
        </div>
        <div className="space-y-2">
          <TextInput
            label={t('dialog.device_settings.selected_camera')}
            value={editProfile.selected_camera_id ?? ''}
            onChange={(v) => updateField('selected_camera_id', v.trim() ? v.trim() : null)}
          />
          <CameraMetadataRow
            label={t('dialog.device_settings.calibration')}
            present={editProfile.camera_calibration !== null}
            onClear={() => updateField('camera_calibration', null)}
          />
          <CameraMetadataRow
            label={t('dialog.device_settings.alignment')}
            present={editProfile.camera_alignment !== null}
            onClear={() => updateField('camera_alignment', null)}
          />
        </div>
      </div>

      {/* Output Policy */}
      <div className="border-t border-bb-border pt-2 mt-3">
        <div className="text-xs font-semibold text-bb-text mb-2">
          {t('dialog.device_settings.output_policy')}
        </div>
        <div className="space-y-2">
          <div className="flex items-center justify-between gap-2 text-xs">
            <span className="text-bb-text-muted shrink-0">
              {t('dialog.device_settings.constant_power')}
            </span>
            <Toggle
              checked={editProfile.use_constant_power ?? false}
              onChange={(v) => updateField('use_constant_power', v)}
            />
          </div>
          <div className="flex items-center justify-between gap-2 text-xs">
            <span className="text-bb-text-muted shrink-0">
              {t('dialog.device_settings.emit_s_every_g1')}
            </span>
            <Toggle
              checked={editProfile.emit_s_every_g1 ?? false}
              onChange={(v) => updateField('emit_s_every_g1', v)}
            />
          </div>
          <NumberInput
            label={t('dialog.device_settings.s_value_max')}
            value={editProfile.s_value_max ?? 1000}
            onChange={(v) => updateField('s_value_max', Math.max(1, Math.round(v)))}
            min={1}
            step={1}
          />
          <div className="flex items-center justify-between gap-2 text-xs">
            <span className="text-bb-text-muted shrink-0">
              {t('dialog.device_settings.use_g0_for_overscan')}
            </span>
            <Toggle
              checked={editProfile.use_g0_for_overscan ?? true}
              onChange={(v) => updateField('use_g0_for_overscan', v)}
            />
          </div>
        </div>
      </div>

      {/* Calibration */}
      <div className="border-t border-bb-border pt-2 mt-3">
        <div className="text-xs font-semibold text-bb-text mb-2">
          {t('dialog.device_settings.calibration')}
        </div>
        <div className="space-y-2">
          <Toggle
            label={t('dialog.device_settings.enable_dot_width')}
            checked={editProfile.enable_dot_width ?? false}
            onChange={(v) => updateField('enable_dot_width', v)}
            className="w-full"
            labelFirst
          />
          {(editProfile.enable_dot_width ?? false) && (
            <NumberInput
              label={labelWithUnit(
                t('dialog.device_settings.dot_width'),
                lengthUnitLabel(displayUnit),
              )}
              value={roundDisplayLength(
                mmToDisplay(editProfile.dot_width_mm ?? 0, displayUnit),
                displayUnit,
              )}
              onChange={(v) => updateField('dot_width_mm', displayToMm(v, displayUnit))}
              min={mmToDisplay(0, displayUnit)}
              max={mmToDisplay(1, displayUnit)}
              step={lengthStep(displayUnit, 0.01, 0.0005)}
            />
          )}
          <div className="flex items-center justify-between gap-2 text-xs">
            <span className="text-bb-text-muted shrink-0">
              {t('dialog.device_settings.enable_scanning_offset')}
            </span>
            <Toggle
              checked={editProfile.enable_scanning_offset ?? false}
              onChange={(v) => updateField('enable_scanning_offset', v)}
            />
          </div>
          {(editProfile.enable_scanning_offset ?? false) && (
            <ScanningOffsetTable
              entries={editProfile.scanning_offsets ?? []}
              onChange={(entries) => updateField('scanning_offsets', entries)}
            />
          )}
        </div>
      </div>

      <div className="flex justify-end mt-3">
        <button
          data-testid="machine-save-btn"
          onClick={() => void handleSave()}
          className="px-3 py-1 text-xs font-medium rounded bg-bb-accent hover:bg-bb-accent-hover text-bb-on-accent transition-colors"
        >
          {t('common.save')}
        </button>
      </div>
    </div>
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
    const updated = entries.map((entry, i) => (i === index ? { ...entry, [field]: value } : entry));
    onChange(updated);
  };

  return (
    <div className="space-y-1" data-testid="scanning-offset-table">
      <div className="flex gap-2 text-xs text-bb-text-muted font-medium">
        <span className="flex-1">
          {labelWithUnit(
            t('dialog.device_settings.speed'),
            speedUnitLabel(displayUnit, speedTimeUnit),
          )}
        </span>
        <span className="flex-1">
          {labelWithUnit(t('dialog.device_settings.offset'), lengthUnitLabel(displayUnit))}
        </span>
        <span className="w-8" />
      </div>
      {entries.map((entry, i) => (
        <div key={i} className="flex gap-2 items-center">
          <NumberStepper
            value={speedInputValue(entry.speed_mm_min, displayUnit, speedTimeUnit)}
            onChange={(e) =>
              handleUpdate(
                i,
                'speed_mm_min',
                displaySpeedToMmMin(parseFloat(e.target.value) || 0, displayUnit, speedTimeUnit),
              )
            }
            className="flex-1 px-1.5 py-0.5 bg-bb-bg border border-bb-border rounded text-xs text-bb-text focus:outline-none focus:border-bb-accent w-0"
            min={0}
            step={speedStepForUnit(displayUnit, speedTimeUnit)}
          />
          <NumberStepper
            value={roundDisplayLength(mmToDisplay(entry.offset_mm, displayUnit), displayUnit)}
            onChange={(e) =>
              handleUpdate(
                i,
                'offset_mm',
                displayToMm(parseFloat(e.target.value) || 0, displayUnit),
              )
            }
            className="flex-1 px-1.5 py-0.5 bg-bb-bg border border-bb-border rounded text-xs text-bb-text focus:outline-none focus:border-bb-accent w-0"
            step={lengthStep(displayUnit, 0.01, 0.0005)}
          />
          <button
            onClick={() => handleRemove(i)}
            className="w-8 text-xs text-bb-error-fg hover:text-bb-error"
            title={t('dialog.device_settings.remove_entry')}
          >
            ×
          </button>
        </div>
      ))}
      <button
        onClick={handleAdd}
        className="px-2 py-0.5 text-xs rounded bg-bb-bg border border-bb-border text-bb-text hover:bg-bb-hover"
      >
        {t('dialog.device_settings.add_entry')}
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
          {present ? t('dialog.device_settings.saved') : t('dialog.device_settings.none')}
        </span>
        {present && (
          <button
            className="rounded border border-bb-border px-2 py-0.5 text-bb-text hover:bg-bb-hover"
            onClick={onClear}
          >
            {t('dialog.device_settings.clear')}
          </button>
        )}
      </div>
    </div>
  );
}

function GrblTab() {
  const { t } = useTranslation();
  const profiles = useMachineStore((s) => s.profiles);
  const activeProfileId = useMachineStore((s) => s.activeProfileId);
  const saveProfile = useMachineStore((s) => s.saveProfile);
  const [settings, setSettings] = useState<Record<string, string>>({});
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [editKey, setEditKey] = useState<string | null>(null);
  const [editValue, setEditValue] = useState('');
  const [profileApplyBusy, setProfileApplyBusy] = useState(false);

  useEffect(() => {
    let cancelled = false;

    setLoading(true);
    setLoadError(null);
    machineService
      .getGrblSettings()
      .then((s) => {
        if (cancelled) return;
        setSettings(normalizeStringRecord(s));
        setLoading(false);
      })
      .catch((err) => {
        if (cancelled) return;
        setSettings({});
        setLoadError(formatErrorMessage(err));
        setLoading(false);
      });

    return () => {
      cancelled = true;
    };
  }, []);

  const handleEdit = (key: string, value: string) => {
    setEditKey(key);
    setEditValue(value);
    setSaveError(null);
  };

  const handleSaveSetting = async () => {
    if (!editKey) return;
    const rawKey = editKey.startsWith('$') ? editKey.slice(1) : '';
    const numKey = /^\d+$/.test(rawKey) ? Number(rawKey) : Number.NaN;
    const normalizedValue = editValue.trim();
    const numValue = normalizedValue === '' ? Number.NaN : Number(normalizedValue);
    if (!Number.isFinite(numValue)) {
      const msg = t('dialog.device_settings.grbl_value_number');
      setSaveError(msg);
      useNotificationStore.getState().push(msg, 'error');
      return;
    }
    try {
      await machineService.setGrblSetting(numKey, numValue);
      setSettings((prev) => ({ ...prev, [editKey]: editValue }));
      setSaveError(null);
      setEditKey(null);
      setEditValue('');
    } catch (err) {
      const msg = formatErrorMessage(err);
      setSaveError(msg);
      useNotificationStore.getState().push(msg, 'error');
    }
  };

  const handleCancelEdit = () => {
    setEditKey(null);
    setEditValue('');
    setSaveError(null);
  };

  const activeProfile = activeProfileId
    ? profiles.find((profile) => profile.id === activeProfileId)
    : null;
  const grblNumber = (key: string): number | null => {
    const parsed = Number(settings[key]);
    return Number.isFinite(parsed) && parsed > 0 ? parsed : null;
  };
  const profileSValueMax = grblNumber('$30');
  const axisRates = [grblNumber('$110'), grblNumber('$111')].filter(
    (value): value is number => value !== null,
  );
  const profileMaxSpeed = axisRates.length > 0 ? Math.min(...axisRates) : null;
  const canApplyToProfile =
    activeProfile !== null && (profileSValueMax !== null || profileMaxSpeed !== null);

  const handleApplyToProfile = async () => {
    if (!activeProfile || !canApplyToProfile) return;
    const nextProfile: MachineProfile = { ...activeProfile };
    if (profileSValueMax !== null) {
      nextProfile.s_value_max = Math.round(profileSValueMax);
    }
    if (profileMaxSpeed !== null) {
      nextProfile.max_speed_mm_min = Math.round(profileMaxSpeed);
    }
    try {
      setProfileApplyBusy(true);
      await saveProfile(nextProfile);
      useNotificationStore
        .getState()
        .push(t('dialog.device_settings.grbl_applied_to_profile'), 'success');
    } catch (err) {
      const msg = formatErrorMessage(err);
      setSaveError(msg);
      useNotificationStore.getState().push(msg, 'error');
    } finally {
      setProfileApplyBusy(false);
    }
  };

  const entries = Object.entries(settings).sort(([a], [b]) =>
    a.localeCompare(b, undefined, { numeric: true }),
  );

  return (
    <div data-testid="grbl-tab">
      {loading ? (
        <div className="text-xs text-bb-text-muted text-center mt-4">
          {t('dialog.device_settings.loading_grbl')}
        </div>
      ) : loadError ? (
        <div className="text-xs text-bb-error-fg text-center mt-4" data-testid="grbl-error">
          {loadError}
        </div>
      ) : entries.length === 0 ? (
        <div className="text-xs text-bb-text-muted text-center mt-4">
          {t('dialog.device_settings.no_grbl_settings')}
        </div>
      ) : (
        <div className="space-y-1">
          {saveError && (
            <div className="text-xs text-bb-error-fg" data-testid="grbl-save-error">
              {saveError}
            </div>
          )}
          <div className="rounded border border-bb-border bg-bb-bg/50 p-2 text-xs text-bb-text-muted">
            <div className="mb-2">{t('dialog.device_settings.apply_live_grbl_help')}</div>
            <button
              className="rounded bg-bb-accent px-2 py-1 font-medium text-bb-on-accent hover:bg-bb-accent-hover disabled:cursor-not-allowed disabled:opacity-50"
              disabled={!canApplyToProfile || profileApplyBusy}
              onClick={() => void handleApplyToProfile()}
            >
              {t('dialog.device_settings.apply_to_active_profile')}
            </button>
          </div>
          {entries.map(([key, value]) => (
            <div key={key} className="flex items-center justify-between gap-2 text-xs py-0.5">
              <span className="text-bb-text-muted font-mono">{key}</span>
              {editKey === key ? (
                <div className="flex gap-1 items-center">
                  <input
                    type="text"
                    value={editValue}
                    onChange={(e) => setEditValue(e.target.value)}
                    className="w-20 px-1.5 py-0.5 bg-bb-bg border border-bb-border rounded text-xs text-bb-text focus:outline-none focus:border-bb-accent"
                  />
                  <button
                    onClick={handleSaveSetting}
                    className="px-1.5 py-0.5 text-xs rounded bg-bb-accent text-bb-on-accent"
                  >
                    {t('common.ok')}
                  </button>
                  <button
                    onClick={handleCancelEdit}
                    className="px-1.5 py-0.5 text-xs rounded bg-bb-bg text-bb-text"
                  >
                    {t('common.cancel')}
                  </button>
                </div>
              ) : (
                <span
                  onClick={() => handleEdit(key, value)}
                  className="text-bb-text cursor-pointer hover:text-bb-accent font-mono"
                >
                  {value}
                </span>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function ControllerTab() {
  const { t } = useTranslation();
  const [info, setInfo] = useState<Record<string, string>>({});
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    setLoading(true);
    setError(null);
    machineService
      .getControllerInfo()
      .then((i) => {
        if (cancelled) return;
        setInfo(normalizeStringRecord(i));
        setLoading(false);
      })
      .catch((err) => {
        if (cancelled) return;
        setInfo({});
        setError(formatErrorMessage(err));
        setLoading(false);
      });

    return () => {
      cancelled = true;
    };
  }, []);

  const entries = Object.entries(info).sort(([a], [b]) => a.localeCompare(b));

  return (
    <div data-testid="controller-tab">
      {loading ? (
        <div className="text-xs text-bb-text-muted text-center mt-4">
          {t('dialog.device_settings.loading_controller_info')}
        </div>
      ) : error ? (
        <div className="text-xs text-bb-error-fg text-center mt-4" data-testid="controller-error">
          {error}
        </div>
      ) : entries.length === 0 ? (
        <div className="text-xs text-bb-text-muted text-center mt-4">
          {t('dialog.device_settings.no_controller_info')}
        </div>
      ) : (
        <div className="space-y-1">
          {entries.map(([key, value]) => (
            <div key={key} className="flex items-center justify-between gap-2 text-xs py-0.5">
              <span className="text-bb-text-muted">{key}</span>
              <span className="text-bb-text font-mono">{value}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function DiscoveryTab() {
  const { t } = useTranslation();
  const discoveryState = useMachineStore((s) => s.discoveryState) ?? {
    phase: 'idle' as const,
    status_text: t('dialog.device_settings.waiting_for_connection'),
    candidates: [],
    scanned_serial_count: 0,
    scanned_tcp_count: 0,
    scanned_usb_count: 0,
    started_at: null,
    completed_at: null,
  };
  const loading = useMachineStore((s) => s.loading);
  const refreshDiscoveryState = useMachineStore((s) => s.refreshDiscoveryState);
  const startDiscovery = useMachineStore((s) => s.startDiscovery);
  const cancelDiscovery = useMachineStore((s) => s.cancelDiscovery);
  const connectCandidate = useMachineStore((s) => s.connectCandidate);
  const bootstrapProfileFromCandidate = useMachineStore((s) => s.bootstrapProfileFromCandidate);
  const candidateActionInFlightRef = useRef(false);
  const [pendingCandidateId, setPendingCandidateId] = useState<string | null>(null);

  const runCandidateAction = async (candidateId: string, action: () => CandidateAction) => {
    if (candidateActionInFlightRef.current || loading) return;
    candidateActionInFlightRef.current = true;
    setPendingCandidateId(candidateId);
    try {
      await action();
    } finally {
      candidateActionInFlightRef.current = false;
      setPendingCandidateId(null);
    }
  };

  return (
    <div data-testid="discovery-tab">
      <div className="flex items-center justify-between gap-2 mb-3">
        <div>
          <div className="text-xs font-semibold text-bb-text">
            {t('dialog.device_settings.device_discovery')}
          </div>
          <div className="text-xs text-bb-text-muted">{discoveryState.status_text}</div>
        </div>
        <div className="flex gap-1">
          <button
            className="px-2 py-1 text-xs rounded bg-bb-bg border border-bb-border text-bb-text hover:bg-bb-hover"
            onClick={() => void refreshDiscoveryState()}
          >
            {t('dialog.device_settings.refresh')}
          </button>
          <button
            className="px-2 py-1 text-xs rounded bg-bb-accent text-bb-on-accent hover:bg-bb-accent-hover disabled:opacity-60"
            disabled={loading}
            onClick={() => void startDiscovery()}
          >
            {t('dialog.device_settings.scan')}
          </button>
          {discoveryState.phase === 'scanning' && (
            <button
              className="px-2 py-1 text-xs rounded bg-bb-bg border border-bb-border text-bb-text hover:bg-bb-hover"
              onClick={() => void cancelDiscovery()}
            >
              {t('common.cancel')}
            </button>
          )}
        </div>
      </div>

      <div className="text-xs text-bb-text-muted mb-3">
        {t('dialog.device_settings.discovery_counts', {
          serial: discoveryState.scanned_serial_count,
          tcp: discoveryState.scanned_tcp_count,
          usb: discoveryState.scanned_usb_count,
        })}
      </div>

      {discoveryState.candidates.length === 0 ? (
        <div className="text-xs text-bb-text-muted text-center mt-4">
          {t('dialog.device_settings.no_discovery_candidates')}
        </div>
      ) : (
        <div className="space-y-2">
          {discoveryState.candidates.map((candidate) => (
            <div key={candidate.id} className="rounded border border-bb-border p-2">
              <div className="flex items-start justify-between gap-2">
                <div className="min-w-0">
                  <div className="text-xs font-medium text-bb-text truncate">
                    {candidate.identity.display_name}
                  </div>
                  <div className="text-xs text-bb-text-muted">
                    {candidate.controller_family} / {candidate.controller_model} /{' '}
                    {candidate.transport_kind}
                  </div>
                  <div className="text-xs text-bb-text-muted">{candidate.status_text}</div>
                  {candidate.unsupported_reason && (
                    <div
                      id={`discovery-unsupported-${candidate.id}`}
                      className="text-xs text-bb-warning-fg"
                    >
                      {candidate.unsupported_reason}
                    </div>
                  )}
                </div>
                {candidate.controller_model !== 'unknown' && (
                  <div className="text-xs text-bb-text-muted whitespace-nowrap">
                    {(candidate.confidence * 100).toFixed(0)}%
                  </div>
                )}
              </div>
              <div className="flex gap-1 mt-2">
                <button
                  className="px-2 py-1 text-xs rounded bg-bb-bg border border-bb-border text-bb-text hover:bg-bb-hover"
                  disabled={loading || pendingCandidateId === candidate.id}
                  onClick={() =>
                    void runCandidateAction(candidate.id, () =>
                      bootstrapProfileFromCandidate(candidate.id, candidate.identity.display_name),
                    )
                  }
                >
                  {t('dialog.device_settings.bootstrap_profile')}
                </button>
                <button
                  className="px-2 py-1 text-xs rounded bg-bb-accent text-bb-on-accent hover:bg-bb-accent-hover disabled:opacity-60 disabled:cursor-not-allowed"
                  disabled={
                    loading ||
                    pendingCandidateId === candidate.id ||
                    candidate.product_tier === 'unavailable' ||
                    Boolean(candidate.unsupported_reason?.trim())
                  }
                  aria-describedby={
                    candidate.unsupported_reason
                      ? `discovery-unsupported-${candidate.id}`
                      : undefined
                  }
                  title={candidate.unsupported_reason ?? undefined}
                  onClick={() =>
                    void runCandidateAction(candidate.id, () => connectCandidate(candidate.id))
                  }
                >
                  {t('dialog.device_settings.connect')}
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function ProfilesTab() {
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
  const [pendingDiscardAction, setPendingDiscardAction] = useState<PendingProfileAction | null>(
    null,
  );
  const editingProfileExists =
    editingProfile !== null && profiles.some((profile) => profile.id === editingProfile.id);

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
      name: t('dialog.device_settings.new_profile'),
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
      enable_laser_fire_button: false,
      default_fire_power_percent: 1,
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
      useNotificationStore
        .getState()
        .push(
          `${t('menus.file.import')} ${t('dialog.device_settings.profile')}: ${importedProfile.name}`,
          'success',
        );
    } catch (error) {
      useNotificationStore
        .getState()
        .push(
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
      useNotificationStore
        .getState()
        .push(
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
      useNotificationStore
        .getState()
        .push(
          `${t('menus.file.export')} ${t('dialog.device_settings.profile')}: ${editingProfile.name}`,
          'success',
        );
    } catch (error) {
      useNotificationStore
        .getState()
        .push(
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

  const handleSave = async (): Promise<boolean> => {
    if (!editingProfile) return true;
    try {
      await saveProfile(editingProfile);
      setIsDirty(false);
      return true;
    } catch {
      // Store already surfaced the error; preserve the current edits.
      return false;
    }
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

  const updateField = <K extends keyof MachineProfile>(field: K, value: MachineProfile[K]) => {
    if (!editingProfile) return;
    setEditingProfile({ ...editingProfile, [field]: value });
    setIsDirty(true);
  };
  const originOptions = [
    { value: 'bottom_left', label: t('dialog.device_settings.origin_bottom_left') },
    { value: 'top_left', label: t('dialog.device_settings.origin_top_left') },
  ];
  const transferModeOptions = [
    { value: 'buffered', label: t('dialog.device_settings.transfer_buffered') },
    { value: 'synchronous', label: t('dialog.device_settings.transfer_synchronous') },
  ];
  const airCommandOptions = [
    { value: 'M7', label: 'M7' },
    { value: 'M8', label: 'M8' },
    { value: 'custom', label: t('dialog.device_settings.custom') },
  ];

  return (
    <div data-testid="profiles-tab">
      {pendingDiscardAction && (
        <div className="mb-3 rounded border border-bb-warning-border bg-bb-warning-bg p-3 text-xs text-bb-text">
          <div className="mb-2 text-bb-warning-fg">
            {t('dialog.device_settings.discard_unsaved')}
          </div>
          <div className="flex justify-end gap-2">
            <button
              className="rounded border border-bb-border px-2 py-1 text-bb-text-muted hover:bg-bb-hover"
              onClick={() => setPendingDiscardAction(null)}
            >
              {t('dialog.device_settings.keep_editing')}
            </button>
            <button
              className="rounded bg-bb-warning px-2 py-1 font-medium text-bb-on-warning hover:bg-bb-warning-hover"
              onClick={handleConfirmDiscard}
            >
              {t('dialog.device_settings.discard')}
            </button>
          </div>
        </div>
      )}
      <div className="flex gap-3 min-h-[250px]">
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
                  <span className="ml-1 text-xs">{t('dialog.device_settings.active_marker')}</span>
                )}
              </div>
            ))}
          </div>
          <button
            onClick={handleNewProfile}
            className="px-2 py-1 text-xs font-medium rounded bg-bb-accent hover:bg-bb-accent-hover text-bb-on-accent transition-colors"
          >
            {t('dialog.device_settings.new')}
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
        <div className="flex-1 pl-3 overflow-y-auto">
          {editingProfile ? (
            <div className="space-y-2">
              <TextInput
                label={t('dialog.device_settings.name')}
                value={editingProfile.name}
                onChange={(v) => updateField('name', v)}
              />
              <NumberInput
                label={labelWithUnit(
                  t('dialog.device_settings.width_mm'),
                  lengthUnitLabel(displayUnit),
                )}
                value={roundDisplayLength(
                  mmToDisplay(editingProfile.bed_width_mm, displayUnit),
                  displayUnit,
                )}
                onChange={(v) => updateField('bed_width_mm', displayToMm(v, displayUnit))}
                min={mmToDisplay(1, displayUnit)}
                max={mmToDisplay(2000, displayUnit)}
                step={lengthStep(displayUnit, 1, 0.05)}
              />
              <NumberInput
                label={labelWithUnit(
                  t('dialog.device_settings.height_mm'),
                  lengthUnitLabel(displayUnit),
                )}
                value={roundDisplayLength(
                  mmToDisplay(editingProfile.bed_height_mm, displayUnit),
                  displayUnit,
                )}
                onChange={(v) => updateField('bed_height_mm', displayToMm(v, displayUnit))}
                min={mmToDisplay(1, displayUnit)}
                max={mmToDisplay(2000, displayUnit)}
                step={lengthStep(displayUnit, 1, 0.05)}
              />
              <NumberInput
                label={labelWithUnit(
                  t('dialog.device_settings.max_speed_short'),
                  speedUnitLabel(displayUnit, speedTimeUnit),
                )}
                value={speedInputValue(editingProfile.max_speed_mm_min, displayUnit, speedTimeUnit)}
                onChange={(v) =>
                  updateField(
                    'max_speed_mm_min',
                    displaySpeedToMmMin(v, displayUnit, speedTimeUnit),
                  )
                }
                min={speedInputValue(100, displayUnit, speedTimeUnit)}
                max={speedInputValue(50000, displayUnit, speedTimeUnit)}
                step={speedStepForUnit(displayUnit, speedTimeUnit)}
              />
              <NumberInput
                label={t('dialog.device_settings.max_power_short')}
                value={editingProfile.max_power_percent}
                onChange={(v) => updateField('max_power_percent', v)}
                min={1}
                max={100}
              />
              <div className="flex items-center justify-between gap-2 text-xs">
                <span className="text-bb-text-muted shrink-0">
                  {t('dialog.device_settings.homing')}
                </span>
                <Toggle
                  checked={editingProfile.homing_enabled}
                  onChange={(v) => updateField('homing_enabled', v)}
                />
              </div>
              <Select
                label={t('dialog.device_settings.baud_rate')}
                value={String(editingProfile.default_baud_rate)}
                options={SERIAL_BAUD_RATE_OPTIONS}
                onChange={(v) => updateField('default_baud_rate', Number(v))}
              />
              <TextInput
                label={t('dialog.device_settings.firmware')}
                value={editingProfile.firmware_type}
                onChange={(v) => updateField('firmware_type', v)}
              />
              <Select
                label={t('dialog.device_settings.origin')}
                value={editingProfile.origin ?? 'bottom_left'}
                options={originOptions}
                onChange={(v) => updateField('origin', v as MachineProfile['origin'])}
              />
              <TextInput
                label={t('dialog.device_settings.notes')}
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
                <div className="text-xs font-semibold text-bb-text mb-2">
                  {t('dialog.device_settings.camera_metadata')}
                </div>
                <div className="space-y-2">
                  <TextInput
                    label={t('dialog.device_settings.selected_camera')}
                    value={editingProfile.selected_camera_id ?? ''}
                    onChange={(v) => updateField('selected_camera_id', v.trim() ? v.trim() : null)}
                  />
                  <CameraMetadataRow
                    label={t('dialog.device_settings.calibration')}
                    present={editingProfile.camera_calibration !== null}
                    onClear={() => updateField('camera_calibration', null)}
                  />
                  <CameraMetadataRow
                    label={t('dialog.device_settings.alignment')}
                    present={editingProfile.camera_alignment !== null}
                    onClear={() => updateField('camera_alignment', null)}
                  />
                </div>
              </div>
              <div className="border-t border-bb-border pt-2 mt-3">
                <div className="text-xs font-semibold text-bb-text mb-2">
                  {t('dialog.device_settings.output_policy')}
                </div>
                <div className="space-y-2">
                  <div className="flex items-center justify-between gap-2 text-xs">
                    <span className="text-bb-text-muted shrink-0">
                      {t('dialog.device_settings.constant_power')}
                    </span>
                    <Toggle
                      checked={editingProfile.use_constant_power ?? false}
                      onChange={(v) => updateField('use_constant_power', v)}
                    />
                  </div>
                  <div className="flex items-center justify-between gap-2 text-xs">
                    <span className="text-bb-text-muted shrink-0">
                      {t('dialog.device_settings.laser_on_framing')}
                    </span>
                    <Toggle
                      checked={editingProfile.laser_on_when_framing ?? false}
                      onChange={(v) => updateField('laser_on_when_framing', v)}
                    />
                  </div>
                  <div className="flex items-center justify-between gap-2 text-xs">
                    <span className="text-bb-text-muted shrink-0">
                      {t('dialog.device_settings.enable_laser_fire_button')}
                    </span>
                    <Toggle
                      checked={editingProfile.enable_laser_fire_button ?? false}
                      onChange={(v) => updateField('enable_laser_fire_button', v)}
                    />
                  </div>
                  {(editingProfile.enable_laser_fire_button ?? false) && (
                    <NumberInput
                      label={t('dialog.device_settings.default_fire_power_percent')}
                      value={editingProfile.default_fire_power_percent ?? 1}
                      onChange={(v) =>
                        updateField('default_fire_power_percent', Math.min(100, Math.max(0.1, v)))
                      }
                      min={0.1}
                      max={100}
                      step={0.1}
                    />
                  )}
                  <div className="flex items-center justify-between gap-2 text-xs">
                    <span className="text-bb-text-muted shrink-0">
                      {t('dialog.device_settings.emit_s_every_g1')}
                    </span>
                    <Toggle
                      checked={editingProfile.emit_s_every_g1 ?? false}
                      onChange={(v) => updateField('emit_s_every_g1', v)}
                    />
                  </div>
                  <NumberInput
                    label={t('dialog.device_settings.s_value_max')}
                    value={editingProfile.s_value_max ?? 1000}
                    onChange={(v) => updateField('s_value_max', Math.max(1, Math.round(v)))}
                    min={1}
                    step={1}
                  />
                  <div className="flex items-center justify-between gap-2 text-xs">
                    <span className="text-bb-text-muted shrink-0">
                      {t('dialog.device_settings.use_g0_for_overscan')}
                    </span>
                    <Toggle
                      checked={editingProfile.use_g0_for_overscan ?? true}
                      onChange={(v) => updateField('use_g0_for_overscan', v)}
                    />
                  </div>
                  <Select
                    label={t('dialog.device_settings.air_command')}
                    value={
                      (editingProfile.air_assist_on_gcode ?? 'M7').trim() === 'M8'
                        ? 'M8'
                        : (editingProfile.air_assist_on_gcode ?? 'M7').trim() === 'M7'
                          ? 'M7'
                          : 'custom'
                    }
                    options={airCommandOptions}
                    onChange={(v) => {
                      if (v === 'M7' || v === 'M8') updateField('air_assist_on_gcode', v);
                    }}
                  />
                  <TextInput
                    label={t('dialog.device_settings.air_on_gcode')}
                    value={editingProfile.air_assist_on_gcode ?? 'M7'}
                    onChange={(v) => updateField('air_assist_on_gcode', v)}
                  />
                  <TextInput
                    label={t('dialog.device_settings.air_off_gcode')}
                    value={editingProfile.air_assist_off_gcode ?? 'M9'}
                    onChange={(v) => updateField('air_assist_off_gcode', v)}
                  />
                  <NumberInput
                    label={t('dialog.device_settings.air_delay')}
                    value={editingProfile.air_assist_on_delay_ms ?? 0}
                    onChange={(v) =>
                      updateField('air_assist_on_delay_ms', Math.max(0, Math.round(v)))
                    }
                    min={0}
                    step={50}
                  />
                  <TextArea
                    label={t('dialog.device_settings.job_header')}
                    value={editingProfile.job_header_gcode ?? ''}
                    onChange={(v) => updateField('job_header_gcode', v)}
                    monospace
                    describedBy={START_END_GCODE_HELP_ID}
                  />
                  <TextArea
                    label={t('dialog.device_settings.job_footer')}
                    value={editingProfile.job_footer_gcode ?? ''}
                    onChange={(v) => updateField('job_footer_gcode', v)}
                    monospace
                    describedBy={START_END_GCODE_HELP_ID}
                  />
                  <div id={START_END_GCODE_HELP_ID} className="text-[11px] text-bb-text-muted">
                    {t('dialog.device_settings.header_footer_help')}
                  </div>
                  <Select
                    label={t('dialog.device_settings.streaming')}
                    value={editingProfile.transfer_mode ?? 'buffered'}
                    options={transferModeOptions}
                    onChange={(v) =>
                      updateField('transfer_mode', v as MachineProfile['transfer_mode'])
                    }
                  />
                  {(editingProfile.transfer_mode ?? 'buffered') === 'synchronous' && (
                    <div className="text-[11px] text-bb-warning-fg">
                      {t('dialog.device_settings.synchronous_warning')}
                    </div>
                  )}
                </div>
              </div>
              <div className="border-t border-bb-border pt-2 mt-3">
                <div className="text-xs font-semibold text-bb-text mb-2">
                  {t('dialog.device_settings.calibration')}
                </div>
                <div className="space-y-2">
                  <Toggle
                    label={t('dialog.device_settings.enable_dot_width')}
                    checked={editingProfile.enable_dot_width ?? false}
                    onChange={(v) => updateField('enable_dot_width', v)}
                    className="w-full"
                    labelFirst
                  />
                  {(editingProfile.enable_dot_width ?? false) && (
                    <NumberInput
                      label={labelWithUnit(
                        t('dialog.device_settings.dot_width'),
                        lengthUnitLabel(displayUnit),
                      )}
                      value={roundDisplayLength(
                        mmToDisplay(editingProfile.dot_width_mm ?? 0, displayUnit),
                        displayUnit,
                      )}
                      onChange={(v) => updateField('dot_width_mm', displayToMm(v, displayUnit))}
                      min={mmToDisplay(0, displayUnit)}
                      max={mmToDisplay(1, displayUnit)}
                      step={lengthStep(displayUnit, 0.01, 0.0005)}
                    />
                  )}
                  <div className="flex items-center justify-between gap-2 text-xs">
                    <span className="text-bb-text-muted shrink-0">
                      {t('dialog.device_settings.enable_scanning_offset')}
                    </span>
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
              {t('dialog.device_settings.select_profile')}
            </div>
          )}
        </div>
      </div>

      {/* Action buttons */}
      {editingProfile && (
        <div className="flex gap-2 justify-end mt-3">
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
            {t('dialog.device_settings.delete')}
          </button>
          <button
            onClick={handleSetActive}
            disabled={editingProfile.id === activeProfileId}
            className="px-3 py-1 text-xs font-medium rounded bg-bb-success hover:bg-bb-success-hover text-bb-on-success transition-colors disabled:opacity-60 disabled:cursor-not-allowed"
          >
            {t('dialog.device_settings.set_active')}
          </button>
        </div>
      )}
    </div>
  );
}
