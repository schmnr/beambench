import type { CameraDeviceInfo } from '../types/camera';

interface BrowserCameraFrame {
  imageData: Uint8Array;
  widthPx: number;
  heightPx: number;
  mediaType: string;
}

const GET_USER_MEDIA_TIMEOUT_MS = 8000;
const CAMERA_LABEL_STOP_WORDS = new Set(['camera', 'webcam', 'video', 'usb', 'hd']);
const CAMERA_PERMISSION_DENIED_MESSAGE =
  'Camera permission denied. Enable camera access for Beam Bench in your operating system privacy settings, then try again.';
const CAMERA_NOT_FOUND_MESSAGE = 'Camera not found. Check that it is connected, then try again.';
const CAMERA_BUSY_MESSAGE = 'Camera is already in use by another app. Close the other app, then try again.';
const CAMERA_STREAM_TIMEOUT_MESSAGE = 'Timed out waiting for camera access. Check the camera connection and try again.';

export const normalizeCameraLabel = (value: string) => value
  .normalize('NFKD')
  .toLowerCase()
  .replace(/\p{Mark}/gu, '')
  .replace(/[^\p{Letter}\p{Number}]+/gu, ' ')
  .trim();

const compactCameraLabel = (value: string) => normalizeCameraLabel(value).replace(/\s+/g, '');

const cameraLabelTokens = (value: string) => normalizeCameraLabel(value)
  .split(/\s+/)
  .filter((token) => token.length > 1 && !CAMERA_LABEL_STOP_WORDS.has(token));

const cameraLabelScore = (target: string, candidate: string): number => {
  const targetCompact = compactCameraLabel(target);
  const candidateCompact = compactCameraLabel(candidate);
  if (!targetCompact || !candidateCompact) return 0;
  if (targetCompact === candidateCompact) return 100;
  if (candidateCompact.includes(targetCompact) || targetCompact.includes(candidateCompact)) return 80;

  const targetTokens = cameraLabelTokens(target);
  const candidateTokens = cameraLabelTokens(candidate);
  if (targetTokens.length === 0 || candidateTokens.length === 0) return 0;

  const matchedTokens = targetTokens.filter((targetToken) =>
    candidateTokens.some((candidateToken) =>
      candidateToken.includes(targetToken) || targetToken.includes(candidateToken),
    ),
  );
  const requiredMatches = Math.min(2, targetTokens.length);
  return matchedTokens.length >= requiredMatches ? 40 + matchedTokens.length : 0;
};

const cameraErrorName = (error: unknown): string | null => {
  if (!error || typeof error !== 'object' || !('name' in error)) return null;
  return String((error as { name?: unknown }).name);
};

const translateCameraError = (error: unknown): Error | null => {
  const name = cameraErrorName(error);
  if (name === 'NotAllowedError' || name === 'SecurityError') {
    return new Error(CAMERA_PERMISSION_DENIED_MESSAGE);
  }
  if (name === 'NotFoundError' || name === 'OverconstrainedError') {
    return new Error(CAMERA_NOT_FOUND_MESSAGE);
  }
  if (name === 'NotReadableError' || name === 'AbortError') {
    return new Error(CAMERA_BUSY_MESSAGE);
  }
  return null;
};

const stopStream = (stream: MediaStream) => {
  for (const track of stream.getTracks()) {
    track.stop();
  }
};

const waitForVideoReady = (video: HTMLVideoElement) => new Promise<void>((resolve, reject) => {
  const timeout = window.setTimeout(() => {
    reject(new Error('Timed out waiting for camera frame'));
  }, 8000);

  const finish = () => {
    window.clearTimeout(timeout);
    resolve();
  };

  if (video.readyState >= HTMLMediaElement.HAVE_METADATA && video.videoWidth > 0) {
    finish();
    return;
  }

  video.onloadedmetadata = finish;
  video.onerror = () => {
    window.clearTimeout(timeout);
    reject(new Error('Failed to load camera stream'));
  };
});

const encodeCanvasPng = (canvas: HTMLCanvasElement) => new Promise<Uint8Array>((resolve, reject) => {
  canvas.toBlob((blob) => {
    if (!blob) {
      reject(new Error('Failed to encode camera frame'));
      return;
    }
    blob.arrayBuffer()
      .then((buffer) => resolve(new Uint8Array(buffer)))
      .catch(() => reject(new Error('Failed to encode camera frame')));
  }, 'image/png');
});

const listVideoInputs = async (): Promise<MediaDeviceInfo[]> => {
  const devices = await navigator.mediaDevices.enumerateDevices();
  return devices.filter((candidate) => candidate.kind === 'videoinput');
};

