import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { open, save } from '@tauri-apps/plugin-dialog';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn(), save: vi.fn() }));

import { projectService } from '../projectService';
import { appService } from '../appService';
import { DEFAULT_TOOLBAR_VISIBILITY } from '../../panels';
import { importService } from '../importService';
import { useNotificationStore } from '../../stores/notificationStore';
import type { AlignmentType, DistributionDirection, FlipAxis, OperationType, RasterMode, ResizeSlotsOptions, SameSizeAxis } from '../../types/project';
import type { AppSettingsUpdate } from '../appService';

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  vi.mocked(open).mockReset();
  vi.mocked(save).mockReset();
});

afterEach(() => {
  vi.useRealTimers();
});

describe('projectService methods', () => {
  it('addLayer and addObjectAtomic accept backend operation literals only', async () => {
    vi.mocked(invoke).mockResolvedValue({});
    const operation: OperationType = 'offset_fill';

    await projectService.addLayer('Offsets', operation);
    await projectService.addObjectAtomic(
      'Rect',
      'layer-1',
      { type: 'shape', kind: 'rectangle', width: 10, height: 20, corner_radius: 0 },
      { min: { x: 0, y: 0 }, max: { x: 10, y: 20 } },
      { name: 'Derived Layer', operation },
    );

    expect(invoke).toHaveBeenNthCalledWith(1, 'add_layer', { name: 'Offsets', operation: 'offset_fill' });
    expect(invoke).toHaveBeenNthCalledWith(2, 'add_object_atomic', {
      name: 'Rect',
      layerId: 'layer-1',
      objectData: { type: 'shape', kind: 'rectangle', width: 10, height: 20, corner_radius: 0 },
      bounds: { min: { x: 0, y: 0 }, max: { x: 10, y: 20 } },
      createLayerName: 'Derived Layer',
      createLayerColorTag: undefined,
      createLayerOperation: 'offset_fill',
      createLayerEntryPatch: undefined,
    });
  });

  it('addObjectAtomic never invents a CutEntryId — createLayerEntry is not sent on the wire', async () => {
    // Regression for the rectangle-draw bug: the old implementation
    // synthesized a CutEntry with `id: ''` when the caller supplied only an
    // operation, which blew up at the Tauri IPC boundary with
    // "UUID parsing failed: invalid length: expected length 32 ... found 0".
    // The backend now mints the CutEntry (and its id) via
    // `Layer::new_single_entry`, so the wire payload must never carry a
    // `createLayerEntry` field at all.
    vi.mocked(invoke).mockResolvedValue({
      object: { id: 'obj-1', layer_id: 'new-layer' },
      createdLayer: null,
    });

    await projectService.addObjectAtomic(
      'Rect',
      'auto',
      { type: 'shape', kind: 'rectangle', width: 10, height: 10, corner_radius: 0 },
      { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } },
      { name: 'Line', operation: 'line', color_tag: '#000000' },
    );

    const payload = vi.mocked(invoke).mock.calls[0][1] as Record<string, unknown>;
    expect(payload).not.toHaveProperty('createLayerEntry');
    expect(payload.createLayerOperation).toBe('line');
    expect(payload.createLayerName).toBe('Line');
    expect(payload.createLayerColorTag).toBe('#000000');
    expect(payload.createLayerEntryPatch).toBeUndefined();
  });

  it('addObjectAtomic forwards entry_patch when the caller wants to override defaults', async () => {
    vi.mocked(invoke).mockResolvedValue({
      object: { id: 'obj-1', layer_id: 'new-layer' },
      createdLayer: null,
    });

    await projectService.addObjectAtomic(
      'Rect',
      'auto',
      { type: 'shape', kind: 'rectangle', width: 10, height: 10, corner_radius: 0 },
      { min: { x: 0, y: 0 }, max: { x: 10, y: 10 } },
      {
        name: 'Line',
        operation: 'line',
        entry_patch: { speed_mm_min: 3500, power_percent: 25 },
      },
    );

    expect(invoke).toHaveBeenCalledWith(
      'add_object_atomic',
      expect.objectContaining({
        createLayerOperation: 'line',
        createLayerEntryPatch: { speed_mm_min: 3500, power_percent: 25 },
      }),
    );
  });

  it('updateObject forwards powerScale and lockAspectRatio through the real command payload', async () => {
    vi.mocked(invoke).mockResolvedValue({});

    await projectService.updateObject('obj-1', {
      power_scale: 0.65,
      lock_aspect_ratio: true,
    });

    expect(invoke).toHaveBeenCalledWith('update_object', {
      objectId: 'obj-1',
      powerScale: 0.65,
      lockAspectRatio: true,
    });
  });

  it('mirrorAcrossLine forwards object ids and axis object id', async () => {
    vi.mocked(invoke).mockResolvedValue([]);

    await projectService.mirrorAcrossLine(['obj-1', 'axis-1'], 'axis-1');

    expect(invoke).toHaveBeenCalledWith('mirror_across_line', {
      objectIds: ['obj-1', 'axis-1'],
      axisObjectId: 'axis-1',
    });
  });

  it('makeSameSize forwards axis and preserveAspect', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    const axis: SameSizeAxis = 'width';

    await projectService.makeSameSize(['obj-1', 'obj-2'], 'obj-2', axis, true);

    expect(invoke).toHaveBeenCalledWith('make_same_size', {
      objectIds: ['obj-1', 'obj-2'],
      anchorObjectId: 'obj-2',
      axis: 'width',
      preserveAspect: true,
    });
  });

  it('resizeSlots forwards options unchanged', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    const options: ResizeSlotsOptions = {
      currentThicknessMm: 3,
      newThicknessMm: 4,
      toleranceMm: 0.1,
    };

    await projectService.resizeSlots(['obj-1'], options);

    expect(invoke).toHaveBeenCalledWith('resize_slots', {
      objectIds: ['obj-1'],
      options,
    });
  });

  it('updateCutEntry invokes correct command', async () => {
    vi.mocked(invoke).mockResolvedValue({});
    await projectService.updateCutEntry('layer-1', 'entry-1', {
      operation: 'cut',
      speed_mm_min: 1000,
      power_percent: 80,
    });
    expect(invoke).toHaveBeenCalledWith('update_cut_entry', {
      layerId: 'layer-1',
      entryId: 'entry-1',
      patch: {
        operation: 'cut',
        speed_mm_min: 1000,
        power_percent: 80,
      },
    });
  });

  it('updateLayer wraps the patch in the Tauri command payload', async () => {
    vi.mocked(invoke).mockResolvedValue({
      id: 'layer-1',
      name: 'Image',
      entries: [],
      enabled: true,
      order_index: 0,
      color_tag: '#000000',
      visible: true,
    });

    await projectService.updateLayer('layer-1', { color_tag: '#000000' });

    expect(invoke).toHaveBeenCalledWith('update_layer', {
      layerId: 'layer-1',
      patch: { color_tag: '#000000' },
    });
    const payload = vi.mocked(invoke).mock.calls[0]?.[1] as Record<string, unknown>;
    expect(payload).not.toHaveProperty('colorTag');
    expect(payload).not.toHaveProperty('color_tag');
  });

  it('lockObjects invokes correct command', async () => {
    vi.mocked(invoke).mockResolvedValue(null);
    await projectService.lockObjects(['obj-1', 'obj-2']);
    expect(invoke).toHaveBeenCalledWith('lock_objects', { objectIds: ['obj-1', 'obj-2'] });
  });

  it('pushDrawOrder invokes correct command with the UI direction contract', async () => {
    vi.mocked(invoke).mockResolvedValue(null);
    await projectService.pushDrawOrder('obj-1', 'front');
    expect(invoke).toHaveBeenCalledWith('push_draw_order', { objectId: 'obj-1', direction: 'front' });
  });

  it('flip/rotate/move/reassign commands are typed as void mutations', async () => {
    vi.mocked(invoke).mockResolvedValue(null);
    const axis: FlipAxis = 'horizontal';

    await projectService.flipObjects(['obj-1'], axis);
    await projectService.rotateObjects(['obj-1'], 90, { x: 5, y: 6 });
    await projectService.moveObjectsTo(['obj-1'], 25, 30);
    await projectService.reassignLayer(['obj-1'], 'layer-2');

    expect(invoke).toHaveBeenNthCalledWith(1, 'flip_objects', {
      objectIds: ['obj-1'],
      horizontal: true,
    });
    expect(invoke).toHaveBeenNthCalledWith(2, 'rotate_objects', {
      objectIds: ['obj-1'],
      degrees: 90,
      pivotX: 5,
      pivotY: 6,
    });
    expect(invoke).toHaveBeenNthCalledWith(3, 'move_objects_to', {
      objectIds: ['obj-1'],
      x: 25,
      y: 30,
    });
    expect(invoke).toHaveBeenNthCalledWith(4, 'reassign_layer', {
      objectIds: ['obj-1'],
      targetLayerId: 'layer-2',
    });
  });

  it('flipObjects includes a shared pivot when provided', async () => {
    vi.mocked(invoke).mockResolvedValue(null);

    await projectService.flipObjects(['group-1', 'child-1'], 'vertical', { x: 15, y: 20 });

    expect(invoke).toHaveBeenCalledWith('flip_objects', {
      objectIds: ['group-1', 'child-1'],
      horizontal: false,
      pivotX: 15,
      pivotY: 20,
    });
  });

  it('rotateObjectsAndBakeActivePath invokes the node-align bake command', async () => {
    vi.mocked(invoke).mockResolvedValue({ id: 'obj-1' });

    await projectService.rotateObjectsAndBakeActivePath(
      ['obj-1', 'obj-2'],
      45,
      { x: 5, y: 6 },
      'obj-1',
    );

    expect(invoke).toHaveBeenCalledWith('rotate_objects_and_bake_active_path', {
      objectIds: ['obj-1', 'obj-2'],
      degrees: 45,
      pivotX: 5,
      pivotY: 6,
      activeObjectId: 'obj-1',
    });
  });

  it('setStartFrom invokes correct command', async () => {
    vi.mocked(invoke).mockResolvedValue({});
    await projectService.setStartFrom('user_origin');
    expect(invoke).toHaveBeenCalledWith('set_start_from', { mode: 'user_origin' });
  });

  it('autoJoinShapes returns raw path strings from the real command contract', async () => {
    const updated = ['M0 0 L10 0'];
    vi.mocked(invoke).mockResolvedValue(updated);

    const result = await projectService.autoJoinShapes(['obj-1', 'obj-2'], 0.5);

    expect(invoke).toHaveBeenCalledWith('auto_join_shapes', {
      objectIds: ['obj-1', 'obj-2'],
      tolerance: 0.5,
    });
    expect(result).toEqual(updated);
  });

  it('optimizeShapes returns raw path strings from the real command contract', async () => {
    const updated = ['M0 0 L8 0'];
    vi.mocked(invoke).mockResolvedValue(updated);

    const result = await projectService.optimizeShapes(['obj-1']);

    expect(invoke).toHaveBeenCalledWith('optimize_shapes', {
      objectIds: ['obj-1'],
      tolerance: 0.1,
    });
    expect(result).toEqual(updated);
  });

  it('align and distribute accept only backend-supported literals', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    const alignment: AlignmentType = 'centers_h';
    const direction: DistributionDirection = 'v_centered';

    await projectService.alignObjects(['obj-1', 'obj-2'], alignment);
    await projectService.distributeObjects(['obj-1', 'obj-2', 'obj-3'], direction);

    expect(invoke).toHaveBeenNthCalledWith(1, 'align_objects', {
      objectIds: ['obj-1', 'obj-2'],
      alignmentType: 'centers_h',
      anchorObjectId: null,
    });
    expect(invoke).toHaveBeenNthCalledWith(2, 'distribute_objects', {
      objectIds: ['obj-1', 'obj-2', 'obj-3'],
      direction: 'v_centered',
    });
  });
});

