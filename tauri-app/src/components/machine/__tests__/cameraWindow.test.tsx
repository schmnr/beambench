import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, render, waitFor } from '@testing-library/react';
import { CameraWindow } from '../CameraWindow';
import { useCameraStore } from '../../../stores/cameraStore';
import { useUiStore } from '../../../stores/uiStore';

const initialCameraState = useCameraStore.getState();
const initialUiState = useUiStore.getState();

afterEach(() => {
  cleanup();
  useCameraStore.setState(initialCameraState, true);
  useUiStore.setState(initialUiState, true);
});

describe('CameraWindow', () => {
  it('hydrates overlay, calibration, and alignment on mount even from a cold store', async () => {
    const refreshDevices = vi.fn().mockResolvedValue(undefined);
    const refreshOverlayState = vi.fn().mockResolvedValue(undefined);
    const refreshCalibration = vi.fn().mockResolvedValue(undefined);
    const refreshAlignment = vi.fn().mockResolvedValue(undefined);

    useCameraStore.setState({
      devices: [],
      selectedCameraId: null,
      overlayState: null,
      calibration: null,
      alignment: null,
      loading: false,
      error: null,
      refreshDevices,
      selectCamera: vi.fn().mockResolvedValue(undefined),
      refreshOverlayState,
      refreshCalibration,
      refreshAlignment,
      captureFrame: vi.fn().mockResolvedValue(undefined),
      solveCalibration: vi.fn().mockResolvedValue(undefined),
      saveCalibration: vi.fn().mockResolvedValue(undefined),
      resetCalibration: vi.fn().mockResolvedValue(undefined),
      solveAlignment: vi.fn().mockResolvedValue(undefined),
      saveAlignment: vi.fn().mockResolvedValue(undefined),
      resetAlignment: vi.fn().mockResolvedValue(undefined),
    });
    useUiStore.setState({ toggleCameraWindow: vi.fn() });

    render(<CameraWindow />);

    await waitFor(() => {
      expect(refreshDevices).toHaveBeenCalled();
      expect(refreshOverlayState).toHaveBeenCalled();
      expect(refreshCalibration).toHaveBeenCalled();
      expect(refreshAlignment).toHaveBeenCalled();
    });
  });
});
