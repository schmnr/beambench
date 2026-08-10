import type { CameraDeviceInfo } from '../types/camera';

export interface BrowserCameraFrame {
  imageData: Uint8Array;
  widthPx: number;
  heightPx: number;
  mediaType: string;
}

const GET_USER_MEDIA_TIMEOUT_MS = 8000;
const VIDEO_READY_TIMEOUT_MS = 8000;
const CAMERA_SESSION_IDLE_TIMEOUT_MS = 30_000;
const FALLBACK_WARMUP_MS = 180;
const WARMUP_FRAME_COUNT = 2;
const CAMERA_LABEL_STOP_WORDS = new Set(['camera', 'webcam', 'video', 'usb', 'hd']);
const CAMERA_PERMISSION_DENIED_MESSAGE =
  'Camera permission denied. Enable camera access for Beam Bench in your operating system privacy settings, then try again.';
const CAMERA_NOT_FOUND_MESSAGE = 'Camera not found. Check that it is connected, then try again.';
const CAMERA_BUSY_MESSAGE = 'Camera is already in use by another app. Close the other app, then try again.';
const CAMERA_STREAM_TIMEOUT_MESSAGE = 'Timed out waiting for camera access. Check the camera connection and try again.';

export type BrowserCameraCaptureStage =
  | 'resolving'
  | 'opening'
  | 'warming'
  | 'capturing'
  | 'encoding'
  | 'retrying';

interface BrowserCameraCaptureOptions {
  onStage?: (stage: BrowserCameraCaptureStage) => void;
}

interface BrowserCameraSession {
  deviceId: string;
  stream: MediaStream;
  video: HTMLVideoElement;
  idleTimer: ReturnType<typeof setTimeout> | null;
}

let activeSession: BrowserCameraSession | null = null;

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

const clearSessionIdleTimer = (session: BrowserCameraSession) => {
  if (session.idleTimer !== null) {
    clearTimeout(session.idleTimer);
    session.idleTimer = null;
  }
};

export const disposeBrowserCameraSession = () => {
  const session = activeSession;
  activeSession = null;
  if (!session) return;
  clearSessionIdleTimer(session);
  stopStream(session.stream);
  session.video.pause();
  session.video.srcObject = null;
};

const scheduleSessionDisposal = (session: BrowserCameraSession) => {
  clearSessionIdleTimer(session);
  session.idleTimer = setTimeout(() => {
    if (activeSession === session) {
      disposeBrowserCameraSession();
    }
  }, CAMERA_SESSION_IDLE_TIMEOUT_MS);
};

const sessionIsLive = (session: BrowserCameraSession) =>
  session.stream.getTracks().some((track) => track.readyState !== 'ended');

const waitForVideoReady = (video: HTMLVideoElement) => new Promise<void>((resolve, reject) => {
  let settled = false;

  const cleanup = () => {
    window.clearTimeout(timeout);
    video.removeEventListener('loadedmetadata', finish);
    video.removeEventListener('canplay', finish);
    video.removeEventListener('error', fail);
  };

  const finish = () => {
    if (settled || video.videoWidth <= 0 || video.videoHeight <= 0) return;
    settled = true;
    cleanup();
    resolve();
  };

  const fail = () => {
    if (settled) return;
    settled = true;
    cleanup();
    reject(new Error('Failed to load camera stream'));
  };

  const timeout = window.setTimeout(() => {
    if (settled) return;
    settled = true;
    cleanup();
    reject(new Error('Timed out waiting for camera frame. The camera may still be warming up or in use by another app.'));
  }, VIDEO_READY_TIMEOUT_MS);

  if (video.readyState >= HTMLMediaElement.HAVE_METADATA && video.videoWidth > 0) {
    finish();
    return;
  }

  video.addEventListener('loadedmetadata', finish);
  video.addEventListener('canplay', finish);
  video.addEventListener('error', fail);
});

const startVideoPlayback = async (video: HTMLVideoElement) => {
  let timeout: number | undefined;
  try {
    await Promise.race([
      video.play(),
      new Promise<never>((_, reject) => {
        timeout = window.setTimeout(() => {
          reject(new Error('Timed out starting the camera preview.'));
        }, VIDEO_READY_TIMEOUT_MS);
      }),
    ]);
  } finally {
    if (timeout !== undefined) window.clearTimeout(timeout);
  }
};

