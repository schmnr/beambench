import { create } from 'zustand';
import type {
  DiscoveryScanState,
  SessionState,
  MachineStatus,
  PortInfo,
  JobProgress,
  PreflightReport,
  MachineProfile,
  FrameMode,
  ControllerConnectionResult,
  ControllerConnectionEndpoint,
  ControllerMismatchDecision,
  ControllerSelection,
  DeviceCapabilities,
  LihuiyuUsbDeviceInfo,
} from '../types/machine';
import { machineService } from '../services/machineService';
import { discoveryService } from '../services/discoveryService';
import { appService } from '../services/appService';
import { useNotificationStore } from './notificationStore';
import { usePreviewStore } from './previewStore';
import { useCameraStore } from './cameraStore';
import { useProjectStore } from './projectStore';
import { useUiStore } from './uiStore';
import { sessionJobOptions } from '../types/jobOptions';
import { wrapBackendError } from '../i18n/errors';
import i18n from '../i18n';

const notifyError = (msg: string) => useNotificationStore.getState().push(wrapBackendError(msg), 'error');
const notifySuccess = (msg: string) => useNotificationStore.getState().push(msg, 'success');

// The preview machine simulates a fully-featured GRBL controller.
const PREVIEW_CAPABILITIES: DeviceCapabilities = {
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
};
const invalidateMachinePreview = () => usePreviewStore.getState().invalidate();
const suppressedProfileEvents = new Map<string, number>();
const PREVIEW_CONNECTED_PORT = 'Preview machine';
type ControllerConnectionChallenge = Extract<ControllerConnectionResult, { status: 'challenge' }>;

export function controllerEndpointDisplayName(endpoint: ControllerConnectionEndpoint): string {
  if (endpoint.type === 'serial') return endpoint.port_name;
  if (endpoint.type === 'usb') {
    return `${endpoint.device_id} (${endpoint.vendor_id.toString(16).padStart(4, '0')}:${endpoint.product_id.toString(16).padStart(4, '0')})`;
  }
  const host = endpoint.host.trim();
  const displayHost = host.includes(':') && !(host.startsWith('[') && host.endsWith(']'))
    ? `[${host}]`
    : host;
  return `${displayHost}:${endpoint.port}`;
}

function makePreviewMachineStatus(): MachineStatus {
  return {
    run_state: 'idle',
    machine_position: { x: 0, y: 0, z: 0 },
    work_position: { x: 0, y: 0, z: 0 },
    feed_rate: 0,
    spindle_speed: 0,
    feed_override: 100,
    spindle_override: 100,
    rapid_override: 100,
    pin_states: '',
  };
}

function isAlreadyConnectedError(error: unknown): boolean {
  return String(error).toLowerCase().includes('already connected');
}

function profileEventKey(eventType: string, profileId?: string | null) {
  return `${eventType}:${profileId ?? '__none__'}`;
}

export function suppressProfileEvent(eventType: string, profileId?: string | null) {
  const key = profileEventKey(eventType, profileId);
  suppressedProfileEvents.set(key, (suppressedProfileEvents.get(key) ?? 0) + 1);
}

export function releaseSuppressedProfileEvent(eventType: string, profileId?: string | null) {
  const key = profileEventKey(eventType, profileId);
  const current = suppressedProfileEvents.get(key) ?? 0;
  if (current <= 1) {
    suppressedProfileEvents.delete(key);
  } else {
    suppressedProfileEvents.set(key, current - 1);
  }
}

export function consumeSuppressedProfileEvent(
  eventType: string,
  profileId?: string | null,
): boolean {
  const key = profileEventKey(eventType, profileId);
  const current = suppressedProfileEvents.get(key) ?? 0;
  if (current <= 0) return false;
  releaseSuppressedProfileEvent(eventType, profileId);
  return true;
}

export function resetSuppressedProfileEvents() {
  suppressedProfileEvents.clear();
}

interface MachineStoreState {
  // Connection
  sessionState: SessionState;
  machineStatus: MachineStatus | null;
  machineCoordinatesValid: boolean;
  availablePorts: PortInfo[];
  availableLihuiyuUsbDevices: LihuiyuUsbDeviceInfo[];
  connectedPort: string | null;
  connectionPreview: boolean;
  controllerSelection: ControllerSelection;
  controllerConnectionChallenge: ControllerConnectionChallenge | null;
  /** Capabilities of the connected controller; null while unknown (UI gates fail closed). */
  capabilities: DeviceCapabilities | null;