export const findVideoInput = async (
  device: CameraDeviceInfo,
  cameraDevices: CameraDeviceInfo[] = [device],
): Promise<MediaDeviceInfo | null> => {
  const videoInputs = await listVideoInputs();
  const matches = videoInputs
    .map((candidate, index) => ({
      candidate,
      index,
      score: cameraLabelScore(device.display_name, candidate.label),
    }))
    .filter((match) => match.score > 0)
    .sort((left, right) => right.score - left.score || left.index - right.index);
  if (matches.length === 0) {
    return null;
  }

  const normalizedName = normalizeCameraLabel(device.display_name);
  const matchingCameras = cameraDevices.filter(
    (candidate) => candidate.backend_kind === 'native'
      && normalizeCameraLabel(candidate.display_name) === normalizedName,
  );
  const occurrence = matchingCameras.findIndex(
    (candidate) => candidate.camera_id === device.camera_id,
  );
  return matches[Math.max(occurrence, 0)]?.candidate ?? null;
};

const describeVideoInputs = (videoInputs: MediaDeviceInfo[]): string =>
  videoInputs.map((candidate) => candidate.label || candidate.deviceId || 'Unnamed camera').join(', ');

const requestCameraStream = async (browserDeviceId?: string) => {
  let timedOut = false;
  let timeoutHandle: number | undefined;
  const streamPromise = navigator.mediaDevices.getUserMedia({
    audio: false,
    video: browserDeviceId
      ? {
          deviceId: { exact: browserDeviceId },
          height: { ideal: 720 },
          width: { ideal: 1280 },
        }
      : {
          height: { ideal: 720 },
          width: { ideal: 1280 },
        },
  }).then((stream) => {
    if (timedOut) {
      stopStream(stream);
    }
    return stream;
  });
  const timeoutPromise = new Promise<never>((_, reject) => {
    timeoutHandle = window.setTimeout(() => {
      timedOut = true;
      reject(new Error(CAMERA_STREAM_TIMEOUT_MESSAGE));
    }, GET_USER_MEDIA_TIMEOUT_MS);
  });

  try {
    return await Promise.race([streamPromise, timeoutPromise]);
  } catch (error) {
    const translated = translateCameraError(error);
    if (translated) {
      throw translated;
    }
    throw error;
  } finally {
    if (timeoutHandle !== undefined) {
      window.clearTimeout(timeoutHandle);
    }
  }
};

export const resolveBrowserVideoInput = async (
  device: CameraDeviceInfo,
  cameraDevices: CameraDeviceInfo[] = [device],
): Promise<MediaDeviceInfo> => {
  let browserDevice = await findVideoInput(device, cameraDevices);
  if (browserDevice?.deviceId) {
    return browserDevice;
  }

  const permissionStream = await requestCameraStream();
  stopStream(permissionStream);

  browserDevice = await findVideoInput(device, cameraDevices);
  if (browserDevice?.deviceId) {
    return browserDevice;
  }

  const videoInputs = await listVideoInputs();
  const nativeDeviceCount = cameraDevices.filter(
    (candidate) => candidate.backend_kind === 'native',
  ).length;
  if (videoInputs.length === 1 && nativeDeviceCount <= 1 && videoInputs[0]?.deviceId) {
    return videoInputs[0];
  }

  throw new Error(
    `Could not match "${device.display_name}" to an available browser camera. Available cameras: ${describeVideoInputs(videoInputs) || 'none'}.`,
  );
};

export const captureBrowserCameraFrame = async (
  device: CameraDeviceInfo,
  cameraDevices: CameraDeviceInfo[] = [device],
): Promise<BrowserCameraFrame> => {
  if (!navigator.mediaDevices?.getUserMedia) {
    throw new Error('Camera capture is not available in this webview');
  }

  const browserDevice = await resolveBrowserVideoInput(device, cameraDevices);
  const stream = await requestCameraStream(browserDevice.deviceId);
  try {
    const video = document.createElement('video');
    video.muted = true;
    video.playsInline = true;
    video.srcObject = stream;
    await video.play();
    await waitForVideoReady(video);

    const widthPx = video.videoWidth;
    const heightPx = video.videoHeight;
    if (widthPx <= 0 || heightPx <= 0) {
      throw new Error('Camera returned an empty frame');
    }

    const canvas = document.createElement('canvas');
    canvas.width = widthPx;
    canvas.height = heightPx;
    const ctx = canvas.getContext('2d');
    if (!ctx) {
      throw new Error('Failed to create camera frame canvas');
    }
    ctx.drawImage(video, 0, 0, widthPx, heightPx);

    const imageData = await encodeCanvasPng(canvas);

    return {
      imageData,
      widthPx,
      heightPx,
      mediaType: 'image/png',
    };
  } finally {
    stopStream(stream);
  }
};