const waitForWarmupFrames = async (video: HTMLVideoElement, frameCount = WARMUP_FRAME_COUNT) => {
  const frameVideo = video as HTMLVideoElement & {
    requestVideoFrameCallback?: (callback: () => void) => number;
  };
  if (typeof frameVideo.requestVideoFrameCallback !== 'function') {
    await new Promise<void>((resolve) => window.setTimeout(resolve, FALLBACK_WARMUP_MS));
    return;
  }

  for (let frame = 0; frame < frameCount; frame += 1) {
    await new Promise<void>((resolve, reject) => {
      let settled = false;
      const timeout = window.setTimeout(() => {
        if (settled) return;
        settled = true;
        reject(new Error('Timed out waiting for camera warm-up frames.'));
      }, VIDEO_READY_TIMEOUT_MS);
      frameVideo.requestVideoFrameCallback?.(() => {
        if (settled) return;
        settled = true;
        window.clearTimeout(timeout);
        resolve();
      });
    });
  }
};

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

const createBrowserCameraSession = async (
  deviceId: string,
  onStage?: (stage: BrowserCameraCaptureStage) => void,
): Promise<BrowserCameraSession> => {
  onStage?.('opening');
  const stream = await requestCameraStream(deviceId);
  const video = document.createElement('video');
  video.muted = true;
  video.playsInline = true;
  video.srcObject = stream;

  try {
    await startVideoPlayback(video);
    await waitForVideoReady(video);
    onStage?.('warming');
    await waitForWarmupFrames(video);
  } catch (error) {
    stopStream(stream);
    video.srcObject = null;
    throw error;
  }

  return { deviceId, stream, video, idleTimer: null };
};

const acquireBrowserCameraSession = async (
  deviceId: string,
  onStage?: (stage: BrowserCameraCaptureStage) => void,
): Promise<BrowserCameraSession> => {
  if (activeSession?.deviceId === deviceId && sessionIsLive(activeSession)) {
    clearSessionIdleTimer(activeSession);
    onStage?.('warming');
    await waitForWarmupFrames(activeSession.video, 1);
    return activeSession;
  }

  disposeBrowserCameraSession();
  const session = await createBrowserCameraSession(deviceId, onStage);
  activeSession = session;
  return session;
};

const isRecoverableCaptureError = (error: unknown) => {
  if (!(error instanceof Error)) return false;
  return error.message.includes('Timed out waiting for camera')
    || error.message.includes('Timed out starting the camera')
    || error.message.includes('Failed to load camera stream')
    || error.message.includes('already in use');
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
  options: BrowserCameraCaptureOptions = {},
): Promise<BrowserCameraFrame> => {
  if (!navigator.mediaDevices?.getUserMedia) {
    throw new Error('Camera capture is not available in this webview');
  }

  options.onStage?.('resolving');
  const browserDevice = await resolveBrowserVideoInput(device, cameraDevices);
  let lastError: unknown = null;

  for (let attempt = 0; attempt < 2; attempt += 1) {
    if (attempt > 0) {
      options.onStage?.('retrying');
      disposeBrowserCameraSession();
    }

    let session: BrowserCameraSession | null = null;
    try {
      session = await acquireBrowserCameraSession(browserDevice.deviceId, options.onStage);
      const video = session.video;
      const widthPx = video.videoWidth;
      const heightPx = video.videoHeight;
      if (widthPx <= 0 || heightPx <= 0) {
        throw new Error('Camera returned an empty frame');
      }

      options.onStage?.('capturing');
      const canvas = document.createElement('canvas');
      canvas.width = widthPx;
      canvas.height = heightPx;
      const ctx = canvas.getContext('2d');
      if (!ctx) {
        throw new Error('Failed to create camera frame canvas');
      }
      ctx.drawImage(video, 0, 0, widthPx, heightPx);

      options.onStage?.('encoding');
      const imageData = await encodeCanvasPng(canvas);
      scheduleSessionDisposal(session);

      return {
        imageData,
        widthPx,
        heightPx,
        mediaType: 'image/png',
      };
    } catch (error) {
      lastError = error;
      if (session && activeSession === session) {
        disposeBrowserCameraSession();
      }
      if (attempt > 0 || !isRecoverableCaptureError(error)) {
        throw error;
      }
    }
  }

  throw lastError;
};