describe('appService methods', () => {
  it('updateSettings serializes display units and app appearance for the Tauri command', async () => {
    vi.mocked(invoke).mockResolvedValue({});
    const updates: AppSettingsUpdate = {
      display_unit: 'inches',
      speed_time_unit: 'seconds',
      ui_theme: 'light',
    };

    await appService.updateSettings(updates);

    expect(invoke).toHaveBeenCalledWith('update_app_settings', {
      displayUnit: 'inches',
      speedTimeUnit: 'seconds',
      uiTheme: 'light',
    });
  });

  it('updateDisplaySettings invokes correct command', async () => {
    vi.mocked(invoke).mockResolvedValue({});
    await appService.updateDisplaySettings({ dark_mode: true });
    expect(invoke).toHaveBeenCalledWith('update_display_settings', { darkMode: true });
  });

  it('requests window close through the backend command', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);

    await appService.requestWindowClose();

    expect(invoke).toHaveBeenCalledWith('request_window_close');
  });

  it('getSystemFonts invokes correct command', async () => {
    vi.mocked(invoke).mockResolvedValue(['Arial', 'Helvetica']);
    const fonts = await appService.getSystemFonts();
    expect(invoke).toHaveBeenCalledWith('get_system_fonts');
    expect(fonts).toEqual(['Arial', 'Helvetica']);
  });

  it('persistLayout notifies when the debounced settings save fails', async () => {
    vi.useFakeTimers();
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    vi.mocked(invoke).mockRejectedValue(new Error('persist failed'));
    useNotificationStore.setState({ notifications: [] });

    appService.persistLayout({
      zones: {
        left: { panelIds: [], activeTab: '' },
        bottom: { panelIds: [], activeTab: '' },
        'upper-right': { panelIds: ['cuts_layers'], activeTab: 'cuts_layers' },
        'lower-right': { panelIds: ['laser'], activeTab: 'laser' },
      },
      hiddenPanelIds: [],
      floatingPanels: [],
      upperSplitRatio: 0.5,
      rightPanelWidth: 320,
      leftPanelWidth: 280,
      bottomPanelHeight: 80,
      sidePanelsVisible: true,
      toolbarVisibility: { ...DEFAULT_TOOLBAR_VISIBILITY },
    });

    await vi.advanceTimersByTimeAsync(500);
    await Promise.resolve();

    const notifications = useNotificationStore.getState().notifications;
    expect(notifications[notifications.length - 1]?.message).toContain('Failed to save panel layout changes');
    expect(notifications[notifications.length - 1]?.type).toBe('error');
    expect(consoleError).toHaveBeenCalledWith(
      '[Beam Bench] Failed to save panel layout changes',
      expect.any(Error),
    );
    consoleError.mockRestore();
  });

  it('persistLayout serializes nested panel layout fields for the Rust command schema', async () => {
    vi.useFakeTimers();
    vi.mocked(invoke).mockResolvedValue({});

    appService.persistLayout({
      zones: {
        left: { panelIds: ['art_library'], activeTab: 'art_library' },
        bottom: { panelIds: ['console'], activeTab: 'console' },
        'upper-right': { panelIds: ['cuts_layers', 'move'], activeTab: 'move' },
        'lower-right': { panelIds: ['laser'], activeTab: 'laser' },
      },
      hiddenPanelIds: ['macros'],
      floatingPanels: [{
        panelId: 'camera',
        x: 10,
        y: 20,
        width: 320,
        height: 240,
        zIndex: 3,
        originZone: 'upper-right',
        originIndex: 1,
      }],
      upperSplitRatio: 0.55,
      rightPanelWidth: 360,
      leftPanelWidth: 260,
      bottomPanelHeight: 90,
      sidePanelsVisible: false,
      toolbarVisibility: { ...DEFAULT_TOOLBAR_VISIBILITY, main: false },
    });

    await vi.advanceTimersByTimeAsync(500);

    expect(invoke).toHaveBeenCalledWith('update_app_settings', {
      panelLayout: {
        zones: {
          left: { panel_ids: ['art_library'], active_tab: 'art_library' },
          bottom: { panel_ids: ['console'], active_tab: 'console' },
          'upper-right': { panel_ids: ['cuts_layers', 'move'], active_tab: 'move' },
          'lower-right': { panel_ids: ['laser'], active_tab: 'laser' },
        },
        hidden_panel_ids: ['macros'],
        floating_panels: [{
          panel_id: 'camera',
          x: 10,
          y: 20,
          width: 320,
          height: 240,
          z_index: 3,
          origin_zone: 'upper-right',
          origin_index: 1,
        }],
        upper_split_ratio: 0.55,
        right_panel_width: 360,
        left_panel_width: 260,
        bottom_panel_height: 90,
        side_panels_visible: false,
        toolbar_visibility: { ...DEFAULT_TOOLBAR_VISIBILITY, main: false },
      },
    });
  });
});

