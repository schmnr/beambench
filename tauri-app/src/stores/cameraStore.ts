import i18n from '../i18n';
import { create } from 'zustand';
import type {
  AlignmentPointSet,
  CalibrationPointSet,
  CameraAgentState,
  CameraAlignment,
  CameraCalibration,
  CameraDeviceInfo,
  CameraOverlayState,
  SimilarityTransform,
} from '../types/camera';
import type { Workspace } from '../types/project';
import { fitCameraOverlayToWorkspace } from '../canvas/cameraOverlay';
import {
  captureBrowserCameraFrame,
  disposeBrowserCameraSession,
  type BrowserCameraCaptureStage,
} from '../services/browserCameraCapture';
import { cameraService } from '../services/cameraService';
import { useNotificationStore } from './notificationStore';
import { useMachineStore } from './machineStore';
import { wrapBackendError } from '../i18n/errors';

const notifyError = (msg: string) => useNotificationStore.getState().push(wrapBackendError(msg), 'error');
const notifySuccess = (msg: string) => useNotificationStore.getState().push(msg, 'success');

export type CameraCaptureStage =
  | 'idle'
  | BrowserCameraCaptureStage
  | 'saving'
  | 'success'
  | 'error';

interface CameraStoreState {
  devices: CameraDeviceInfo[];
  selectedCameraId: string | null;
  overlayState: CameraOverlayState | null;
  overlayVisible: boolean;
  overlayOpacity: number;
  draftOverlayTransform: SimilarityTransform | null;
  draftOverlayBaseTransform: SimilarityTransform | null;
  overlayAdjustMode: boolean;
  overlayDraftDirty: boolean;
  calibration: CameraCalibration | null;
  alignment: CameraAlignment | null;
  loading: boolean;
  captureStage: CameraCaptureStage;
  error: string | null;
  refreshDevices: () => Promise<void>;
  selectCamera: (cameraId: string | null) => Promise<void>;
  refreshOverlayState: () => Promise<void>;
  setOverlayVisible: (visible: boolean) => void;
  toggleOverlayVisible: () => void;
  setOverlayOpacity: (opacity: number) => void;
  refreshCalibration: () => Promise<void>;
  refreshAlignment: () => Promise<void>;
  captureFrame: (workspace?: Workspace | null) => Promise<void>;
  beginOverlayAdjust: (workspace?: Workspace | null) => void;
  exitOverlayAdjust: () => void;
  fitDraftOverlayToWorkspace: (workspace?: Workspace | null) => void;
  setDraftOverlayTransform: (transform: SimilarityTransform, dirty?: boolean) => void;
  commitDraftOverlayTransform: () => Promise<void>;
  saveDraftAlignment: () => Promise<void>;
  discardDraftOverlay: () => void;
  solveCalibration: (cameraId: string, points: CalibrationPointSet) => Promise<CameraCalibration>;
  saveCalibration: (cameraId: string, calibration: CameraCalibration) => Promise<void>;
  resetCalibration: () => Promise<boolean>;
  solveAlignment: (points: AlignmentPointSet) => Promise<CameraAlignment>;
  saveAlignment: (alignment: CameraAlignment) => Promise<void>;
  resetAlignment: () => Promise<boolean>;
}

function applyAgentState(state: CameraAgentState) {
  return {
    overlayState: {
      selected_camera_id: state.selected_camera_id,
      frame: state.frame,
      calibration: state.calibration,
      alignment: state.alignment,
      overlay_ready: state.overlay_ready,
    },
    selectedCameraId: state.selected_camera_id,
    calibration: state.calibration,
    alignment: state.alignment,
    overlayVisible: state.display.overlay_visible,
    overlayOpacity: state.display.overlay_opacity,
    draftOverlayTransform: state.display.draft_overlay_transform,
    draftOverlayBaseTransform: state.display.draft_base_transform,
    overlayAdjustMode: state.display.overlay_adjust_mode,
    overlayDraftDirty: state.display.draft_dirty,
    error: null,
  };
}

/**
 * Bumped when a camera is selected. Camera replies that started before a newer
 * selection belong to the previous camera and are discarded.
 */
let selectionGeneration = 0;

function captureCameraSelection(): () => boolean {
  const generation = selectionGeneration;
  const cameraId = useCameraStore.getState().selectedCameraId;
  const profileId = useMachineStore.getState().activeProfileId;
  return () => generation === selectionGeneration
    && cameraId === useCameraStore.getState().selectedCameraId
    && profileId === useMachineStore.getState().activeProfileId;
}

