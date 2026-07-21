import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  findVideoInput,
  normalizeCameraLabel,
  resolveBrowserVideoInput,
} from './browserCameraCapture';
import type { CameraDeviceInfo } from '../types/camera';

const cameraDevice = (displayName: string, cameraId = `native:${displayName}`): CameraDeviceInfo => ({
  camera_id: cameraId,
  display_name: displayName,
  backend_kind: 'native',
  available: true,
  width_px: 0,
  height_px: 0,
  status_text: 'Ready',
});

const mediaDevice = (label: string, deviceId: string): MediaDeviceInfo => ({
  deviceId,
  groupId: '',
  kind: 'videoinput',
  label,
  toJSON: () => ({}),
});

const mediaStream = (): MediaStream => ({
  getTracks: () => [{ stop: vi.fn() } as unknown as MediaStreamTrack],
}) as unknown as MediaStream;

describe('browserCameraCapture', () => {
  beforeEach(() => {
    Object.defineProperty(navigator, 'mediaDevices', {
      configurable: true,
      value: {
        enumerateDevices: vi.fn(),
        getUserMedia: vi.fn(),
      },
    });
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it('normalizes camera labels with unicode and punctuation preserved where useful', () => {
    expect(normalizeCameraLabel('Caméra Élite HD (046D:085C)')).toBe('camera elite hd 046d 085c');
    expect(normalizeCameraLabel('カメラ Pro')).toBe('カメラ pro');
  });

  it('matches selected native cameras against browser video input labels', async () => {
    vi.mocked(navigator.mediaDevices.enumerateDevices).mockResolvedValue([
      mediaDevice('MacBook Pro Camera', 'macbook'),
      mediaDevice('Logitech C922 Pro Stream Webcam (046d:085c)', 'c922'),
    ]);

    const match = await findVideoInput(cameraDevice('C922 Pro Stream Webcam'));

    expect(match?.deviceId).toBe('c922');
  });

  it('translates permission denial into an actionable message', async () => {
    vi.mocked(navigator.mediaDevices.enumerateDevices).mockResolvedValue([]);
    vi.mocked(navigator.mediaDevices.getUserMedia).mockRejectedValue(
      new DOMException('denied', 'NotAllowedError'),
    );

    await expect(resolveBrowserVideoInput(cameraDevice('C922 Pro Stream Webcam'))).rejects.toThrow(
      'Camera permission denied. Enable camera access for Beam Bench in your operating system privacy settings, then try again.',
    );
  });

  it('matches duplicate camera models to distinct browser inputs', async () => {
    const first = cameraDevice('C922 Pro Stream Webcam', 'camera-native:c922-a');
    const second = cameraDevice('C922 Pro Stream Webcam', 'camera-native:c922-b');
    vi.mocked(navigator.mediaDevices.enumerateDevices).mockResolvedValue([
      mediaDevice('C922 Pro Stream Webcam', 'c922-first'),
      mediaDevice('C922 Pro Stream Webcam', 'c922-second'),
    ]);

    const firstMatch = await findVideoInput(first, [first, second]);
    const secondMatch = await findVideoInput(second, [first, second]);

    expect(firstMatch?.deviceId).toBe('c922-first');
    expect(secondMatch?.deviceId).toBe('c922-second');
  });

  it('translates missing camera errors into an actionable message', async () => {
    vi.mocked(navigator.mediaDevices.enumerateDevices).mockResolvedValue([]);
    vi.mocked(navigator.mediaDevices.getUserMedia).mockRejectedValue(
      new DOMException('not found', 'NotFoundError'),
    );

    await expect(resolveBrowserVideoInput(cameraDevice('C922 Pro Stream Webcam'))).rejects.toThrow(
      'Camera not found. Check that it is connected, then try again.',
    );
  });

  it('times out if getUserMedia does not resolve', async () => {
    vi.useFakeTimers();
    vi.mocked(navigator.mediaDevices.enumerateDevices).mockResolvedValue([]);
    vi.mocked(navigator.mediaDevices.getUserMedia).mockReturnValue(new Promise(() => {}));

    const result = resolveBrowserVideoInput(cameraDevice('C922 Pro Stream Webcam'));
    const expectation = expect(result).rejects.toThrow(
      'Timed out waiting for camera access. Check the camera connection and try again.',
    );
    await vi.advanceTimersByTimeAsync(8000);

    await expectation;
  });

  it('uses the only available browser camera when labels do not match', async () => {
    vi.mocked(navigator.mediaDevices.enumerateDevices).mockResolvedValue([
      mediaDevice('Unexpected Camera Label', 'single-camera'),
    ]);
    vi.mocked(navigator.mediaDevices.getUserMedia).mockResolvedValue(mediaStream());

    const resolvedDevice = await resolveBrowserVideoInput(cameraDevice('C922 Pro Stream Webcam'));

    expect(resolvedDevice.deviceId).toBe('single-camera');
    expect(navigator.mediaDevices.getUserMedia).toHaveBeenCalledWith({
      audio: false,
      video: {
        height: { ideal: 720 },
        width: { ideal: 1280 },
      },
    });
  });

  it('fails clearly when multiple browser cameras exist but none match the selected native camera', async () => {
    vi.mocked(navigator.mediaDevices.enumerateDevices).mockResolvedValue([
      mediaDevice('MacBook Pro Camera', 'macbook'),
      mediaDevice('iPhone Camera', 'iphone'),
    ]);
    vi.mocked(navigator.mediaDevices.getUserMedia).mockResolvedValue(mediaStream());

    await expect(resolveBrowserVideoInput(cameraDevice('C922 Pro Stream Webcam'))).rejects.toThrow(
      'Could not match "C922 Pro Stream Webcam" to an available browser camera.',
    );
  });
});
