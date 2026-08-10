import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  captureBrowserCameraFrame,
  disposeBrowserCameraSession,
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

const mediaStream = (stop = vi.fn()): MediaStream => ({
  getTracks: () => [{ stop, readyState: 'live' } as unknown as MediaStreamTrack],
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
    vi.spyOn(HTMLMediaElement.prototype, 'play').mockResolvedValue(undefined);
    vi.spyOn(HTMLMediaElement.prototype, 'pause').mockImplementation(() => {});
    Object.defineProperty(HTMLVideoElement.prototype, 'videoWidth', {
      configurable: true,
      get: () => 1280,
    });
    Object.defineProperty(HTMLVideoElement.prototype, 'videoHeight', {
      configurable: true,
      get: () => 720,
    });
    Object.defineProperty(HTMLVideoElement.prototype, 'readyState', {
      configurable: true,
      get: () => HTMLMediaElement.HAVE_METADATA,
    });
    Object.defineProperty(HTMLVideoElement.prototype, 'requestVideoFrameCallback', {
      configurable: true,
      value: (callback: () => void) => {
        queueMicrotask(callback);
        return 1;
      },
    });
    vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue({
      drawImage: vi.fn(),
    } as unknown as CanvasRenderingContext2D);
    vi.spyOn(HTMLCanvasElement.prototype, 'toBlob').mockImplementation((callback) => {
      callback({
        arrayBuffer: async () => new Uint8Array([1, 2, 3]).buffer,
      } as Blob);
    });
  });

  afterEach(() => {
    disposeBrowserCameraSession();
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

  it('stops a camera stream that arrives after the access timeout', async () => {
    vi.useFakeTimers();
    const stop = vi.fn();
    let provideStream!: (stream: MediaStream) => void;
    vi.mocked(navigator.mediaDevices.enumerateDevices).mockResolvedValue([]);
    vi.mocked(navigator.mediaDevices.getUserMedia).mockReturnValue(new Promise((resolve) => {
      provideStream = resolve;
    }));

    const result = resolveBrowserVideoInput(cameraDevice('C922 Pro Stream Webcam'));
    const expectation = expect(result).rejects.toThrow('Timed out waiting for camera access');
    await vi.advanceTimersByTimeAsync(8000);
    await expectation;

    provideStream(mediaStream(stop));
    await vi.runAllTimersAsync();
    expect(stop).toHaveBeenCalledTimes(1);
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

  it('reuses a warm camera session for consecutive captures', async () => {
    const stop = vi.fn();
    vi.mocked(navigator.mediaDevices.enumerateDevices).mockResolvedValue([
      mediaDevice('C922 Pro Stream Webcam', 'c922'),
    ]);
    vi.mocked(navigator.mediaDevices.getUserMedia).mockResolvedValue(mediaStream(stop));

    const firstStages: string[] = [];
    const secondStages: string[] = [];
    const first = await captureBrowserCameraFrame(cameraDevice('C922 Pro Stream Webcam'), undefined, {
      onStage: (stage) => firstStages.push(stage),
    });
    const second = await captureBrowserCameraFrame(cameraDevice('C922 Pro Stream Webcam'), undefined, {
      onStage: (stage) => secondStages.push(stage),
    });

    expect(first).toMatchObject({ widthPx: 1280, heightPx: 720, mediaType: 'image/png' });
    expect(second).toMatchObject({ widthPx: 1280, heightPx: 720, mediaType: 'image/png' });
    expect(navigator.mediaDevices.getUserMedia).toHaveBeenCalledTimes(1);
    expect(firstStages).toEqual(expect.arrayContaining(['resolving', 'opening', 'warming', 'capturing', 'encoding']));
    expect(secondStages).toEqual(expect.arrayContaining(['resolving', 'warming', 'capturing', 'encoding']));
    expect(stop).not.toHaveBeenCalled();

    disposeBrowserCameraSession();
    expect(stop).toHaveBeenCalledTimes(1);
  });

  it('retries once when a camera is temporarily busy', async () => {
    vi.mocked(navigator.mediaDevices.enumerateDevices).mockResolvedValue([
      mediaDevice('C922 Pro Stream Webcam', 'c922'),
    ]);
    vi.mocked(navigator.mediaDevices.getUserMedia)
      .mockRejectedValueOnce(new DOMException('busy', 'NotReadableError'))
      .mockResolvedValueOnce(mediaStream());
    const stages: string[] = [];

    await expect(captureBrowserCameraFrame(cameraDevice('C922 Pro Stream Webcam'), undefined, {
      onStage: (stage) => stages.push(stage),
    })).resolves.toMatchObject({ widthPx: 1280, heightPx: 720 });

    expect(navigator.mediaDevices.getUserMedia).toHaveBeenCalledTimes(2);
    expect(stages).toContain('retrying');
  });
});
