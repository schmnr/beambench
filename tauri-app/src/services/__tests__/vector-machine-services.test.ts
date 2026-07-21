import { describe, it, expect, vi, beforeEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { open, save } from '@tauri-apps/plugin-dialog';
import { exportCanvasScreenshot } from '../canvasScreenshotExportService';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn(), save: vi.fn() }));
vi.mock('../canvasScreenshotExportService', () => ({ exportCanvasScreenshot: vi.fn() }));

import { vectorService } from '../vectorService';
import { machineService } from '../machineService';
import { cameraService } from '../cameraService';
import { persistenceService } from '../persistenceService';
import { appService } from '../appService';
import type { FrameMode, OverrideAction } from '../../types/machine';
import type { CameraFrameHandle } from '../../types/camera';
import type {
  GridSpacingMode,
  HandleType,
  BooleanAssistantOperation,
  OffsetCornerStyle,
  OffsetDirection,
  StartPointMode,
} from '../../types/vector';

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  vi.mocked(save).mockReset();
  vi.mocked(open).mockReset();
  vi.mocked(exportCanvasScreenshot).mockReset();
  delete (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
});

describe('vectorService methods', () => {
  it('booleanIntersection invokes correct command', async () => {
    vi.mocked(invoke).mockResolvedValue({});
    await vectorService.booleanIntersection('a', 'b');
    expect(invoke).toHaveBeenCalledWith('boolean_intersection', { objectIdA: 'a', objectIdB: 'b' });
  });

  it('booleanExclude invokes correct command', async () => {
    vi.mocked(invoke).mockResolvedValue({});
    await vectorService.booleanExclude('a', 'b');
    expect(invoke).toHaveBeenCalledWith('boolean_exclude', { objectIdA: 'a', objectIdB: 'b' });
  });

  it('booleanAssistantPreview invokes the read-only preview command', async () => {
    vi.mocked(invoke).mockResolvedValue({});
    const operation: BooleanAssistantOperation = 'subtract';
    await vectorService.booleanAssistantPreview(['a', 'b'], operation);
    expect(invoke).toHaveBeenCalledWith('boolean_assistant_preview', {
      objectIds: ['a', 'b'],
      operation: 'subtract',
    });
  });

  it('cutShapesApply invokes the mutating Cut Shapes command', async () => {
    vi.mocked(invoke).mockResolvedValue({ createdObjectIds: [], cutterObjectId: 'cutter' });
    await vectorService.cutShapesApply(['subject', 'cutter']);
    expect(invoke).toHaveBeenCalledWith('cut_shapes_apply', {
      objectIds: ['subject', 'cutter'],
    });
  });

  it('gridArray invokes correct command', async () => {
    vi.mocked(invoke).mockResolvedValue({ createdIds: [], groupId: null });
    const spacingMode: GridSpacingMode = 'edgeToEdge';
    await vectorService.gridArray({
      objectIds: ['obj-1'],
      rows: 3,
      cols: 4,
      hSpacingMm: 10,
      vSpacingMm: 10,
      spacingMode,
    });
    expect(invoke).toHaveBeenCalledWith('grid_array', {
      objectIds: ['obj-1'], rows: 3, cols: 4, hSpacingMm: 10, vSpacingMm: 10, spacingMode: 'edgeToEdge',
    });
  });

  it('offset/start-point/node-update accept only backend-supported literals', async () => {
    vi.mocked(invoke).mockResolvedValue({});
    const direction: OffsetDirection = 'both';
    const cornerStyle: OffsetCornerStyle = 'round';
    const mode: StartPointMode = 'set_and_reverse';
    const handleType: HandleType = 'out';

    await vectorService.offsetShapes(['obj-1'], 2, direction, cornerStyle, true);
    await vectorService.setStartPoint('obj-1', 10, 20, mode);
    await vectorService.updateNode('obj-1', 0, 1, 5, 6, handleType);

    expect(invoke).toHaveBeenNthCalledWith(1, 'offset_shapes', {
      objectIds: ['obj-1'],
      distance: 2,
      direction: 'both',
      cornerStyle: 'round',
      deleteOriginal: true,
    });
    expect(invoke).toHaveBeenNthCalledWith(2, 'set_start_point', {
      objectId: 'obj-1',
      x: 10,
      y: 20,
      mode: 'set_and_reverse',
    });
    expect(invoke).toHaveBeenNthCalledWith(3, 'update_node', {
      objectId: 'obj-1',
      subpathIdx: 0,
      commandIdx: 1,
      x: 5,
      y: 6,
      handleType: 'out',
    });
  });

  it('copyAlongPathBatch invokes correct command', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    await vectorService.copyAlongPathBatch(['obj-1', 'obj-2'], 'path-1', {
      count: 6,
      rotateCopies: true,
      scaleCopies: false,
      finalScalePercent: 100,
    });
    expect(invoke).toHaveBeenCalledWith('copy_along_path_batch', {
      objectIds: ['obj-1', 'obj-2'],
      pathObjectId: 'path-1',
      count: 6,
      rotate: true,
      scaleCopies: false,
      finalScalePercent: 100,
    });
  });

  it('addTabs invokes correct command', async () => {
    vi.mocked(invoke).mockResolvedValue({});
    await vectorService.addTabs('obj-1', 4, 2.5);
    expect(invoke).toHaveBeenCalledWith('add_tabs', { objectId: 'obj-1', count: 4, widthMm: 2.5 });
  });

  it('cropImage invokes the raster crop command', async () => {
    vi.mocked(invoke).mockResolvedValue({});
    await vectorService.cropImage('image-1', 'mask-1');
    expect(invoke).toHaveBeenCalledWith('crop_image', {
      imageObjectId: 'image-1',
      maskObjectId: 'mask-1',
    });
  });
});

