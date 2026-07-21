import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useCameraStore } from '../cameraStore';
import { cameraService } from '../../services/cameraService';
import type {
  CameraAgentState,
  CameraAlignment,
  CameraCalibration,
  CameraFrameHandle,
} from '../../types/camera';

vi.mock('../../services/browserCameraCapture', () => ({
  captureBrowserCameraFrame: vi.fn(),
}));

vi.mock('../../services/cameraService', () => ({
  cameraService: {
    listDevices: vi.fn(),
    selectCamera: vi.fn(),
    getOverlayState: vi.fn(),
    getAgentState: vi.fn(),
    updateOverlayDisplay: vi.fn(),
    fitOverlayToBed: vi.fn(),
    discardOverlayDraft: vi.fn(),
    saveOverlayAlignment: vi.fn(),
    commitOverlayTransform: vi.fn(),
    registerAgentBridge: vi.fn(),
    unregisterAgentBridge: vi.fn(),
    completeCaptureRequest: vi.fn(),
    completeOverlayRenderRequest: vi.fn(),
    getCalibration: vi.fn(),
    getAlignment: vi.fn(),
    captureFrame: vi.fn(),
    saveFrame: vi.fn(),
    solveCalibration: vi.fn(),
    saveCalibration: vi.fn(),
    resetCalibration: vi.fn(),
    solveAlignment: vi.fn(),
    updateAlignment: vi.fn(),
    resetAlignment: vi.fn(),
  },
}));

const initialState = useCameraStore.getState();

const frame: CameraFrameHandle = {
  handle_id: 'frame-1',
  file_path: '/tmp/beam-bench-camera/frame-1.png',
  width_px: 100,
  height_px: 50,
  media_type: 'image/png',
  captured_at: '2026-05-22T12:00:00Z',
};

const savedCalibration: CameraCalibration = {
  image_width_px: 100,
  image_height_px: 50,
  transform: {
    scale: 0.5,
    rotation_deg: 2,
    translation_x: 3,
    translation_y: 4,
  },
  rmse_px: 0.2,
  quality_score: 0.98,
  solved_at: '2026-05-22T12:00:00Z',
};

const solvedAlignment: CameraAlignment = {
  transform: {
    scale: 0.4,
    rotation_deg: 1,
    translation_x: 5,
    translation_y: 6,
  },
  rmse_mm: 0.1,
  quality_score: 0.99,
  solved_at: '2026-05-22T12:00:00Z',
  source: 'solved_points',
};

function agentState(overrides: Partial<CameraAgentState> = {}): CameraAgentState {
  const alignment = overrides.alignment ?? null;
  return {
    schema_version: 1,
    selected_camera_id: 'cam-1',
    frame,
    calibration: null,
    alignment,
    overlay_ready: Boolean(alignment),
    display: {
      overlay_visible: true,
      overlay_opacity: 0.4,
      draft_overlay_transform: alignment ? null : {
        scale: 2,
        rotation_deg: 0,
        translation_x: 0,
        translation_y: 0,
      },
      draft_base_transform: alignment ? null : {
        scale: 2,
        rotation_deg: 0,
        translation_x: 0,
        translation_y: 0,
      },
      draft_dirty: false,
      overlay_adjust_mode: false,
      effective_transform: alignment?.transform ?? {
        scale: 2,
        rotation_deg: 0,
        translation_x: 0,
        translation_y: 0,
      },
      status: alignment ? 'saved_alignment' : 'preview_fitted_to_bed',
    },
    latest_capture: null,
    latest_render: null,
    ...overrides,
  };
}