  // Job
  jobProgress: JobProgress | null;
  preflightReport: PreflightReport | null;

  // Profiles
  profiles: MachineProfile[];
  activeProfileId: string | null;

  // Discovery
  discoveryState: DiscoveryScanState;

  // Job options
  frameSelectedOnly: boolean;

  // UI state
  loading: boolean;
  error: string | null;
  showPreflightDialog: boolean;

  // Actions
  refreshPorts: () => Promise<void>;
  refreshLihuiyuUsbDevices: () => Promise<void>;
  connect: (portName: string, baudRate?: number, selection?: ControllerSelection) => Promise<void>;
  connectNetwork: (host: string, port: number, selection?: ControllerSelection) => Promise<void>;
  connectUsb: (device: LihuiyuUsbDeviceInfo) => Promise<void>;
  setControllerSelection: (selection: ControllerSelection) => void;
  continueControllerConnection: (decision?: ControllerMismatchDecision) => Promise<void>;
  disconnect: () => Promise<void>;
  loadRuntimeCapabilities: () => Promise<void>;
  setConnectionPreview: (enabled: boolean) => void;
  refreshStatus: () => Promise<void>;
  refreshSessionState: () => Promise<void>;
  hydrateSession: () => Promise<void>;
  home: () => Promise<void>;
  unlock: () => Promise<void>;
  jog: (xMm: number, yMm: number, feedRate: number, zMm?: number | null, continuous?: boolean) => Promise<void>;
  runPreflight: () => Promise<PreflightReport | null>;
  startJob: () => Promise<void>;
  frameJob: (
    frameMode?: FrameMode,
    selectedObjectIds?: string[],
    laserOnOverride?: boolean,
  ) => Promise<JobProgress | null>;
  refreshJobProgress: () => Promise<void>;
  pauseJob: () => Promise<void>;
  resumeJob: () => Promise<void>;
  cancelJob: () => Promise<void>;
  loadProfiles: () => Promise<void>;
  saveProfile: (profile: MachineProfile) => Promise<void>;
  deleteProfile: (profileId: string) => Promise<void>;
  setActiveProfile: (profileId: string | null) => Promise<void>;
  refreshDiscoveryState: () => Promise<void>;
  startDiscovery: () => Promise<void>;
  cancelDiscovery: () => Promise<void>;
  connectCandidate: (candidateId: string) => Promise<void>;
  bootstrapProfileFromCandidate: (candidateId: string, profileName?: string) => Promise<void>;
  emergencyStop: () => Promise<void>;
  setWorkOrigin: () => Promise<void>;
  resetWorkOrigin: () => Promise<void>;
  setFrameSelectedOnly: (val: boolean) => void;
  openPreflightDialog: () => void;
  closePreflightDialog: () => void;
}