describe('machineService methods', () => {
  it('picks .bbprofile import/export paths and preserves the extension', async () => {
    vi.mocked(open).mockResolvedValue('/tmp/shared.bbprofile');
    vi.mocked(save).mockResolvedValue('/tmp/My_Profile');

    await expect(machineService.pickMachineProfileImportPath()).resolves.toBe('/tmp/shared.bbprofile');
    await expect(machineService.pickMachineProfileExportPath('My/Profile')).resolves.toBe(
      '/tmp/My_Profile.bbprofile',
    );
    vi.mocked(save).mockResolvedValue('/tmp/UPPER.BBPROFILE');
    await expect(machineService.pickMachineProfileExportPath('Upper')).resolves.toBe(
      '/tmp/UPPER.BBPROFILE',
    );

    expect(open).toHaveBeenCalledWith(expect.objectContaining({
      multiple: false,
      directory: false,
      filters: [{ name: expect.any(String), extensions: ['bbprofile'] }],
    }));
    expect(save).toHaveBeenCalledWith(expect.objectContaining({
      defaultPath: 'My_Profile.bbprofile',
      filters: [{ name: expect.any(String), extensions: ['bbprofile'] }],
    }));
  });

  it('imports and exports machine profiles through the native commands', async () => {
    const profile = { id: 'imported-profile', name: 'Imported Profile' };
    vi.mocked(invoke).mockResolvedValueOnce(profile).mockResolvedValueOnce(undefined);

    await expect(machineService.importMachineProfile('/tmp/shared.bbprofile')).resolves.toBe(profile);
    await machineService.exportMachineProfile('saved-profile', '/tmp/saved.bbprofile');

    expect(invoke).toHaveBeenNthCalledWith(1, 'import_machine_profile', {
      path: '/tmp/shared.bbprofile',
    });
    expect(invoke).toHaveBeenNthCalledWith(2, 'export_machine_profile', {
      profileId: 'saved-profile',
      path: '/tmp/saved.bbprofile',
    });
  });

  it('frameJob accepts only supported frame modes', async () => {
    vi.mocked(invoke).mockResolvedValue({});
    const mode: FrameMode = 'rubber_band';
    await machineService.frameJob(mode, ['obj-1']);
    expect(invoke).toHaveBeenCalledWith('frame_job', {
      frameMode: 'rubber_band',
      selectedObjectIds: ['obj-1'],
      laserOnOverride: false,
    });
  });

  it('override methods accept only backend-supported actions', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    const reset: OverrideAction = 'reset';
    const increase: OverrideAction = 'increase_10';

    await machineService.setFeedOverride(reset);
    await machineService.setSpindleOverride(increase);

    expect(invoke).toHaveBeenNthCalledWith(1, 'set_feed_override', { action: 'reset' });
    expect(invoke).toHaveBeenNthCalledWith(2, 'set_spindle_override', { action: 'increase_10' });
  });

  it('sendGcodeLine invokes correct command', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await machineService.sendGcodeLine('G0 X10 Y10');
    expect(invoke).toHaveBeenCalledWith('send_gcode_line', { line: 'G0 X10 Y10' });
  });

  it('getConsoleLog invokes correct command', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    await machineService.getConsoleLog();
    expect(invoke).toHaveBeenCalledWith('get_console_log', { limit: 200 });
  });

  it('clearConsoleLog invokes correct command', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await machineService.clearConsoleLog();
    expect(invoke).toHaveBeenCalledWith('clear_console_log');
  });

  it('getMachineCoordinatesValid invokes correct command', async () => {
    vi.mocked(invoke).mockResolvedValue(true);
    await expect(machineService.getMachineCoordinatesValid()).resolves.toBe(true);
    expect(invoke).toHaveBeenCalledWith('get_machine_coordinates_valid');
  });

  it('setGrblSetting preserves the full unsigned 16-bit identifier range', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    for (const key of [0, 255, 256, 376, 65535]) {
      await machineService.setGrblSetting(key, 1);
      expect(invoke).toHaveBeenCalledWith('set_grbl_setting', { key, value: 1 });
    }
  });

  it('setGrblSetting rejects invalid identifiers and non-finite values before invoke', async () => {
    for (const key of [-1, 1.5, 65536, Number.NaN]) {
      await expect(machineService.setGrblSetting(key, 1)).rejects.toThrow(
        'GRBL setting ID must be an integer',
      );
    }
    for (const value of [Number.NaN, Number.POSITIVE_INFINITY]) {
      await expect(machineService.setGrblSetting(376, value)).rejects.toThrow(
        'GRBL setting value must be a finite number',
      );
    }
    expect(invoke).not.toHaveBeenCalled();
  });
});