describe('importService methods', () => {
  it('pickFiles exposes the full raster filter set including tif', async () => {
    vi.mocked(open).mockResolvedValue(['/path/to/file.tif']);

    const result = await importService.pickFiles();

    expect(open).toHaveBeenCalledWith({
      title: 'Import Files',
      multiple: true,
      directory: false,
      filters: [
        { name: 'Supported Files', extensions: ['svg', 'png', 'jpg', 'jpeg', 'bmp', 'gif', 'tif', 'tiff', 'webp', 'tga', 'dxf', 'ai', 'pdf', 'eps', 'lbrn', 'lbrn2'] },
        { name: 'Lbrn Projects', extensions: ['lbrn', 'lbrn2'] },
        { name: 'SVG Files', extensions: ['svg'] },
        { name: 'Image Files', extensions: ['png', 'jpg', 'jpeg', 'bmp', 'gif', 'tif', 'tiff', 'webp', 'tga'] },
        { name: 'DXF Files', extensions: ['dxf'] },
        { name: 'AI Files', extensions: ['ai'] },
        { name: 'PDF Files', extensions: ['pdf'] },
        { name: 'EPS Files', extensions: ['eps'] },
      ],
    });
    expect(result).toEqual(['/path/to/file.tif']);
  });

  it('importDxfFile invokes correct command', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    await importService.importDxfFile('/path/to/file.dxf', 'layer-1');
    expect(invoke).toHaveBeenCalledWith('import_dxf_file', {
      filePath: '/path/to/file.dxf',
      layerId: 'layer-1',
    });
  });

  it('importGcodeFile invokes correct command with the real payload contract', async () => {
    vi.mocked(invoke).mockResolvedValue([
      { line_number: 1, raw: 'G0 X0', command: 'G0', params: { X: 0 } },
    ]);
    const lines = await importService.importGcodeFile('/path/to/file.gcode');
    expect(invoke).toHaveBeenCalledWith('import_gcode_file', { filePath: '/path/to/file.gcode' });
    expect(lines).toEqual([{ line_number: 1, raw: 'G0 X0', command: 'G0', params: { X: 0 } }]);
  });

  it('traceImage invokes correct command', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    await importService.traceImage('obj-1');
    expect(invoke).toHaveBeenCalledWith('trace_image', {
      objectId: 'obj-1',
      threshold: 128,
      cutoff: 0,
      turdsize: 2,
      alphamax: 1.0,
      opttolerance: 0.2,
      traceAlpha: false,
      sketchTrace: false,
      deleteSource: false,
      boundary: null,
    });
  });

  it('traceImage forwards an explicit trace boundary', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    const boundary = { x: 4, y: 8, width: 20, height: 24 };
    await importService.traceImage('obj-1', 128, 0, 2, 1.0, 0.2, false, false, true, boundary);
    expect(invoke).toHaveBeenCalledWith('trace_image', {
      objectId: 'obj-1',
      threshold: 128,
      cutoff: 0,
      turdsize: 2,
      alphamax: 1.0,
      opttolerance: 0.2,
      traceAlpha: false,
      sketchTrace: false,
      deleteSource: true,
      boundary,
    });
  });

  it('traceImagePreview includes a null boundary by default', async () => {
    vi.mocked(invoke).mockResolvedValue({ paths: [], source_width: 10, source_height: 10 });
    await importService.traceImagePreview('obj-1', 128, 0, 2, 1.0, 0.2, false, false, 7);
    expect(invoke).toHaveBeenCalledWith('trace_image_preview', {
      objectId: 'obj-1',
      threshold: 128,
      cutoff: 0,
      turdsize: 2,
      alphamax: 1.0,
      opttolerance: 0.2,
      traceAlpha: false,
      sketchTrace: false,
      requestId: 7,
      boundary: null,
    });
  });

  it('traceImagePreview forwards an explicit trace boundary', async () => {
    vi.mocked(invoke).mockResolvedValue({ paths: [], source_width: 10, source_height: 10 });
    const boundary = { x: 1, y: 2, width: 3, height: 4 };
    await importService.traceImagePreview('obj-1', 128, 0, 2, 1.0, 0.2, false, false, 8, boundary);
    expect(invoke).toHaveBeenCalledWith('trace_image_preview', {
      objectId: 'obj-1',
      threshold: 128,
      cutoff: 0,
      turdsize: 2,
      alphamax: 1.0,
      opttolerance: 0.2,
      traceAlpha: false,
      sketchTrace: false,
      requestId: 8,
      boundary,
    });
  });

  it('adjustImagePreview accepts only backend raster mode literals', async () => {
    vi.mocked(invoke).mockResolvedValue({ png_base64: '', width: 10, height: 10 });
    const mode: RasterMode = 'halftone';

    await importService.adjustImagePreview({
      objectId: 'obj-1',
      brightness: 0,
      contrast: 0,
      gamma: 1,
      invert: false,
      threshold: 128,
      saturation: 1,
      sharpen: 0,
      edgeEnhance: false,
      enhanceRadius: 0,
      enhanceAmount: 0,
      enhanceDenoise: 0,
      mode,
      dpi: 254,
      negative: false,
      passThrough: false,
      halftoneCellsPerInch: 12,
      halftoneAngleDeg: 15,
      newsprintAngleDeg: 45,
      newsprintFrequency: 10,
    });

    expect(invoke).toHaveBeenCalledWith('adjust_image_preview', expect.objectContaining({
      objectId: 'obj-1',
      mode: 'halftone',
      dpi: 254,
    }));
  });

  it('replaceImage treats dialog cancel as a no-op', async () => {
    vi.mocked(open).mockResolvedValue(null);

    await expect(importService.replaceImage('obj-1')).resolves.toBeNull();

    expect(invoke).not.toHaveBeenCalled();
  });

  it('replaceImage picker filter includes tif', async () => {
    vi.mocked(open).mockResolvedValue('/path/to/file.tif');
    vi.mocked(invoke).mockResolvedValue({ id: 'obj-1' });

    await importService.replaceImage('obj-1');

    expect(open).toHaveBeenCalledWith({
      title: 'Replace Image',
      multiple: false,
      directory: false,
      filters: [
        { name: 'Image Files', extensions: ['png', 'jpg', 'jpeg', 'bmp', 'gif', 'tif', 'tiff', 'webp', 'tga'] },
      ],
    });
  });
});
