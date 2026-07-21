import { convertFileSrc } from '@tauri-apps/api/core';
import { join, tempDir } from '@tauri-apps/api/path';

const CAMERA_FRAME_TEMP_DIR = 'beam-bench-camera';

let tempScopeChecked = false;

function normalizePath(path: string): string {
  return path.replace(/\\/g, '/').replace(/\/+$/, '');
}

export function cameraFrameAssetUrl(filePath: string, cacheKey?: string): string {
  const assetUrl = convertFileSrc(filePath);
  if (!cacheKey) {
    return assetUrl;
  }
  const separator = assetUrl.includes('?') ? '&' : '?';
  return `${assetUrl}${separator}cameraFrame=${encodeURIComponent(cacheKey)}`;
}

export async function verifyCameraFrameTempScope(filePath: string): Promise<void> {
  const isProduction = Boolean(
    (import.meta as ImportMeta & { env?: { PROD?: boolean } }).env?.PROD,
  );
  if (tempScopeChecked || !filePath || isProduction) {
    return;
  }
  tempScopeChecked = true;

  try {
    const expectedDir = normalizePath(await join(await tempDir(), CAMERA_FRAME_TEMP_DIR));
    const actualPath = normalizePath(filePath);
    const actualDir = actualPath.slice(0, actualPath.lastIndexOf('/'));
    if (actualDir !== expectedDir) {
      console.warn(
        `Camera frame temp path differs from Tauri tempDir scope: frame=${actualDir}, tauri=${expectedDir}`,
      );
    }
  } catch (error) {
    console.warn('Failed to verify camera frame temp scope', error);
  }
}

export function resetCameraFrameTempScopeCheckForTests(): void {
  tempScopeChecked = false;
}
