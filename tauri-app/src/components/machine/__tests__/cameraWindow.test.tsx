import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { CameraOverlayControls } from '../CameraOverlayControls';
import { useMachineStore } from '../../../stores/machineStore';
import { CameraWindow } from '../CameraWindow';
import { useCameraStore } from '../../../stores/cameraStore';
import { useUiStore } from '../../../stores/uiStore';

const initialCameraState = useCameraStore.getState();
const initialMachineState = useMachineStore.getState();
const initialUiState = useUiStore.getState();

afterEach(() => {
  cleanup();
  useMachineStore.setState(initialMachineState, true);
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

describe('CameraOverlayControls', () => {
  it('disables display controls without a profile and enables them for a disconnected profile', () => {
    const setOverlayVisible = vi.fn();
    const setOverlayOpacity = vi.fn();
    useMachineStore.setState({ activeProfileId: null, sessionState: 'disconnected' });
    useCameraStore.setState({ overlayVisible: true, setOverlayVisible, setOverlayOpacity });
    render(<CameraOverlayControls />);
    const visibility = screen.getByRole('checkbox');
    const opacity = screen.getByRole('slider');
    expect(visibility).toHaveProperty('disabled', true);
    expect(visibility).toHaveProperty('checked', false);
    expect(opacity).toHaveProperty('disabled', true);
    (visibility as HTMLInputElement).click();
    expect(setOverlayVisible).not.toHaveBeenCalled();
    act(() => useMachineStore.setState({ activeProfileId: 'profile-1' }));
    expect(visibility).toHaveProperty('disabled', false);
    expect(opacity).toHaveProperty('disabled', false);
    fireEvent.click(visibility);
    fireEvent.change(opacity, { target: { value: '0.7' } });
    expect(setOverlayVisible).toHaveBeenCalledWith(false);
    expect(setOverlayOpacity).toHaveBeenCalledWith(0.7);
  });
});