export const useMachineStore = create<MachineStoreState>((set, get) => ({
  sessionState: 'disconnected',
  machineStatus: null,
  machineCoordinatesValid: false,
  availablePorts: [],
  availableLihuiyuUsbDevices: [],
  connectedPort: null,
  connectionPreview: false,
  controllerSelection: { mode: 'known_driver', driver: 'grbl' },
  controllerConnectionChallenge: null,
  capabilities: null,
  jobProgress: null,
  preflightReport: null,
  profiles: [],
  activeProfileId: null,
  discoveryState: {
    phase: 'idle',
    status_text: 'Waiting for connection...',
    candidates: [],
    scanned_serial_count: 0,
    scanned_tcp_count: 0,
    scanned_usb_count: 0,
    started_at: null,
    completed_at: null,
  },
  frameSelectedOnly: false,
  loading: false,
  error: null,
  showPreflightDialog: false,

  refreshPorts: async () => {
    if (get().connectionPreview) {
      set({ availablePorts: [], error: null });
      return;
    }
    try {
      const ports = await machineService.listSerialPorts();
      set({ availablePorts: ports, error: null });
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    }
  },

  refreshLihuiyuUsbDevices: async () => {
    if (get().connectionPreview) {
      set({ availableLihuiyuUsbDevices: [], error: null });
      return;
    }
    try {
      const devices = await machineService.listLihuiyuUsbDevices();
      set({ availableLihuiyuUsbDevices: devices, error: null });
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    }
  },

  connect: async (portName, baudRate, selection) => {
    set({ loading: true, error: null });
    try {
      if (get().connectionPreview) {
        set({ connectionPreview: false });
      }
      const controllerSelection = selection ?? get().controllerSelection;
      set({ controllerSelection });
      const result = await machineService.beginControllerConnection(
        portName,
        baudRate,
        controllerSelection,
      );
      if (result.status === 'challenge') {
        set({
          sessionState: 'disconnected',
          connectedPort: null,
          machineStatus: null,
          machineCoordinatesValid: false,
          jobProgress: null,
          controllerConnectionChallenge: result,
          loading: false,
        });
        return;
      }
      if (result.status === 'cancelled') {
        set({
          sessionState: 'disconnected',
          connectedPort: null,
          controllerConnectionChallenge: null,
          loading: false,
        });
        return;
      }
      let status = null;
      try {
        status = await machineService.getMachineStatus();
      } catch {
        // The polling hook will retry; connection itself already succeeded.
      }
      const sessionState = status?.run_state === 'alarm' ? 'alarm' : result.session_state;
      set({
        sessionState,
        connectedPort: controllerEndpointDisplayName(result.endpoint),
        machineStatus: status,
        machineCoordinatesValid: false,
        jobProgress: null,
        controllerConnectionChallenge: null,
        loading: false,
      });
      void get().loadRuntimeCapabilities();
      notifySuccess(`Connected to ${controllerEndpointDisplayName(result.endpoint)}`);
    } catch (e) {
      const msg = String(e);
      if (isAlreadyConnectedError(e)) {
        await get().hydrateSession();
        set({ error: null, loading: false, controllerConnectionChallenge: null });
        notifySuccess('Using existing machine connection');
        return;
      }
      set({ error: msg, loading: false });
      notifyError(msg);
    }
  },

  connectNetwork: async (host, port, selection) => {
    set({ loading: true, error: null });
    try {
      if (get().connectionPreview) {
        set({ connectionPreview: false });
      }
      const controllerSelection = selection ?? get().controllerSelection;
      set({ controllerSelection });
      const result = await machineService.beginNetworkControllerConnection(
        host,
        port,
        controllerSelection,
      );
      if (result.status === 'challenge') {
        set({
          sessionState: 'disconnected',
          connectedPort: null,
          machineStatus: null,
          machineCoordinatesValid: false,
          jobProgress: null,
          controllerConnectionChallenge: result,
          loading: false,
        });
        return;
      }
      if (result.status === 'cancelled') {
        set({
          sessionState: 'disconnected',
          connectedPort: null,
          controllerConnectionChallenge: null,
          loading: false,
        });
        return;
      }
      let status = null;
      try {
        status = await machineService.getMachineStatus();
      } catch {
        // The polling hook will retry; connection itself already succeeded.
      }
      const sessionState = status?.run_state === 'alarm' ? 'alarm' : result.session_state;
      const endpointName = controllerEndpointDisplayName(result.endpoint);
      set({
        sessionState,
        connectedPort: endpointName,
        machineStatus: status,
        machineCoordinatesValid: false,
        jobProgress: null,
        controllerConnectionChallenge: null,
        loading: false,
      });
      void get().loadRuntimeCapabilities();
      notifySuccess(`Connected to ${endpointName}`);
    } catch (e) {
      const msg = String(e);
      if (isAlreadyConnectedError(e)) {
        await get().hydrateSession();
        set({ error: null, loading: false, controllerConnectionChallenge: null });
        notifySuccess('Using existing machine connection');
        return;
      }
      set({ error: msg, loading: false });
      notifyError(msg);
    }
  },

  connectUsb: async (device) => {
    set({ loading: true, error: null });
    try {
      if (get().connectionPreview) {
        set({ connectionPreview: false });
      }
      const controllerSelection: ControllerSelection = {
        mode: 'known_driver',
        driver: 'lihuiyu',
      };
      set({ controllerSelection });
      const result = await machineService.beginUsbControllerConnection(
        device,
        controllerSelection,
      );
      if (result.status !== 'connected') {
        throw new Error('Lihuiyu USB connection did not complete');
      }
      let status = null;
      try {
        status = await machineService.getMachineStatus();
      } catch {
        // The polling hook will retry; connection itself already succeeded.
      }
      const endpointName = controllerEndpointDisplayName(result.endpoint);
      set({
        sessionState: result.session_state,
        connectedPort: endpointName,
        machineStatus: status,
        machineCoordinatesValid: false,
        jobProgress: null,
        controllerConnectionChallenge: null,
        loading: false,
      });
      void get().loadRuntimeCapabilities();
      notifySuccess(`Connected to ${endpointName}`);
    } catch (e) {
      const msg = String(e);
      if (isAlreadyConnectedError(e)) {
        await get().hydrateSession();
        set({ error: null, loading: false, controllerConnectionChallenge: null });
        notifySuccess('Using existing machine connection');
        return;
      }
      set({ error: msg, loading: false });
      notifyError(msg);
    }
  },

  setControllerSelection: (controllerSelection) => set({ controllerSelection }),

  continueControllerConnection: async (decision) => {
    const challenge = get().controllerConnectionChallenge;
    if (!challenge) return;
    set({ loading: true, error: null });
    try {
      const result = await machineService.continueControllerConnection(
        challenge.attempt_id,
        get().controllerSelection,
        decision,
      );
      if (result.status === 'challenge') {
        set({ controllerConnectionChallenge: result, loading: false });
        return;
      }
      if (result.status === 'cancelled') {
        set({
          sessionState: 'disconnected',
          connectedPort: null,
          machineStatus: null,
          machineCoordinatesValid: false,
          controllerConnectionChallenge: null,
          loading: false,
        });
        return;
      }
      let status = null;
      try {
        status = await machineService.getMachineStatus();
      } catch {
        // The polling hook will retry; connection itself already succeeded.
      }
      const sessionState = status?.run_state === 'alarm' ? 'alarm' : result.session_state;
      set({
        sessionState,
        connectedPort: controllerEndpointDisplayName(result.endpoint),
        machineStatus: status,
        machineCoordinatesValid: false,
        jobProgress: null,
        controllerConnectionChallenge: null,
        loading: false,
      });
      void get().loadRuntimeCapabilities();
      notifySuccess(`Connected to ${controllerEndpointDisplayName(result.endpoint)}`);
    } catch (e) {
      const msg = String(e);
      const challengeExpired = msg.toLowerCase().includes('decision expired')
        || msg.toLowerCase().includes('no controller connection decision is pending');
      set({
        error: msg,
        loading: false,
        ...(challengeExpired ? { controllerConnectionChallenge: null } : {}),
      });
      notifyError(msg);
    }
  },

  disconnect: async () => {
    if (get().connectionPreview) {
      set({
        connectionPreview: false,
        sessionState: 'disconnected',
        machineStatus: null,
        machineCoordinatesValid: false,
        connectedPort: null,
        controllerConnectionChallenge: null,
        capabilities: null,
        jobProgress: null,
        error: null,
      });
      notifySuccess('Preview machine disconnected');
      return;
    }
    try {
      await machineService.disconnect();
      set({
        sessionState: 'disconnected',
        machineStatus: null,
        machineCoordinatesValid: false,
        connectedPort: null,
        controllerConnectionChallenge: null,
        capabilities: null,
        jobProgress: null,
        error: null,
      });
      notifySuccess('Disconnected');
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    }
  },

  loadRuntimeCapabilities: async () => {
    if (get().connectionPreview) {
      set({ capabilities: PREVIEW_CAPABILITIES });
      return;
    }
    try {
      const runtime = await machineService.getMachineRuntimeState();
      set({ capabilities: runtime.capabilities ?? null });
    } catch {
      // Fail closed: unknown capabilities keep capability-gated controls off.
      set({ capabilities: null });
    }
  },

  setConnectionPreview: (enabled) => {
    if (enabled) {
      set({
        connectionPreview: true,
        sessionState: 'ready',
        machineStatus: makePreviewMachineStatus(),
        machineCoordinatesValid: true,
        connectedPort: PREVIEW_CONNECTED_PORT,
        controllerConnectionChallenge: null,
        capabilities: PREVIEW_CAPABILITIES,
        jobProgress: null,
        loading: false,
        error: null,
      });
      notifySuccess('Preview machine connected');
      return;
    }
    set({
      connectionPreview: false,
      sessionState: 'disconnected',
      machineStatus: null,
      machineCoordinatesValid: false,
      connectedPort: null,
      controllerConnectionChallenge: null,
      capabilities: null,
      jobProgress: null,
      error: null,
    });
    notifySuccess('Preview machine disconnected');
  },

  refreshStatus: async () => {
    if (get().connectionPreview) {
      set({
        sessionState: 'ready',
        machineStatus: get().machineStatus ?? makePreviewMachineStatus(),
        machineCoordinatesValid: true,
        error: null,
      });
      return;
    }
    try {
      const [status, machineCoordinatesValid] = await Promise.all([
        machineService.getMachineStatus(),
        machineService.getMachineCoordinatesValid().catch(() => false),
      ]);
      const currentState = get().sessionState;
      const sessionState =
        status.run_state === 'alarm' ? 'alarm'
        : currentState === 'alarm' && status.run_state === 'idle' ? 'ready'
        : currentState;
      set({ machineStatus: status, machineCoordinatesValid, sessionState, error: null });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  refreshSessionState: async () => {
    if (get().connectionPreview) {
      set({ sessionState: 'ready', machineCoordinatesValid: true, error: null });
      return;
    }
    try {
      const state = await machineService.getSessionState();
      set({ sessionState: state, error: null });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  hydrateSession: async () => {
    if (get().connectionPreview) {
      set({
        sessionState: 'ready',
        machineStatus: get().machineStatus ?? makePreviewMachineStatus(),
        machineCoordinatesValid: true,
        connectedPort: PREVIEW_CONNECTED_PORT,
        error: null,
      });
      return;
    }
    try {
      const state = await machineService.getSessionState();
      const activeStates: SessionState[] = ['ready', 'running', 'paused', 'alarm'];
      let status: MachineStatus | null = null;
      let machineCoordinatesValid = false;
      if (activeStates.includes(state)) {
        try {
          status = await machineService.getMachineStatus();
          machineCoordinatesValid = await machineService.getMachineCoordinatesValid();
        } catch {
          // Polling will retry once the UI has caught up to the backend session.
        }
      }
      const sessionState = status?.run_state === 'alarm' ? 'alarm' : state;
      set({
        sessionState,
        machineStatus: status,
        machineCoordinatesValid,
        connectedPort: activeStates.includes(sessionState) ? get().connectedPort : null,
        jobProgress: activeStates.includes(sessionState) ? get().jobProgress : null,
        capabilities: activeStates.includes(sessionState) ? get().capabilities : null,
        error: null,
      });
      if (activeStates.includes(sessionState)) {
        void get().loadRuntimeCapabilities();
      }
    } catch (e) {
      set({ error: String(e) });
    }
  },

  home: async () => {
    try {
      await machineService.home();
      await get().refreshStatus();
      await get().refreshSessionState();
      set({ error: null });
      notifySuccess('Homing started');
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    }
  },

  unlock: async () => {
    try {
      await machineService.unlock();
      await get().refreshStatus();
      await get().refreshSessionState();
      set({ error: null });
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    }
  },

  jog: async (xMm, yMm, feedRate, zMm = null, continuous = false) => {
    try {
      await machineService.jog(xMm, yMm, feedRate, zMm, continuous);
      await get().refreshStatus();
      await get().refreshSessionState();
      set({ error: null });
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    }
  },

  runPreflight: async () => {
    // Clear any prior report so a failed request can never fall through to Start
    // on stale data (laser safety).
    set({ loading: true, error: null, preflightReport: null });
    try {
      const report = await machineService.runPreflightCheck(
        sessionJobOptions(useUiStore.getState().jobOptions, useProjectStore.getState().selectedObjectIds),
      );
      set({ preflightReport: report, loading: false });
      return report;
    } catch (e) {
      const msg = String(e);
      set({ error: msg, loading: false, preflightReport: null });
      notifyError(msg);
      return null;
    }
  },

  startJob: async () => {
    set({ loading: true, error: null });
    try {
      await useProjectStore.getState().advanceAutoVariableText();
      const progress = await machineService.startJob(
        sessionJobOptions(useUiStore.getState().jobOptions, useProjectStore.getState().selectedObjectIds),
      );
      set({ jobProgress: progress, loading: false });
    } catch (e) {
      const msg = String(e);
      set({ error: msg, loading: false });
      notifyError(msg);
    }
  },

  frameJob: async (frameMode, selectedObjectIds, laserOnOverride = false) => {
    set({ loading: true, error: null });
    try {
      const progress = await machineService.frameJob(
        frameMode,
        selectedObjectIds,
        laserOnOverride,
        useUiStore.getState().moveWindowJogFeedRateMmMin,
      );
      set({ jobProgress: progress, loading: false });
      useNotificationStore
        .getState()
        .push(
          laserOnOverride
            ? i18n.t('notifications.framing_started_laser_on')
            : i18n.t('notifications.framing_started'),
          'info',
        );
      return progress;
    } catch (e) {
      const msg = String(e);
      set({ error: msg, loading: false });
      notifyError(msg);
      return null;
    }
  },

  refreshJobProgress: async () => {
    try {
      const progress = await machineService.getJobProgress();
      set({ jobProgress: progress, error: null });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  pauseJob: async () => {
    try {
      await machineService.pauseJob();
      set({ error: null });
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    }
  },

  resumeJob: async () => {
    try {
      await machineService.resumeJob();
      set({ error: null });
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    }
  },

  cancelJob: async () => {
    try {
      await machineService.cancelJob();
      // Keep jobProgress: polling picks up the cancelled terminal state and
      // useMachinePolling clears it after the 3s terminal display window.
      set({ error: null });
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    }
  },

  loadProfiles: async () => {
    try {
      const profiles = await machineService.getMachineProfiles();
      let activeProfileId = get().activeProfileId;
      try {
        const settings = await appService.getSettings();
        activeProfileId = settings.active_profile_id ?? null;
      } catch {
        if (activeProfileId && !profiles.some((profile) => profile.id === activeProfileId)) {
          activeProfileId = null;
        }
      }
      set({
        profiles,
        activeProfileId,
        error: null,
      });
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    }
  },

  saveProfile: async (profile) => {
    const previousActiveProfileId = get().activeProfileId;
    suppressProfileEvent('profile.saved', profile.id);
    try {
      await machineService.saveMachineProfile(profile);
      const profiles = await machineService.getMachineProfiles();
      let activeProfileId = get().activeProfileId;
      try {
        const settings = await appService.getSettings();
        activeProfileId = settings.active_profile_id ?? null;
      } catch {
        if (activeProfileId && !profiles.some((existing) => existing.id === activeProfileId)) {
          activeProfileId = null;
        }
      }
      set({
        profiles,
        activeProfileId,
        error: null,
      });
      if (
        previousActiveProfileId !== activeProfileId ||
        previousActiveProfileId === profile.id ||
        activeProfileId === profile.id
      ) {
        if (activeProfileId === profile.id && useProjectStore.getState().project) {
          await useProjectStore.getState().bindMachineProfile();
        }
        invalidateMachinePreview();
      }
    } catch (e) {
      releaseSuppressedProfileEvent('profile.saved', profile.id);
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
      throw e;
    }
  },

  deleteProfile: async (profileId) => {
    const previousActiveProfileId = get().activeProfileId;
    suppressProfileEvent('profile.deleted', profileId);
    try {
      await machineService.deleteMachineProfile(profileId);
      const profiles = await machineService.getMachineProfiles();
      let activeProfileId = get().activeProfileId;
      try {
        const settings = await appService.getSettings();
        activeProfileId = settings.active_profile_id ?? null;
      } catch {
        if (activeProfileId && !profiles.some((profile) => profile.id === activeProfileId)) {
          activeProfileId = null;
        }
      }
      set({
        profiles,
        activeProfileId,
        error: null,
      });
      if (previousActiveProfileId !== activeProfileId || previousActiveProfileId === profileId) {
        invalidateMachinePreview();
      }
    } catch (e) {
      releaseSuppressedProfileEvent('profile.deleted', profileId);
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
      throw e;
    }
  },

  setActiveProfile: async (profileId) => {
    suppressProfileEvent(
      profileId === null ? 'profile.deactivated' : 'profile.activated',
      profileId,
    );
    try {
      await machineService.setActiveProfile(profileId);
      set({ activeProfileId: profileId, error: null });
      if (profileId !== null && useProjectStore.getState().project) {
        await useProjectStore.getState().bindMachineProfile();
      }
      await Promise.all([
        useCameraStore.getState().refreshDevices(),
        useCameraStore.getState().refreshOverlayState(),
        useCameraStore.getState().refreshCalibration(),
        useCameraStore.getState().refreshAlignment(),
      ]);
      invalidateMachinePreview();
    } catch (e) {
      releaseSuppressedProfileEvent(
        profileId === null ? 'profile.deactivated' : 'profile.activated',
        profileId,
      );
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    }
  },

  refreshDiscoveryState: async () => {
    try {
      const discoveryState = await discoveryService.getDiscoveryState();
      set({ discoveryState, error: null });
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    }
  },

  startDiscovery: async () => {
    set({ loading: true, error: null });
    try {
      const discoveryState = await discoveryService.startDiscovery();
      set({ discoveryState, loading: false });
      notifySuccess('Machine discovery started');
    } catch (e) {
      const msg = String(e);
      set({ error: msg, loading: false });
      notifyError(msg);
    }
  },

  cancelDiscovery: async () => {
    try {
      const discoveryState = await discoveryService.cancelDiscovery();
      set({ discoveryState, error: null });
      notifySuccess('Machine discovery cancelled');
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    }
  },

  connectCandidate: async (candidateId) => {
    set({ loading: true, error: null });
    try {
      const state = await discoveryService.connectCandidate(candidateId);
      const candidate = get().discoveryState.candidates.find((item) => item.id === candidateId);
      set({
        sessionState: state,
        connectedPort: candidate?.identity.port_name ?? null,
        machineCoordinatesValid: false,
        loading: false,
      });
      void get().loadRuntimeCapabilities();
      notifySuccess('Connected to discovered device');
    } catch (e) {
      const msg = String(e);
      set({ error: msg, loading: false });
      notifyError(msg);
    }
  },

  bootstrapProfileFromCandidate: async (candidateId, profileName) => {
    set({ loading: true, error: null });
    try {
      const profile = await discoveryService.bootstrapProfile(candidateId, profileName, true);
      const profiles = await machineService.getMachineProfiles();
      set({ profiles, activeProfileId: profile.id, loading: false });
      invalidateMachinePreview();
      notifySuccess('Machine profile created from discovery candidate');
    } catch (e) {
      const msg = String(e);
      set({ error: msg, loading: false });
      notifyError(msg);
    }
  },

  emergencyStop: async () => {
    try {
      await machineService.emergencyStop();
      set({ jobProgress: null, machineCoordinatesValid: false, error: null });
      notifySuccess('Emergency stop sent');
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    }
  },

  setWorkOrigin: async () => {
    try {
      const pos = await machineService.setWorkOrigin();
      set({ error: null });
      // Backend stored user_origin + pushed undo snapshot; sync local state + invalidate
      const { useProjectStore } = await import('./projectStore');
      const project = useProjectStore.getState().project;
      if (project) {
        useProjectStore.setState({
          project: { ...project, user_origin: pos, dirty: true },
        });
        const { usePreviewStore } = await import('./previewStore');
        usePreviewStore.getState().invalidate();
        const { useUndoStore } = await import('./undoStore');
        await useUndoStore.getState().refresh();
      }
      notifySuccess(`User origin set to (${pos[0].toFixed(1)}, ${pos[1].toFixed(1)})`);
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    }
  },

  resetWorkOrigin: async () => {
    try {
      await machineService.resetWorkOrigin();
      set({ error: null });
      // Clear user_origin on the project store, invalidate preview, refresh undo
      const { useProjectStore } = await import('./projectStore');
      const project = useProjectStore.getState().project;
      if (project) {
        useProjectStore.setState({
          project: { ...project, user_origin: null, dirty: true },
        });
      }
      const { usePreviewStore } = await import('./previewStore');
      usePreviewStore.getState().invalidate();
      const { useUndoStore } = await import('./undoStore');
      await useUndoStore.getState().refresh();
      notifySuccess('User origin cleared');
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    }
  },

  setFrameSelectedOnly: (val) => set({ frameSelectedOnly: val }),

  openPreflightDialog: () => set({ showPreflightDialog: true }),
  closePreflightDialog: () => set({ showPreflightDialog: false }),
}));
