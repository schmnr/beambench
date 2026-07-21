import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { CameraCalibrationDialog } from '../CameraCalibrationDialog';
import { CameraAlignmentDialog } from '../CameraAlignmentDialog';
import { useCameraStore } from '../../../stores/cameraStore';
import { useProjectStore } from '../../../stores/projectStore';
import { makeProject } from '../../../test-utils/projectFixtures';

vi.mock('../../../services/cameraFrameAsset', () => ({
  cameraFrameAssetUrl: (filePath: string) => `asset://${filePath}`,
}));

const initialCameraState = useCameraStore.getState();
const initialProjectState = useProjectStore.getState();

const solvedCalibration = {
  camera_id: 'cam-1',
  image_width_px: 1920,
  image_height_px: 1080,
  solved_at: '2026-03-20T12:00:00Z',
  transform: {
    scale: 1,
    rotation_deg: 0,
    translation_x: 0,
    translation_y: 0,
  },
  quality_score: 0.98,
  rmse_px: 0.25,
};

const solvedAlignment = {
  camera_id: 'cam-1',
  solved_at: '2026-03-20T12:00:00Z',
  transform: {
    scale: 1,
    rotation_deg: 0,
    translation_x: 0,
    translation_y: 0,
  },
  quality_score: 0.96,
  rmse_mm: 0.15,
};

afterEach(() => {
  cleanup();
  useCameraStore.setState(initialCameraState, true);
  useProjectStore.setState(initialProjectState, true);
});

describe('CameraCalibrationDialog', () => {
  it('solves and saves calibration through the camera store', async () => {
    const solveCalibration = vi.fn().mockResolvedValue(solvedCalibration);
    const saveCalibration = vi.fn().mockResolvedValue(undefined);
    const onClose = vi.fn();

    useCameraStore.setState({
      overlayState: {
        selected_camera_id: 'cam-1',
        frame: {
          handle_id: 'frame-1',
          width_px: 1920,
          height_px: 1080,
          media_type: 'image/png',
          captured_at: '2026-03-20T12:00:00Z',
          file_path: '/tmp/frame.png',
        },
        calibration: null,
        alignment: null,
        overlay_ready: true,
      },
      calibration: null,
      solveCalibration,
      saveCalibration,
      resetCalibration: vi.fn().mockResolvedValue(undefined),
    });

    render(<CameraCalibrationDialog cameraId="cam-1" onClose={onClose} />);

    fireEvent.click(screen.getByText('Solve'));

    await waitFor(() => {
      expect(solveCalibration).toHaveBeenCalledWith(
        'cam-1',
        expect.objectContaining({
          image_width_px: 1920,
          image_height_px: 1080,
          points: expect.any(Array),
        }),
      );
    });

    await screen.findByText(/Quality:/);
    fireEvent.click(screen.getByText('Save Mapping'));

    await waitFor(() => {
      expect(saveCalibration).toHaveBeenCalledWith('cam-1', solvedCalibration);
      expect(onClose).toHaveBeenCalled();
    });
  });

  it('keeps calibration dialog open when save fails', async () => {
    const solveCalibration = vi.fn().mockResolvedValue(solvedCalibration);
    const saveCalibration = vi.fn().mockRejectedValue(new Error('save failed'));
    const onClose = vi.fn();

    useCameraStore.setState({
      overlayState: {
        selected_camera_id: 'cam-1',
        frame: {
          handle_id: 'frame-1',
          width_px: 1920,
          height_px: 1080,
          media_type: 'image/png',
          captured_at: '2026-03-20T12:00:00Z',
          file_path: '/tmp/frame.png',
        },
        calibration: null,
        alignment: null,
        overlay_ready: true,
      },
      calibration: null,
      solveCalibration,
      saveCalibration,
      resetCalibration: vi.fn().mockResolvedValue(undefined),
    });

    render(<CameraCalibrationDialog cameraId="cam-1" onClose={onClose} />);

    fireEvent.click(screen.getByText('Solve'));
    await screen.findByText(/Quality:/);

    fireEvent.click(screen.getByText('Save Mapping'));

    await waitFor(() => {
      expect(saveCalibration).toHaveBeenCalledWith('cam-1', solvedCalibration);
    });
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByText('Camera Mapping')).toBeDefined();
  });

  it('preserves the solved calibration when reset fails', async () => {
    const resetCalibration = vi.fn().mockResolvedValue(false);

    useCameraStore.setState({
      overlayState: {
        selected_camera_id: 'cam-1',
        frame: null,
        calibration: solvedCalibration,
        alignment: null,
        overlay_ready: true,
      },
      calibration: solvedCalibration,
      solveCalibration: vi.fn(),
      saveCalibration: vi.fn(),
      resetCalibration,
    });

    render(<CameraCalibrationDialog cameraId="cam-1" onClose={vi.fn()} />);

    expect(screen.getByText(/Quality:/)).toBeDefined();
    fireEvent.click(screen.getByText('Reset Saved'));

    await waitFor(() => {
      expect(resetCalibration).toHaveBeenCalled();
    });
    expect(screen.getByText(/Quality:/)).toBeDefined();
  });

  it('invalidates a solved calibration when its points change', async () => {
    useCameraStore.setState({
      calibration: null,
      solveCalibration: vi.fn().mockResolvedValue(solvedCalibration),
      saveCalibration: vi.fn(),
      resetCalibration: vi.fn(),
    });

    render(<CameraCalibrationDialog cameraId="cam-1" onClose={vi.fn()} />);

    fireEvent.click(screen.getByText('Solve'));
    await screen.findByText(/Quality:/);
    expect((screen.getByText('Save Mapping') as HTMLButtonElement).disabled).toBe(false);

    fireEvent.change(screen.getAllByLabelText('Image X')[0], { target: { value: '125' } });

    expect(screen.queryByText(/Quality:/)).toBeNull();
    expect((screen.getByText('Save Mapping') as HTMLButtonElement).disabled).toBe(true);
  });

});

