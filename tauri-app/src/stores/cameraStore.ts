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
import { captureBrowserCameraFrame } from '../services/browserCameraCapture';
import { cameraService } from '../services/cameraService';
import { useNotificationStore } from './notificationStore';
import { wrapBackendError } from '../i18n/errors';

const notifyError = (msg: string) => useNotificationStore.getState().push(wrapBackendError(msg), 'error');
const notifySuccess = (msg: string) => useNotificationStore.getState().push(msg, 'success');

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

export const useCameraStore = create<CameraStoreState>((set, get) => ({
  devices: [],
  selectedCameraId: null,
  overlayState: null,
  overlayVisible: true,
  overlayOpacity: 0.4,
  draftOverlayTransform: null,
  draftOverlayBaseTransform: null,
  overlayAdjustMode: false,
  overlayDraftDirty: false,
  calibration: null,
  alignment: null,
  loading: false,
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
    try {
      const selectedCameraId = await cameraService.selectCamera(cameraId);
      set({
        selectedCameraId,
        draftOverlayTransform: null,
        draftOverlayBaseTransform: null,
        overlayAdjustMode: false,
        overlayDraftDirty: false,
        error: null,
      });
      await get().refreshOverlayState();
      await get().refreshCalibration();
      await get().refreshAlignment();
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    }
  },

  refreshOverlayState: async () => {
    try {
      const state = await cameraService.getAgentState();
      set(applyAgentState(state));
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    }
  },

  setOverlayVisible: (overlayVisible) => {
    set({ overlayVisible });
    void cameraService.updateOverlayDisplay({ overlayVisible })
      .then((state) => set(applyAgentState(state)))
      .catch((e) => {
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
    const overlayOpacity = Math.max(0, Math.min(1, opacity));
    set({ overlayOpacity });
    void cameraService.updateOverlayDisplay({ overlayOpacity })
      .then((state) => set(applyAgentState(state)))
      .catch((e) => {
        const msg = String(e);
        set({ error: msg });
        notifyError(msg);
      });
  },

  refreshCalibration: async () => {
    try {
      const calibration = await cameraService.getCalibration(get().selectedCameraId);
      set({ calibration, error: null });
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    }
  },

  refreshAlignment: async () => {
    try {
      const alignment = await cameraService.getAlignment();
      set({ alignment, error: null });
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    }
  },

  captureFrame: async (workspace) => {
    void workspace;
    set({ loading: true, error: null });
    try {
      const selectedCameraId = get().selectedCameraId;
      const devices = get().devices;
      const selectedDevice = devices.find((device) => device.camera_id === selectedCameraId);
      if (selectedCameraId && selectedDevice?.backend_kind === 'native') {
        const frame = await captureBrowserCameraFrame(selectedDevice, devices);
        await cameraService.saveFrame(
          selectedCameraId,
          frame.imageData,
          frame.widthPx,
          frame.heightPx,
          frame.mediaType,
        );
      } else {
        await cameraService.captureFrame(selectedCameraId);
      }
      await get().refreshOverlayState();
      set({ loading: false });
      notifySuccess('Camera overlay updated');
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      set({ error: msg, loading: false });
      notifyError(msg);
    }
  },

  beginOverlayAdjust: (workspace) => {
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
      const current = get();
      set({
        ...applyAgentState(agentState),
        draftOverlayTransform: current.draftOverlayTransform,
        draftOverlayBaseTransform: current.draftOverlayBaseTransform,
        overlayAdjustMode: current.overlayAdjustMode,
        overlayDraftDirty: current.overlayDraftDirty,
      });
    }).catch((e) => {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
    });
  },

  exitOverlayAdjust: () => {
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
        .then((agentState) => set(applyAgentState(agentState)))
        .catch((e) => {
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
      .then((agentState) => set(applyAgentState(agentState)))
      .catch((e) => {
        const msg = String(e);
        set({ error: msg });
        notifyError(msg);
      });
  },

  fitDraftOverlayToWorkspace: (workspace) => {
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
      .then((agentState) => set(applyAgentState(agentState)))
      .catch((e) => {
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
    const transform = get().draftOverlayTransform;
    if (!transform) return;
    try {
      const state = await cameraService.commitOverlayTransform(transform);
      set(applyAgentState(state));
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
      throw e;
    }
  },

  saveDraftAlignment: async () => {
    const transform = get().draftOverlayTransform;
    if (!transform) return;
    await get().commitDraftOverlayTransform();
    try {
      const state = await cameraService.saveOverlayAlignment();
      set(applyAgentState(state));
      notifySuccess('Camera alignment saved');
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
      throw e;
    }
  },

  discardDraftOverlay: () => {
    if (get().alignment ?? get().overlayState?.alignment) return;
    set({
      draftOverlayTransform: null,
      draftOverlayBaseTransform: null,
      overlayAdjustMode: false,
      overlayDraftDirty: false,
    });
    void cameraService.discardOverlayDraft()
      .then((agentState) => set(applyAgentState(agentState)))
      .catch((e) => {
        const msg = String(e);
        set({ error: msg });
        notifyError(msg);
      });
  },

  solveCalibration: async (cameraId, points) => {
    const result = await cameraService.solveCalibration(cameraId, points);
    set({ error: null });
    notifySuccess('Camera mapping solved');
    return result.calibration;
  },

  saveCalibration: async (cameraId, calibration) => {
    try {
      const saved = await cameraService.saveCalibration(cameraId, calibration);
      set({ calibration: saved, error: null });
      await get().refreshOverlayState();
      notifySuccess('Camera mapping saved');
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
      throw e;
    }
  },

  resetCalibration: async () => {
    try {
      await cameraService.resetCalibration(get().selectedCameraId);
      set({ calibration: null, error: null });
      await get().refreshOverlayState();
      notifySuccess('Camera mapping reset');
      return true;
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
      return false;
    }
  },

  solveAlignment: async (points) => {
    const alignment = await cameraService.solveAlignment(points, get().selectedCameraId);
    set({ error: null });
    notifySuccess('Camera alignment solved');
    return alignment;
  },

  saveAlignment: async (alignment) => {
    try {
      const saved = await cameraService.updateAlignment(alignment, get().selectedCameraId);
      set({
        alignment: saved,
        draftOverlayTransform: null,
        draftOverlayBaseTransform: null,
        overlayAdjustMode: false,
        overlayDraftDirty: false,
        error: null,
      });
      await get().refreshOverlayState();
      notifySuccess('Camera alignment saved');
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
      throw e;
    }
  },

  resetAlignment: async () => {
    try {
      await cameraService.resetAlignment(get().selectedCameraId);
      set({
        alignment: null,
        draftOverlayTransform: null,
        draftOverlayBaseTransform: null,
        overlayAdjustMode: false,
        overlayDraftDirty: false,
        error: null,
      });
      await get().refreshOverlayState();
      notifySuccess('Camera alignment reset');
      return true;
    } catch (e) {
      const msg = String(e);
      set({ error: msg });
      notifyError(msg);
      return false;
    }
  },
}));
