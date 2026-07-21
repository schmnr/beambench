import { invoke } from '@tauri-apps/api/core';
import type {
  AlignmentPointSet,
  CalibrationPointSet,
  CalibrationSolveResult,
  CameraAgentState,
  CameraAlignment,
  CameraCalibration,
  CameraDeviceInfo,
  CameraFrameHandle,
  CameraOverlayState,
} from '../types/camera';

export const cameraService = {
  async listDevices(): Promise<CameraDeviceInfo[]> {
    return invoke<CameraDeviceInfo[]>('list_camera_devices');
  },

  async selectCamera(cameraId: string | null): Promise<string | null> {
    return invoke<string | null>('select_camera_device', { cameraId });
  },

  async getCalibration(cameraId?: string | null): Promise<CameraCalibration | null> {
    return invoke<CameraCalibration | null>('get_camera_calibration', { cameraId });
  },

  async solveCalibration(
    cameraId: string,
    points: CalibrationPointSet,
  ): Promise<CalibrationSolveResult> {
    return invoke<CalibrationSolveResult>('solve_camera_calibration', { cameraId, points });
  },

  async saveCalibration(
    cameraId: string,
    calibration: CameraCalibration,
  ): Promise<CameraCalibration> {
    return invoke<CameraCalibration>('save_camera_calibration', { cameraId, calibration });
  },

  async resetCalibration(cameraId?: string | null): Promise<void> {
    return invoke<void>('reset_camera_calibration', { cameraId });
  },

  async captureFrame(cameraId?: string | null): Promise<CameraFrameHandle> {
    return invoke<CameraFrameHandle>('capture_camera_frame', { cameraId });
  },

  async saveFrame(
    cameraId: string,
    imageData: Uint8Array,
    widthPx: number,
    heightPx: number,
    mediaType: string,
  ): Promise<CameraFrameHandle> {
    return invoke<CameraFrameHandle>('save_camera_frame_bytes', imageData, {
      headers: {
        'camera-id': cameraId,
        'width-px': String(widthPx),
        'height-px': String(heightPx),
        'media-type': mediaType,
      },
    });
  },

  async getAlignment(): Promise<CameraAlignment | null> {
    return invoke<CameraAlignment | null>('get_camera_alignment');
  },

  async solveAlignment(
    points: AlignmentPointSet,
    cameraId?: string | null,
  ): Promise<CameraAlignment> {
    return invoke<CameraAlignment>('solve_camera_alignment', { cameraId, points });
  },

  async updateAlignment(
    alignment: CameraAlignment,
    cameraId?: string | null,
  ): Promise<CameraAlignment> {
    return invoke<CameraAlignment>('update_camera_alignment', { alignment, cameraId });
  },

  async resetAlignment(cameraId?: string | null): Promise<void> {
    return invoke<void>('reset_camera_alignment', { cameraId });
  },

  async getOverlayState(): Promise<CameraOverlayState> {
    return invoke<CameraOverlayState>('get_camera_overlay_state');
  },

  async getAgentState(): Promise<CameraAgentState> {
    return invoke<CameraAgentState>('get_camera_agent_state');
  },

  async updateOverlayDisplay(input: {
    overlayVisible?: boolean;
    overlayOpacity?: number;
    overlayAdjustMode?: boolean;
  }): Promise<CameraAgentState> {
    return invoke<CameraAgentState>('update_camera_overlay_display', {
      overlayVisible: input.overlayVisible ?? null,
      overlayOpacity: input.overlayOpacity ?? null,
      overlayAdjustMode: input.overlayAdjustMode ?? null,
    });
  },

  async fitOverlayToBed(): Promise<CameraAgentState> {
    return invoke<CameraAgentState>('fit_camera_overlay_to_bed');
  },

  async discardOverlayDraft(): Promise<CameraAgentState> {
    return invoke<CameraAgentState>('discard_camera_overlay_draft');
  },

  async saveOverlayAlignment(): Promise<CameraAgentState> {
    return invoke<CameraAgentState>('save_camera_overlay_alignment');
  },

  async commitOverlayTransform(transform: CameraAlignment['transform']): Promise<CameraAgentState> {
    return invoke<CameraAgentState>('commit_camera_overlay_transform', { transform });
  },

  async registerAgentBridge(): Promise<void> {
    return invoke<void>('register_camera_agent_bridge');
  },

  async unregisterAgentBridge(): Promise<void> {
    return invoke<void>('unregister_camera_agent_bridge');
  },

  async completeCaptureRequest(
    requestId: string,
    frame: CameraFrameHandle | null,
    error: string | null,
  ): Promise<void> {
    return invoke<void>('complete_camera_capture_request', {
      requestId,
      frame,
      error,
    });
  },

  async completeOverlayRenderRequest(
    requestId: string,
    path: string | null,
    error: string | null,
  ): Promise<void> {
    return invoke<void>('complete_camera_overlay_render_request', {
      requestId,
      path,
      error,
    });
  },
};
