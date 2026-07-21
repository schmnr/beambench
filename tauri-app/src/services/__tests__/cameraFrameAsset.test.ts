import { describe, expect, it, beforeEach, vi } from 'vitest';
import { convertFileSrc } from '@tauri-apps/api/core';
import tauriConfig from '../../../src-tauri/tauri.conf.json';
import {
  cameraFrameAssetUrl,
  resetCameraFrameTempScopeCheckForTests,
  verifyCameraFrameTempScope,
} from '../cameraFrameAsset';

vi.mock('@tauri-apps/api/core', () => ({
  convertFileSrc: vi.fn((filePath: string) => `asset://localhost/${filePath}`),
}));

vi.mock('@tauri-apps/api/path', () => ({
  tempDir: vi.fn().mockResolvedValue('/tmp'),
  join: vi.fn(async (...parts: string[]) => parts.join('/').replace(/\/+/g, '/')),
}));

describe('cameraFrameAsset', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetCameraFrameTempScopeCheckForTests();
  });

  it('converts captured frame files through Tauri asset URLs and handle cache keys', () => {
    const url = cameraFrameAssetUrl('/tmp/beam-bench-camera/frame.png', 'frame-123');

    expect(convertFileSrc).toHaveBeenCalledWith('/tmp/beam-bench-camera/frame.png');
    expect(url).toBe('asset://localhost//tmp/beam-bench-camera/frame.png?cameraFrame=frame-123');
  });

  it('warns when backend frame storage is outside Tauri tempDir scope', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);

    await verifyCameraFrameTempScope('/var/folders/app/T/beam-bench-camera/frame.png');

    expect(warn).toHaveBeenCalledWith(
      expect.stringContaining('Camera frame temp path differs from Tauri tempDir scope'),
    );
    warn.mockRestore();
  });

  it('keeps the production asset protocol scoped to camera temp files', () => {
    expect(tauriConfig.app.security.assetProtocol?.enable).toBe(true);
    expect(tauriConfig.app.security.assetProtocol?.scope).toContain('$TEMP/beam-bench-camera/**/*');
  });
});