describe('cameraService methods', () => {
  it('captureFrame returns the full camera frame handle contract', async () => {
    const frame: CameraFrameHandle = {
      handle_id: 'frame-1',
      file_path: '/tmp/frame-1.png',
      width_px: 640,
      height_px: 480,
      media_type: 'image/png',
      captured_at: '2026-04-16T14:30:00Z',
    };
    vi.mocked(invoke).mockResolvedValue(frame);

    const result = await cameraService.captureFrame('cam-1');

    expect(invoke).toHaveBeenCalledWith('capture_camera_frame', { cameraId: 'cam-1' });
    expect(result).toEqual(frame);
  });

  it('saveFrame sends raw bytes instead of base64 JSON', async () => {
    const frame: CameraFrameHandle = {
      handle_id: 'frame-1',
      file_path: '/tmp/frame-1.png',
      width_px: 1,
      height_px: 1,
      media_type: 'image/png',
      captured_at: '2026-04-16T14:30:00Z',
    };
    const imageData = new Uint8Array([137, 80, 78, 71]);
    vi.mocked(invoke).mockResolvedValue(frame);

    const result = await cameraService.saveFrame('cam-1', imageData, 1, 1, 'image/png');

    expect(invoke).toHaveBeenCalledWith('save_camera_frame_bytes', imageData, {
      headers: {
        'camera-id': 'cam-1',
        'height-px': '1',
        'media-type': 'image/png',
        'width-px': '1',
      },
    });
    expect(result).toEqual(frame);
  });
});

