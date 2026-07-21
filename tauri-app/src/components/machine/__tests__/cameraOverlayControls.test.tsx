import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, render, screen } from '@testing-library/react';
import {
  CameraOverlaySetupControls,
  CameraOverlayStatus,
} from '../CameraOverlayControls';
import { useCameraStore } from '../../../stores/cameraStore';
import { useProjectStore } from '../../../stores/projectStore';
import { makeProject } from '../../../test-utils/projectFixtures';

const initialCameraState = useCameraStore.getState();
const initialProjectState = useProjectStore.getState();

const frame = {
  handle_id: 'frame-1',
  file_path: '/tmp/beam-bench-camera/frame-1.png',
  width_px: 100,
  height_px: 50,
  media_type: 'image/png',
  captured_at: '2026-05-22T12:00:00Z',
};

const manualAlignment = {
  transform: {
    scale: 1,
    rotation_deg: 0,
    translation_x: 0,
    translation_y: 0,
  },
  rmse_mm: 0,
  quality_score: 1,
  solved_at: '2026-05-22T12:00:00Z',
  source: 'manual_adjust' as const,
};

const savedMapping = {
  camera_id: 'cam-1',
  image_width_px: 100,
  image_height_px: 50,
  transform: manualAlignment.transform,
  rmse_px: 0.2,
  quality_score: 0.98,
  solved_at: '2026-05-22T12:00:00Z',
};

afterEach(() => {
  cleanup();
  act(() => {
    useCameraStore.setState(initialCameraState, true);
    useProjectStore.setState(initialProjectState, true);
  });
});

describe('CameraOverlayControls', () => {
  it('renders no-frame, preview, adjusting, unsaved, and saved status states', () => {
    render(<CameraOverlayStatus />);
    expect(screen.getByText('Overlay: No frame')).toBeDefined();
    cleanup();

    act(() => {
      useCameraStore.setState({
        overlayState: {
          selected_camera_id: 'cam-1',
          frame,
          calibration: null,
          alignment: null,
          overlay_ready: false,
        },
        draftOverlayTransform: manualAlignment.transform,
        overlayAdjustMode: false,
        overlayDraftDirty: false,
      });
    });
    render(<CameraOverlayStatus />);
    expect(screen.getByText('Overlay: Preview fitted to bed')).toBeDefined();
    expect(screen.getByText('Camera Mapping: none')).toBeDefined();
    cleanup();

    act(() => {
      useCameraStore.setState({ overlayAdjustMode: true, overlayDraftDirty: false });
    });
    render(<CameraOverlayStatus />);
    expect(screen.getByText('Overlay: Adjusting overlay')).toBeDefined();
    cleanup();

    act(() => {
      useCameraStore.setState({ overlayAdjustMode: true, overlayDraftDirty: true });
    });
    render(<CameraOverlayStatus />);
    expect(screen.getByText('Overlay: Unsaved changes')).toBeDefined();
    cleanup();

    act(() => {
      useCameraStore.setState({
        alignment: manualAlignment,
        overlayState: {
          selected_camera_id: 'cam-1',
          frame,
          calibration: null,
          alignment: manualAlignment,
          overlay_ready: true,
        },
        draftOverlayTransform: null,
        overlayAdjustMode: false,
        overlayDraftDirty: false,
      });
    });
    render(<CameraOverlayStatus />);
    expect(screen.getByText('Overlay: Saved alignment')).toBeDefined();
    expect(screen.getByText('Alignment: Manual')).toBeDefined();
  });

  it('reports a saved camera mapping when it is the active overlay transform', () => {
    act(() => {
      useCameraStore.setState({
        calibration: savedMapping,
        alignment: null,
        overlayState: {
          selected_camera_id: 'cam-1',
          frame,
          calibration: savedMapping,
          alignment: null,
          overlay_ready: true,
        },
        draftOverlayTransform: null,
        overlayAdjustMode: false,
        overlayDraftDirty: false,
      });
    });

    render(<CameraOverlayStatus />);

    expect(screen.getByText('Overlay: Camera Mapping')).toBeDefined();
    expect(screen.getByText('Camera Mapping: 98%')).toBeDefined();
    expect(screen.getByText('Alignment: none')).toBeDefined();
  });

  it('treats a saved camera mapping as the base for overlay adjustment', () => {
    act(() => {
      useCameraStore.setState({
        calibration: savedMapping,
        alignment: null,
        overlayState: {
          selected_camera_id: 'cam-1',
          frame,
          calibration: savedMapping,
          alignment: null,
          overlay_ready: true,
        },
        draftOverlayTransform: null,
        overlayDraftDirty: false,
      });
    });

    render(<CameraOverlaySetupControls controlsEnabled />);

    expect((screen.getByText('Adjust Overlay') as HTMLButtonElement).disabled).toBe(false);
    expect((screen.getByText('Save Alignment') as HTMLButtonElement).disabled).toBe(true);
    expect(screen.queryByText('Discard Preview')).toBeNull();
  });

  it('shows Discard Preview only for unsaved preview drafts', () => {
    act(() => {
      useProjectStore.setState({
        project: makeProject({
          workspace: { bed_width_mm: 200, bed_height_mm: 100, origin: 'top_left' },
        }),
      });
    });
    act(() => {
      useCameraStore.setState({
        overlayState: {
          selected_camera_id: 'cam-1',
          frame,
          calibration: null,
          alignment: null,
          overlay_ready: false,
        },
        draftOverlayTransform: manualAlignment.transform,
        beginOverlayAdjust: vi.fn(),
        exitOverlayAdjust: vi.fn(),
        fitDraftOverlayToWorkspace: vi.fn(),
        saveDraftAlignment: vi.fn(),
        discardDraftOverlay: vi.fn(),
      });
    });

    const { rerender } = render(<CameraOverlaySetupControls controlsEnabled />);
    expect(screen.getByText('Adjust Overlay')).toBeDefined();
    expect(screen.getByText('Fit to Bed')).toBeDefined();
    expect(screen.getByText('Save Alignment')).toBeDefined();
    expect(screen.getByText('Discard Preview')).toBeDefined();

    act(() => {
      useCameraStore.setState({
        alignment: manualAlignment,
        overlayState: {
          selected_camera_id: 'cam-1',
          frame,
          calibration: null,
          alignment: manualAlignment,
          overlay_ready: true,
        },
      });
    });
    rerender(<CameraOverlaySetupControls controlsEnabled />);

    expect(screen.queryByText('Discard Preview')).toBeNull();
  });
});