export const useCameraStore = create<CameraStoreState>((set, get) => ({
  devices: [],
  selectedCameraId: null,
  overlayState: null,
  // An overlay should become active only after the user shows one or a camera
  // workflow produces a frame. Starting enabled makes the toolbar look active
  // even when there is nothing available to render.
  overlayVisible: false,
  overlayOpacity: 0.4,
  draftOverlayTransform: null,
  draftOverlayBaseTransform: null,
  overlayAdjustMode: false,
  overlayDraftDirty: false,
  calibration: null,
  alignment: null,
  loading: false,
  captureStage: 'idle',
  error: null,

  refreshDevices: async () => {
    try {
      const devices = await cameraService.listDevices();
      set({ devices, error: null });
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    }
  },

  selectCamera: async (cameraId) => {
    const generation = ++selectionGeneration;
    const profileId = useMachineStore.getState().activeProfileId;
    const isCurrent = () => generation === selectionGeneration
      && profileId === useMachineStore.getState().activeProfileId;
    try {
      if (cameraId !== get().selectedCameraId) {
        disposeBrowserCameraSession();
      }
      const selectedCameraId = await cameraService.selectCamera(cameraId);
      if (!isCurrent()) return;
      set({
        selectedCameraId,
        draftOverlayTransform: null,
        draftOverlayBaseTransform: null,
        overlayAdjustMode: false,
        overlayDraftDirty: false,
        captureStage: 'idle',
        loading: false,
        error: null,
      });
      await get().refreshOverlayState();
      if (!isCurrent()) return;
      await get().refreshCalibration();
      if (!isCurrent()) return;
      await get().refreshAlignment();
    } catch (e) {
      if (!isCurrent()) return;
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    }
  },

  refreshOverlayState: async () => {
    const isCurrent = captureCameraSelection();
    try {
      const state = await cameraService.getAgentState();
      if (!isCurrent()) return;
      set(applyAgentState(state));
    } catch (e) {
      if (!isCurrent()) return;
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    }
  },

  setOverlayVisible: (overlayVisible) => {
    if (useMachineStore.getState().activeProfileId === null) return;
    const isCurrent = captureCameraSelection();
    void cameraService.updateOverlayDisplay({ overlayVisible })
      .then((state) => {
        if (!isCurrent()) return;
        set(applyAgentState(state));
      })
      .catch((e) => {
        if (!isCurrent()) return;
        const msg = String(e);
        set({ error: msg });
        notifyError(msg);
      });
  },

  toggleOverlayVisible: () => {
    const overlayVisible = !get().overlayVisible;
    get().setOverlayVisible(overlayVisible);
  },

  setOverlayOpacity: (opacity) => {
    if (useMachineStore.getState().activeProfileId === null) return;
    const isCurrent = captureCameraSelection();
    const overlayOpacity = Math.max(0, Math.min(1, opacity));
    void cameraService.updateOverlayDisplay({ overlayOpacity })
      .then((state) => {
        if (!isCurrent()) return;
        set(applyAgentState(state));
      })
      .catch((e) => {
        if (!isCurrent()) return;
        const msg = String(e);
        set({ error: msg });
        notifyError(msg);
      });
  },

  refreshCalibration: async () => {
    const isCurrent = captureCameraSelection();
    try {
      const calibration = await cameraService.getCalibration(get().selectedCameraId);
      if (!isCurrent()) return;
      set({ calibration, error: null });
    } catch (e) {
      if (!isCurrent()) return;
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    }
  },

  refreshAlignment: async () => {
    const isCurrent = captureCameraSelection();
    try {
      const alignment = await cameraService.getAlignment();
      if (!isCurrent()) return;
      set({ alignment, error: null });
    } catch (e) {
      if (!isCurrent()) return;
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    }
  },

  captureFrame: async (workspace) => {
    const isCurrent = captureCameraSelection();
    if (get().loading) return;
    void workspace;
    set({ loading: true, captureStage: 'resolving', error: null });
    try {
      const selectedCameraId = get().selectedCameraId;
      const devices = get().devices;
      const selectedDevice = devices.find((device) => device.camera_id === selectedCameraId);
      if (selectedCameraId && selectedDevice?.backend_kind === 'native') {
        const frame = await captureBrowserCameraFrame(selectedDevice, devices, {
          onStage: (captureStage) => { if (isCurrent()) set({ captureStage }); },
        });
        if (!isCurrent()) return;
        set({ captureStage: 'saving' });
        await cameraService.saveFrame(
          selectedCameraId,
          frame.imageData,
          frame.widthPx,
          frame.heightPx,
          frame.mediaType,
        );
        if (!isCurrent()) return;
      } else {
        set({ captureStage: 'capturing' });
        await cameraService.captureFrame(selectedCameraId);
        if (!isCurrent()) return;
      }
      set({ captureStage: 'saving' });
      const state = await cameraService.getAgentState();
      if (!isCurrent()) return;
      set({ ...applyAgentState(state), loading: false, captureStage: 'success' });
      notifySuccess(i18n.t('notifications.camera.image_captured'));
    } catch (e) {
      if (!isCurrent()) return;
      const msg = e instanceof Error ? e.message : String(e);
      set({ error: msg, loading: false, captureStage: 'error' });
      notifyError(msg);
    }
  },

  beginOverlayAdjust: (workspace) => {
    const isCurrent = captureCameraSelection();
    const state = get();
    const frame = state.overlayState?.frame ?? null;
    if (!frame) return;
    const savedTransform = state.alignment?.transform
      ?? state.overlayState?.alignment?.transform
      ?? state.calibration?.transform
      ?? state.overlayState?.calibration?.transform
      ?? null;
    const draft = state.draftOverlayTransform
      ?? savedTransform
      ?? (workspace
        ? fitCameraOverlayToWorkspace(frame.width_px, frame.height_px, workspace)
        : null);
    if (!draft) return;
    set({
      draftOverlayTransform: draft,
      draftOverlayBaseTransform: savedTransform ?? state.draftOverlayBaseTransform ?? draft,
      overlayAdjustMode: true,
      overlayVisible: true,
      overlayDraftDirty: savedTransform ? false : state.overlayDraftDirty,
    });
    void cameraService.updateOverlayDisplay({
      overlayVisible: true,
      overlayAdjustMode: true,
    }).then((agentState) => {
        if (!isCurrent()) return;
      const current = get();
      set({
        ...applyAgentState(agentState),
        draftOverlayTransform: current.draftOverlayTransform,
        draftOverlayBaseTransform: current.draftOverlayBaseTransform,
        overlayAdjustMode: current.overlayAdjustMode,
        overlayDraftDirty: current.overlayDraftDirty,
      });
    }).catch((e) => {
        if (!isCurrent()) return;
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    });
  },

  exitOverlayAdjust: () => {
    const isCurrent = captureCameraSelection();
    const state = get();
    const savedTransform = state.alignment?.transform
      ?? state.overlayState?.alignment?.transform
      ?? state.calibration?.transform
      ?? state.overlayState?.calibration?.transform
      ?? null;
    if (savedTransform) {
      set({
        draftOverlayTransform: null,
        draftOverlayBaseTransform: null,
        overlayAdjustMode: false,
        overlayDraftDirty: false,
      });
      void cameraService.updateOverlayDisplay({ overlayAdjustMode: false })
        .then((agentState) => { if (isCurrent()) set(applyAgentState(agentState)); })
        .catch((e) => {
        if (!isCurrent()) return;
          const msg = String(e);
          set({ error: msg });
          notifyError(msg);
        });
      return;
    }
    set({
      draftOverlayTransform: state.draftOverlayBaseTransform,
      overlayAdjustMode: false,
      overlayDraftDirty: false,
    });
    void cameraService.discardOverlayDraft()
      .then((agentState) => { if (isCurrent()) set(applyAgentState(agentState)); })
      .catch((e) => {
        if (!isCurrent()) return;
        const msg = String(e);
        set({ error: msg });
        notifyError(msg);
      });
  },

  fitDraftOverlayToWorkspace: (workspace) => {
    const isCurrent = captureCameraSelection();
    const state = get();
    const frame = state.overlayState?.frame ?? null;
    if (!frame || !workspace) return;
    const savedTransform = state.alignment?.transform
      ?? state.overlayState?.alignment?.transform
      ?? state.calibration?.transform
      ?? state.overlayState?.calibration?.transform
      ?? null;
    const fitted = fitCameraOverlayToWorkspace(frame.width_px, frame.height_px, workspace);
    set({
      draftOverlayTransform: fitted,
      draftOverlayBaseTransform: savedTransform ?? fitted,
      overlayAdjustMode: true,
      overlayDraftDirty: Boolean(savedTransform),
      overlayVisible: true,
    });
    void cameraService.fitOverlayToBed()
      .then((agentState) => { if (isCurrent()) set(applyAgentState(agentState)); })
      .catch((e) => {
        if (!isCurrent()) return;
        const msg = String(e);
        set({ error: msg });
        notifyError(msg);
      });
  },

  setDraftOverlayTransform: (transform, dirty = true) => {
    set({
      draftOverlayTransform: transform,
      overlayDraftDirty: dirty,
      overlayVisible: true,
    });
  },

  commitDraftOverlayTransform: async () => {
    const isCurrent = captureCameraSelection();
    const transform = get().draftOverlayTransform;
    if (!transform) return;
    try {
      const state = await cameraService.commitOverlayTransform(transform);
      if (!isCurrent()) return;
      set(applyAgentState(state));
    } catch (e) {
      if (!isCurrent()) return;
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
      throw e;
    }
  },

  saveDraftAlignment: async () => {
    const isCurrent = captureCameraSelection();
    const transform = get().draftOverlayTransform;
    if (!transform) return;
    await get().commitDraftOverlayTransform();
    if (!isCurrent()) return;
    try {
      const state = await cameraService.saveOverlayAlignment();
      if (!isCurrent()) return;
      set(applyAgentState(state));
      notifySuccess(i18n.t('notifications.camera.alignment_saved'));
    } catch (e) {
      if (!isCurrent()) return;
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
      throw e;
    }
  },

  discardDraftOverlay: () => {
    const isCurrent = captureCameraSelection();
    if (get().alignment ?? get().overlayState?.alignment) return;
    set({
      draftOverlayTransform: null,
      draftOverlayBaseTransform: null,
      overlayAdjustMode: false,
      overlayDraftDirty: false,
    });
    void cameraService.discardOverlayDraft()
      .then((agentState) => { if (isCurrent()) set(applyAgentState(agentState)); })
      .catch((e) => {
        if (!isCurrent()) return;
        const msg = String(e);
        set({ error: msg });
        notifyError(msg);
      });
  },

  solveCalibration: async (cameraId, points) => {
    const result = await cameraService.solveCalibration(cameraId, points);
    set({ error: null });
    notifySuccess(i18n.t('notifications.camera.mapping_solved'));
    return result.calibration;
  },

  saveCalibration: async (cameraId, calibration) => {
    const isCurrent = captureCameraSelection();
    try {
      const saved = await cameraService.saveCalibration(cameraId, calibration);
      if (!isCurrent()) return;
      set({ calibration: saved, error: null });
      await get().refreshOverlayState();
      if (!isCurrent()) return;
      notifySuccess(i18n.t('notifications.camera.mapping_saved'));
    } catch (e) {
      if (!isCurrent()) return;
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
      throw e;
    }
  },

  resetCalibration: async () => {
    const isCurrent = captureCameraSelection();
    try {
      await cameraService.resetCalibration(get().selectedCameraId);
      if (!isCurrent()) return false;
      set({ calibration: null, error: null });
      await get().refreshOverlayState();
      if (!isCurrent()) return false;
      notifySuccess(i18n.t('notifications.camera.mapping_reset'));
      return true;
    } catch (e) {
      if (!isCurrent()) return false;
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
      return false;
    }
  },

  solveAlignment: async (points) => {
    const alignment = await cameraService.solveAlignment(points, get().selectedCameraId);
    set({ error: null });
    notifySuccess(i18n.t('notifications.camera.alignment_solved'));
    return alignment;
  },

  saveAlignment: async (alignment) => {
    const isCurrent = captureCameraSelection();
    try {
      const saved = await cameraService.updateAlignment(alignment, get().selectedCameraId);
      if (!isCurrent()) return;
      set({
        alignment: saved,
        draftOverlayTransform: null,
        draftOverlayBaseTransform: null,
        overlayAdjustMode: false,
        overlayDraftDirty: false,
        error: null,
      });
      await get().refreshOverlayState();
      if (!isCurrent()) return;
      notifySuccess(i18n.t('notifications.camera.alignment_saved'));
    } catch (e) {
      if (!isCurrent()) return;
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
      throw e;
    }
  },

  resetAlignment: async () => {
    const isCurrent = captureCameraSelection();
    try {
      await cameraService.resetAlignment(get().selectedCameraId);
      if (!isCurrent()) return false;
      set({
        alignment: null,
        draftOverlayTransform: null,
        draftOverlayBaseTransform: null,
        overlayAdjustMode: false,
        overlayDraftDirty: false,
        error: null,
      });
      await get().refreshOverlayState();
      if (!isCurrent()) return false;
      notifySuccess(i18n.t('notifications.camera.alignment_reset'));
      return true;
    } catch (e) {
      if (!isCurrent()) return false;
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
      return false;
    }
  },
}));