describe('CameraAlignmentDialog', () => {
  it('solves and saves alignment through the camera store', async () => {
    const solveAlignment = vi.fn().mockResolvedValue(solvedAlignment);
    const saveAlignment = vi.fn().mockResolvedValue(undefined);
    const onClose = vi.fn();

    useCameraStore.setState({
      alignment: null,
      solveAlignment,
      saveAlignment,
      resetAlignment: vi.fn().mockResolvedValue(undefined),
    });

    render(<CameraAlignmentDialog onClose={onClose} />);

    fireEvent.click(screen.getByText('Solve'));

    await waitFor(() => {
      expect(solveAlignment).toHaveBeenCalledWith(
        expect.objectContaining({
          points: expect.any(Array),
        }),
      );
    });

    await screen.findByText(/Quality:/);
    fireEvent.click(screen.getByText('Save Alignment'));

    await waitFor(() => {
      expect(saveAlignment).toHaveBeenCalledWith(solvedAlignment);
      expect(onClose).toHaveBeenCalled();
    });
  });

  it('converts bottom-left workspace alignment points to canvas coordinates before solving', async () => {
    const solveAlignment = vi.fn().mockResolvedValue(solvedAlignment);
    useProjectStore.setState({
      project: makeProject({
        workspace: { bed_width_mm: 400, bed_height_mm: 300, origin: 'bottom_left' },
      }),
    });
    useCameraStore.setState({
      alignment: null,
      solveAlignment,
      saveAlignment: vi.fn().mockResolvedValue(undefined),
      resetAlignment: vi.fn().mockResolvedValue(undefined),
    });

    render(<CameraAlignmentDialog onClose={vi.fn()} />);

    fireEvent.click(screen.getByText('Solve'));

    await waitFor(() => {
      expect(solveAlignment).toHaveBeenCalledWith({
        points: [
          { camera_x: 0, camera_y: 0, workspace_x_mm: 0, workspace_y_mm: 0 },
          { camera_x: 100, camera_y: 0, workspace_x_mm: 400, workspace_y_mm: 0 },
          { camera_x: 100, camera_y: 100, workspace_x_mm: 400, workspace_y_mm: 300 },
          { camera_x: 0, camera_y: 100, workspace_x_mm: 0, workspace_y_mm: 300 },
        ],
      });
    });
  });

  it('picks camera coordinates directly from the captured frame', () => {
    useCameraStore.setState({
      overlayState: {
        selected_camera_id: 'cam-1',
        frame: {
          handle_id: 'frame-1',
          width_px: 200,
          height_px: 100,
          media_type: 'image/png',
          captured_at: '2026-03-20T12:00:00Z',
          file_path: '/tmp/frame.png',
        },
        calibration: null,
        alignment: null,
        overlay_ready: false,
      },
      alignment: null,
      solveAlignment: vi.fn(),
      saveAlignment: vi.fn(),
      resetAlignment: vi.fn(),
    });

    render(<CameraAlignmentDialog onClose={vi.fn()} />);

    const preview = screen.getByRole('button', { name: 'Point 1' });
    vi.spyOn(preview, 'getBoundingClientRect').mockReturnValue({
      x: 0,
      y: 0,
      left: 0,
      top: 0,
      right: 100,
      bottom: 50,
      width: 100,
      height: 50,
      toJSON: () => ({}),
    });
    fireEvent.click(preview, { clientX: 50, clientY: 25 });

    expect((screen.getAllByLabelText('Camera X')[0] as HTMLInputElement).value).toBe('100');
    expect((screen.getAllByLabelText('Camera Y')[0] as HTMLInputElement).value).toBe('50');
  });

  it('keeps alignment dialog open when save fails', async () => {
    const solveAlignment = vi.fn().mockResolvedValue(solvedAlignment);
    const saveAlignment = vi.fn().mockRejectedValue(new Error('save failed'));
    const onClose = vi.fn();

    useCameraStore.setState({
      alignment: null,
      solveAlignment,
      saveAlignment,
      resetAlignment: vi.fn().mockResolvedValue(undefined),
    });

    render(<CameraAlignmentDialog onClose={onClose} />);

    fireEvent.click(screen.getByText('Solve'));
    await screen.findByText(/Quality:/);

    fireEvent.click(screen.getByText('Save Alignment'));

    await waitFor(() => {
      expect(saveAlignment).toHaveBeenCalledWith(solvedAlignment);
    });
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByText('Camera Alignment')).toBeDefined();
  });

  it('preserves the solved alignment when reset fails', async () => {
    const resetAlignment = vi.fn().mockResolvedValue(false);

    useCameraStore.setState({
      alignment: solvedAlignment,
      solveAlignment: vi.fn(),
      saveAlignment: vi.fn(),
      resetAlignment,
    });

    render(<CameraAlignmentDialog onClose={vi.fn()} />);

    expect(screen.getByText(/Quality:/)).toBeDefined();
    fireEvent.click(screen.getByText('Reset Saved'));

    await waitFor(() => {
      expect(resetAlignment).toHaveBeenCalled();
    });
    expect(screen.getByText(/Quality:/)).toBeDefined();
  });

  it('invalidates a solved alignment when its points change', async () => {
    useCameraStore.setState({
      alignment: null,
      solveAlignment: vi.fn().mockResolvedValue(solvedAlignment),
      saveAlignment: vi.fn(),
      resetAlignment: vi.fn(),
    });

    render(<CameraAlignmentDialog onClose={vi.fn()} />);

    fireEvent.click(screen.getByText('Solve'));
    await screen.findByText(/Quality:/);
    expect((screen.getByText('Save Alignment') as HTMLButtonElement).disabled).toBe(false);

    fireEvent.change(screen.getAllByLabelText('Camera X')[0], { target: { value: '25' } });

    expect(screen.queryByText(/Quality:/)).toBeNull();
    expect((screen.getByText('Save Alignment') as HTMLButtonElement).disabled).toBe(true);
  });

});