describe('persistenceService methods', () => {
  it('exportArtwork routes by selected extension and persists defaults', async () => {
    vi.mocked(invoke)
      .mockResolvedValueOnce({ export_settings: { last_directory: '/tmp', last_format: 'dxf', filename_stem: 'fixture' } })
      .mockResolvedValueOnce({ export_settings: { last_directory: '/tmp', last_format: 'png', filename_stem: 'output' } });
    vi.mocked(exportCanvasScreenshot).mockResolvedValue('/tmp/output.png');
    vi.mocked(save).mockResolvedValue('/tmp/output.png');

    await persistenceService.exportArtwork({ selectionOnly: true, selectedIds: ['obj-1'], defaultName: 'Project' });

    expect(save).toHaveBeenCalledWith(expect.objectContaining({
      title: 'Export selected vectors to file',
      defaultPath: '/tmp/fixture.dxf',
      filters: expect.arrayContaining([{ name: 'AutoCAD DXF Files (*.dxf)', extensions: ['dxf'] }]),
    }));
    expect(exportCanvasScreenshot).toHaveBeenCalledWith('/tmp/output.png', 'png');
    expect(invoke).not.toHaveBeenCalledWith('export_png', expect.anything());
    expect(invoke).toHaveBeenNthCalledWith(2, 'update_app_settings', {
      exportSettings: {
        last_directory: '/tmp',
        last_format: 'png',
        filename_stem: 'output',
      },
    });
  });

  it('exportArtwork appends the persisted default extension when none is typed', async () => {
    vi.mocked(invoke)
      .mockResolvedValueOnce({ export_settings: { last_directory: null, last_format: 'jpg', filename_stem: null } })
      .mockResolvedValueOnce({});
    vi.mocked(exportCanvasScreenshot).mockResolvedValue('/tmp/output.jpg');
    vi.mocked(save).mockResolvedValue('/tmp/output');

    await persistenceService.exportArtwork();

    expect(save).toHaveBeenCalledWith(expect.objectContaining({
      title: 'Export vectors to file',
      defaultPath: 'output.jpg',
    }));
    expect(exportCanvasScreenshot).toHaveBeenCalledWith('/tmp/output.jpg', 'jpg');
    expect(invoke).not.toHaveBeenCalledWith('export_jpg', expect.anything());
  });

  it('exportArtwork uses the macOS native picker with descriptive format labels', async () => {
    const platformSpy = vi.spyOn(window.navigator, 'platform', 'get').mockReturnValue('MacIntel');
    const userAgentSpy = vi.spyOn(window.navigator, 'userAgent', 'get').mockReturnValue('Mac OS X');
    (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {};
    vi.mocked(invoke)
      .mockResolvedValueOnce({ export_settings: { last_directory: '/tmp', last_format: 'ai', filename_stem: 'fixture' } })
      .mockResolvedValueOnce('/tmp/fixture.ai')
      .mockResolvedValueOnce('/tmp/fixture.ai')
      .mockResolvedValueOnce({});

    try {
      await persistenceService.exportArtwork();
    } finally {
      platformSpy.mockRestore();
      userAgentSpy.mockRestore();
    }

    expect(save).not.toHaveBeenCalled();
    expect(invoke).toHaveBeenNthCalledWith(2, 'pick_artwork_export_path', {
      title: 'Export vectors to file',
      defaultPath: '/tmp/fixture.ai',
      formats: [
        { label: 'Illustrator Files (*.ai)', extension: 'ai' },
        { label: 'AutoCAD DXF Files (*.dxf)', extension: 'dxf' },
        { label: 'SVG Files (*.svg)', extension: 'svg' },
        { label: 'PNG file (*.png)', extension: 'png' },
        { label: 'JPG file (*.jpg)', extension: 'jpg' },
        { label: 'BMP file (*.bmp)', extension: 'bmp' },
      ],
      selectedExtension: 'ai',
    });
    expect(invoke).toHaveBeenNthCalledWith(3, 'export_ai', {
      path: '/tmp/fixture.ai',
      selectionOnly: false,
      selectedIds: [],
    });
  });

  it('exportArtwork cancels before exporting', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ export_settings: { last_directory: null, last_format: 'svg', filename_stem: null } });
    vi.mocked(save).mockResolvedValue(null);

    await expect(persistenceService.exportArtwork()).rejects.toThrow('Export cancelled');
    expect(invoke).toHaveBeenCalledOnce();
    expect(exportCanvasScreenshot).not.toHaveBeenCalled();
  });

  it('exportArtwork rejects unsupported extensions before invoking an exporter', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ export_settings: { last_directory: null, last_format: 'svg', filename_stem: null } });
    vi.mocked(save).mockResolvedValue('/tmp/output.tiff');

    await expect(persistenceService.exportArtwork()).rejects.toThrow('Unsupported export file type ".tiff"');
    expect(invoke).toHaveBeenCalledOnce();
    expect(exportCanvasScreenshot).not.toHaveBeenCalled();
  });

  it('exportSvg invokes correct command with dialog', async () => {
    vi.mocked(save).mockResolvedValue('/tmp/output.svg');
    vi.mocked(invoke).mockResolvedValue('/tmp/output.svg');
    await persistenceService.exportSvg();
    expect(save).toHaveBeenCalled();
    expect(invoke).toHaveBeenCalledWith('export_svg', { path: '/tmp/output.svg', selectionOnly: false, selectedIds: [] });
  });

  it('exportSvg throws on cancel', async () => {
    vi.mocked(save).mockResolvedValue(null);
    await expect(persistenceService.exportSvg()).rejects.toThrow('Export cancelled');
  });

  it('exportEps invokes correct command with dialog', async () => {
    vi.mocked(save).mockResolvedValue('/tmp/output.eps');
    vi.mocked(invoke).mockResolvedValue('/tmp/output.eps');
    await persistenceService.exportEps();
    expect(save).toHaveBeenCalled();
    expect(invoke).toHaveBeenCalledWith('export_eps', { path: '/tmp/output.eps', selectionOnly: false, selectedIds: [] });
  });

  it('exportEps throws on cancel', async () => {
    vi.mocked(save).mockResolvedValue(null);
    await expect(persistenceService.exportEps()).rejects.toThrow('Export cancelled');
  });

  it('exportAi invokes correct command with dialog', async () => {
    vi.mocked(save).mockResolvedValue('/tmp/output.ai');
    vi.mocked(invoke).mockResolvedValue('/tmp/output.ai');
    await persistenceService.exportAi();
    expect(save).toHaveBeenCalled();
    expect(invoke).toHaveBeenCalledWith('export_ai', { path: '/tmp/output.ai', selectionOnly: false, selectedIds: [] });
  });

  it('exportAi throws on cancel', async () => {
    vi.mocked(save).mockResolvedValue(null);
    await expect(persistenceService.exportAi()).rejects.toThrow('Export cancelled');
  });

  it('getRecentFiles invokes correct command', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    await persistenceService.getRecentFiles();
    expect(invoke).toHaveBeenCalledWith('get_recent_files');
  });

  it('saveProcessedBitmap invokes the processed bitmap export command', async () => {
    vi.mocked(save).mockResolvedValue('/tmp/processed.png');
    vi.mocked(invoke).mockResolvedValue('/tmp/processed.png');
    await persistenceService.saveProcessedBitmap('obj-1');
    expect(invoke).toHaveBeenCalledWith('save_processed_bitmap', {
      objectId: 'obj-1',
      path: '/tmp/processed.png',
    });
  });
});