describe('cameraStore overlay display state', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(cameraService.updateOverlayDisplay).mockResolvedValue(agentState());
    vi.mocked(cameraService.getAgentState).mockResolvedValue(agentState());
    vi.mocked(cameraService.fitOverlayToBed).mockResolvedValue(agentState());
    vi.mocked(cameraService.discardOverlayDraft).mockResolvedValue(agentState({
      display: {
        ...agentState().display,
        draft_overlay_transform: null,
        draft_base_transform: null,
        draft_dirty: false,
        overlay_adjust_mode: false,
        effective_transform: null,
        status: 'no_alignment',
      },
    }));
    vi.mocked(cameraService.commitOverlayTransform).mockImplementation(async (transform) => agentState({
      display: {
        ...agentState().display,
        draft_overlay_transform: transform,
        effective_transform: transform,
        draft_dirty: true,
        status: 'unsaved_changes',
      },
    }));
    vi.mocked(cameraService.saveOverlayAlignment).mockResolvedValue(agentState({
      alignment: {
        transform: {
          scale: 1.5,
          rotation_deg: 5,
          translation_x: 10,
          translation_y: 12,
        },
        rmse_mm: 0,
        quality_score: 1,
        solved_at: '2026-05-22T12:00:00Z',
        source: 'manual_adjust',
      },
      display: {
        ...agentState().display,
        draft_overlay_transform: null,
        draft_base_transform: null,
        draft_dirty: false,
        overlay_adjust_mode: false,
        status: 'saved_alignment',
      },
    }));
  });

  afterEach(() => {
    useCameraStore.setState(initialState, true);
  });

  it('keeps visibility and opacity as global frontend-only session state', () => {
    expect(useCameraStore.getState().overlayVisible).toBe(true);
    expect(useCameraStore.getState().overlayOpacity).toBe(0.4);

    useCameraStore.getState().setOverlayVisible(false);
    useCameraStore.getState().setOverlayOpacity(1.5);

    expect(useCameraStore.getState().overlayVisible).toBe(false);
    expect(useCameraStore.getState().overlayOpacity).toBe(1);

    useCameraStore.getState().toggleOverlayVisible();
    useCameraStore.getState().setOverlayOpacity(-0.5);

    expect(useCameraStore.getState().overlayVisible).toBe(true);
    expect(useCameraStore.getState().overlayOpacity).toBe(0);
  });

  it('shows the overlay again after updating a frame', async () => {
    vi.mocked(cameraService.captureFrame).mockResolvedValue(frame);
    vi.mocked(cameraService.getAgentState).mockResolvedValue(agentState());

    useCameraStore.setState({
      selectedCameraId: 'cam-1',
      overlayVisible: false,
      devices: [
        {
          camera_id: 'cam-1',
          display_name: 'Top Camera',
          backend_kind: 'mock_snapshot',
          available: true,
          width_px: 100,
          height_px: 80,
          status_text: 'Ready',
        },
      ],
    });

    await useCameraStore.getState().captureFrame();

    expect(cameraService.captureFrame).toHaveBeenCalledWith('cam-1');
    expect(useCameraStore.getState().overlayVisible).toBe(true);
  });

  it('creates a fitted draft overlay after capture when no saved alignment exists', async () => {
    vi.mocked(cameraService.captureFrame).mockResolvedValue(frame);
    vi.mocked(cameraService.getAgentState).mockResolvedValue(agentState());
    useCameraStore.setState({
      selectedCameraId: 'cam-1',
      devices: [{
        camera_id: 'cam-1',
        display_name: 'Top Camera',
        backend_kind: 'mock_snapshot',
        available: true,
        width_px: 100,
        height_px: 50,
        status_text: 'Ready',
      }],
    });

    await useCameraStore.getState().captureFrame({
      bed_width_mm: 200,
      bed_height_mm: 100,
      origin: 'top_left',
    });

    expect(useCameraStore.getState().draftOverlayTransform).toEqual({
      scale: 2,
      rotation_deg: 0,
      translation_x: 0,
      translation_y: 0,
    });
    expect(useCameraStore.getState().overlayDraftDirty).toBe(false);
  });

  it('does not replace a saved alignment with auto-fit after capture', async () => {
    const savedAlignment = {
      transform: {
        scale: 0.5,
        rotation_deg: 10,
        translation_x: 4,
        translation_y: 8,
      },
      rmse_mm: 0.1,
      quality_score: 0.9,
      solved_at: '2026-05-22T12:00:00Z',
      source: 'solved_points' as const,
    };
    vi.mocked(cameraService.captureFrame).mockResolvedValue(frame);
    vi.mocked(cameraService.getAgentState).mockResolvedValue(agentState({
      alignment: savedAlignment,
      overlay_ready: true,
      display: {
        ...agentState({ alignment: savedAlignment }).display,
        draft_overlay_transform: null,
        draft_base_transform: null,
        effective_transform: savedAlignment.transform,
        status: 'saved_alignment',
      },
    }));
    useCameraStore.setState({
      selectedCameraId: 'cam-1',
      alignment: savedAlignment,
      devices: [{
        camera_id: 'cam-1',
        display_name: 'Top Camera',
        backend_kind: 'mock_snapshot',
        available: true,
        width_px: 100,
        height_px: 50,
        status_text: 'Ready',
      }],
    });

    await useCameraStore.getState().captureFrame({
      bed_width_mm: 200,
      bed_height_mm: 100,
      origin: 'top_left',
    });

    expect(useCameraStore.getState().draftOverlayTransform).toBeNull();
    expect(useCameraStore.getState().alignment).toEqual(savedAlignment);
  });

  it('saves manual draft alignment and exits adjustment state', async () => {
    useCameraStore.setState({
      selectedCameraId: 'cam-1',
      draftOverlayTransform: {
        scale: 1.5,
        rotation_deg: 5,
        translation_x: 10,
        translation_y: 12,
      },
      overlayAdjustMode: true,
      overlayDraftDirty: true,
    });

    await useCameraStore.getState().saveDraftAlignment();

    expect(cameraService.commitOverlayTransform).toHaveBeenCalledWith({
      scale: 1.5,
      rotation_deg: 5,
      translation_x: 10,
      translation_y: 12,
    });
    expect(cameraService.saveOverlayAlignment).toHaveBeenCalled();
    expect(useCameraStore.getState().overlayAdjustMode).toBe(false);
    expect(useCameraStore.getState().draftOverlayTransform).toBeNull();
  });

  it('keeps drag transforms local until final pointer-up commit', async () => {
    useCameraStore.setState({
      selectedCameraId: 'cam-1',
      overlayAdjustMode: true,
      draftOverlayTransform: {
        scale: 1,
        rotation_deg: 0,
        translation_x: 0,
        translation_y: 0,
      },
    });

    let lastTransform = useCameraStore.getState().draftOverlayTransform!;
    for (let i = 1; i <= 60; i += 1) {
      lastTransform = {
        scale: 1 + i / 100,
        rotation_deg: i,
        translation_x: i,
        translation_y: -i,
      };
      useCameraStore.getState().setDraftOverlayTransform(lastTransform);
    }

    expect(cameraService.commitOverlayTransform).not.toHaveBeenCalled();
    expect(useCameraStore.getState().draftOverlayTransform).toEqual(lastTransform);

    await useCameraStore.getState().commitDraftOverlayTransform();

    expect(cameraService.commitOverlayTransform).toHaveBeenCalledTimes(1);
    expect(cameraService.commitOverlayTransform).toHaveBeenCalledWith(lastTransform);
    expect(useCameraStore.getState().draftOverlayTransform).toEqual(lastTransform);
  });

  it('reverts saved-alignment drafts when exiting without saving', () => {
    useCameraStore.setState({
      alignment: {
        transform: {
          scale: 0.5,
          rotation_deg: 0,
          translation_x: 1,
          translation_y: 2,
        },
        rmse_mm: 0,
        quality_score: 1,
        solved_at: '2026-05-22T12:00:00Z',
        source: 'manual_adjust',
      },
      overlayState: {
        selected_camera_id: 'cam-1',
        frame: {
          handle_id: 'frame-1',
          file_path: '/tmp/beam-bench-camera/frame-1.png',
          width_px: 100,
          height_px: 50,
          media_type: 'image/png',
          captured_at: '2026-05-22T12:00:00Z',
        },
        calibration: null,
        alignment: null,
        overlay_ready: true,
      },
    });

    useCameraStore.getState().beginOverlayAdjust({
      bed_width_mm: 200,
      bed_height_mm: 100,
      origin: 'top_left',
    });
    useCameraStore.getState().setDraftOverlayTransform({
      scale: 2,
      rotation_deg: 0,
      translation_x: 0,
      translation_y: 0,
    });
    useCameraStore.getState().exitOverlayAdjust();

    expect(useCameraStore.getState().draftOverlayTransform).toBeNull();
    expect(useCameraStore.getState().overlayDraftDirty).toBe(false);
  });

  it('starts adjustment from a saved camera mapping and reverts without saving', () => {
    useCameraStore.setState({
      calibration: savedCalibration,
      alignment: null,
      overlayState: {
        selected_camera_id: 'cam-1',
        frame,
        calibration: savedCalibration,
        alignment: null,
        overlay_ready: true,
      },
    });

    useCameraStore.getState().beginOverlayAdjust();
    expect(useCameraStore.getState().draftOverlayTransform).toEqual(savedCalibration.transform);

    useCameraStore.getState().setDraftOverlayTransform({
      scale: 2,
      rotation_deg: 0,
      translation_x: 0,
      translation_y: 0,
    });
    useCameraStore.getState().exitOverlayAdjust();

    expect(useCameraStore.getState().draftOverlayTransform).toBeNull();
    expect(useCameraStore.getState().overlayDraftDirty).toBe(false);
  });

  it('does not publish solved camera transforms before they are saved', async () => {
    vi.mocked(cameraService.solveCalibration).mockResolvedValue({
      calibration: savedCalibration,
      point_count: 3,
    });
    vi.mocked(cameraService.solveAlignment).mockResolvedValue(solvedAlignment);
    useCameraStore.setState({ calibration: null, alignment: null, selectedCameraId: 'cam-1' });

    const calibration = await useCameraStore.getState().solveCalibration('cam-1', {
      image_width_px: 100,
      image_height_px: 50,
      points: [],
    });
    const alignment = await useCameraStore.getState().solveAlignment({ points: [] });

    expect(calibration).toEqual(savedCalibration);
    expect(alignment).toEqual(solvedAlignment);
    expect(useCameraStore.getState().calibration).toBeNull();
    expect(useCameraStore.getState().alignment).toBeNull();
  });
});