describe('appService preference helpers', () => {
  it('exports preferences through the .bbprefs picker and backend command', async () => {
    vi.mocked(save).mockResolvedValue('/tmp/beam-bench.bbprefs');
    vi.mocked(invoke).mockResolvedValue('/tmp/beam-bench.bbprefs');

    const path = await appService.pickPreferencesExportPath();
    await appService.exportPreferences(path);

    expect(save).toHaveBeenCalledWith(expect.objectContaining({
      title: 'Export Preferences',
      defaultPath: 'beam-bench.bbprefs',
    }));
    expect(invoke).toHaveBeenCalledWith('export_preferences', { path: '/tmp/beam-bench.bbprefs' });
  });

  it('imports and resets preferences through backend commands', async () => {
    vi.mocked(open).mockResolvedValue('/tmp/beam-bench.bbprefs');
    vi.mocked(invoke).mockResolvedValue({});

    const path = await appService.pickPreferencesImportPath();
    await appService.importPreferences(path);
    await appService.resetPreferences();

    expect(open).toHaveBeenCalledWith(expect.objectContaining({
      title: 'Import Preferences',
      multiple: false,
    }));
    expect(invoke).toHaveBeenNthCalledWith(1, 'import_preferences', { path: '/tmp/beam-bench.bbprefs' });
    expect(invoke).toHaveBeenNthCalledWith(2, 'reset_preferences');
  });

  it('opens the preferences folder through the backend command', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await appService.openPreferencesFolder();
    expect(invoke).toHaveBeenCalledWith('open_preferences_folder');
  });
});

// Optimization travels with the project through
// `projectService.setOptimization`. The backend command and frontend store
// action have their own focused tests.
