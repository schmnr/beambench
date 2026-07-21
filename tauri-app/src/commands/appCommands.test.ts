import { afterEach, describe, expect, it, vi } from 'vitest';
import { APP_COMMANDS } from './appCommandIds';
import {
  BOOLEAN_ASSISTANT_OPEN_EVENT,
  executeAppCommand,
  getAppCommandState,
  isNativeMenuOwnedShortcut,
  nativeMenuShortcutKey,
  QUICK_HELP_DOCS_URL,
  setAppCommandDialogActions,
} from './appCommands';
import { persistenceService } from '../services/persistenceService';
import { printService } from '../services/printService';
import { appService } from '../services/appService';
import { useProjectStore } from '../stores/projectStore';
import { usePreviewStore } from '../stores/previewStore';
import { useUiStore } from '../stores/uiStore';
import { useUndoStore } from '../stores/undoStore';
import { useAppStore } from '../stores/appStore';
import { makeAppSettings, makeProject, makeProjectObject } from '../test-utils/projectFixtures';
import { clearCanvasViewportSize, setCanvasViewportSize } from '../canvas/canvasViewportRegistry';
import { DEFAULT_TOOLBAR_VISIBILITY } from '../panels';
import i18n from '../i18n';

const initialProjectState = useProjectStore.getState();
const initialPreviewState = usePreviewStore.getState();
const initialUiState = useUiStore.getState();
const initialUndoState = useUndoStore.getState();
const initialAppState = useAppStore.getState();

afterEach(() => {
  vi.restoreAllMocks();
  useProjectStore.setState(initialProjectState, true);
  usePreviewStore.setState(initialPreviewState, true);
  useUiStore.setState(initialUiState, true);
  useUndoStore.setState(initialUndoState, true);
  useAppStore.setState(initialAppState, true);
  clearCanvasViewportSize();
  setAppCommandDialogActions({});
});

describe('app command bridge', () => {
  it('executes the unified export command with explicit selection state', async () => {
    const project = makeProject();
    useProjectStore.setState({
      project,
      selectedObjectIds: [project.objects[0].id],
    });
    const exportArtwork = vi.spyOn(persistenceService, 'exportArtwork').mockResolvedValue('/tmp/out.svg');

    await executeAppCommand(APP_COMMANDS.FILE_EXPORT);

    expect(exportArtwork).toHaveBeenCalledWith({
      selectionOnly: true,
      selectedIds: [project.objects[0].id],
      defaultName: project.metadata.project_name,
    });
  });

  it('executes print commands with the requested print mode', async () => {
    const project = makeProject();
    useProjectStore.setState({ project });
    const printProject = vi.spyOn(printService, 'printProject').mockResolvedValue(undefined);

    await executeAppCommand(APP_COMMANDS.FILE_PRINT_BLACK);
    await executeAppCommand(APP_COMMANDS.FILE_PRINT_COLORS);

    expect(printProject).toHaveBeenNthCalledWith(1, 'black');
    expect(printProject).toHaveBeenNthCalledWith(2, 'color');
  });

  it('executes New Window through the app service', async () => {
    const openNewWindow = vi.spyOn(appService, 'openNewWindow').mockResolvedValue('main-test');

    await executeAppCommand(APP_COMMANDS.FILE_NEW_WINDOW);

    expect(openNewWindow).toHaveBeenCalledOnce();
  });

  it('quits through the Tauri window close, not the browser window.close', async () => {
    // The Tauri path triggers CloseRequested (and with it the
    // unsaved-changes prompt); the browser window.close() is unreliable
    // in the webview.
    const requestWindowClose = vi
      .spyOn(appService, 'requestWindowClose')
      .mockResolvedValue(undefined);

    await executeAppCommand(APP_COMMANDS.APP_QUIT);

    expect(requestWindowClose).toHaveBeenCalledOnce();
  });

  it('opens docs for Quick Help', async () => {
    const openExternalUrl = vi.spyOn(appService, 'openExternalUrl').mockResolvedValue(undefined);

    await executeAppCommand(APP_COMMANDS.HELP_QUICK_HELP);

    expect(openExternalUrl).toHaveBeenCalledWith(QUICK_HELP_DOCS_URL);
  });

  it('executes Save Processed Bitmap for a single raster selection', async () => {
    const base = makeProject().objects[0];
    const raster = {
      ...base,
      id: 'img-1',
      data: { type: 'raster_image' as const, asset_key: 'asset-1', original_width_px: 100, original_height_px: 100 },
    };
    const project = makeProject({ objects: [raster] });
    const saveProcessedBitmap = vi.spyOn(persistenceService, 'saveProcessedBitmap').mockResolvedValue('/tmp/processed.png');
    useProjectStore.setState({
      project,
      selectedObjectIds: ['img-1'],
    });

    await executeAppCommand(APP_COMMANDS.FILE_SAVE_PROCESSED_BITMAP);

    expect(saveProcessedBitmap).toHaveBeenCalledWith('img-1');
  });

  it('reports native menu enabled state and export title from stores', () => {
    const project = makeProject();
    useProjectStore.setState({
      project,
      selectedObjectIds: [project.objects[0].id],
    });
    usePreviewStore.setState({ state: 'current' });

    const state = getAppCommandState();
    expect(state.items).toContainEqual(expect.objectContaining({
      id: APP_COMMANDS.FILE_EXPORT,
      enabled: true,
      title: 'Export Selection',
      accelerator: 'Alt+x',
    }));
    expect(state.items).toContainEqual(expect.objectContaining({
      id: APP_COMMANDS.FILE_SAVE_MACHINE_FILES,
      enabled: true,
    }));
    expect(state.items).toContainEqual(expect.objectContaining({
      id: APP_COMMANDS.FILE_PRINT_BLACK,
      enabled: true,
    }));
    expect(state.items).toContainEqual(expect.objectContaining({
      id: APP_COMMANDS.FILE_PRINT_COLORS,
      enabled: true,
    }));
  });

  it('localizes native menu dynamic titles from the active i18n language', async () => {
    const previousLanguage = i18n.language;
    try {
      await i18n.changeLanguage('de');
      const project = makeProject();
      useProjectStore.setState({
        project,
        selectedObjectIds: [project.objects[0].id],
      });
      useUiStore.setState({ showNotesDialog: true });

      const state = getAppCommandState();
      expect(state.items).toContainEqual(expect.objectContaining({
        id: APP_COMMANDS.FILE_EXPORT,
        title: i18n.t('menus.file.export_selection'),
      }));
      expect(state.items).toContainEqual(expect.objectContaining({
        id: APP_COMMANDS.FILE_NOTES,
        title: i18n.t('menus.file.hide_notes'),
      }));
    } finally {
      await i18n.changeLanguage(previousLanguage);
    }
  });

  it('routes Laser Tools test commands to their dialogs', async () => {
    const openMaterialTest = vi.fn();
    const openFocusTest = vi.fn();
    const openIntervalTest = vi.fn();

    await executeAppCommand(APP_COMMANDS.LASER_MATERIAL_TEST, { openMaterialTest });
    await executeAppCommand(APP_COMMANDS.LASER_FOCUS_TEST, { openFocusTest });
    await executeAppCommand(APP_COMMANDS.LASER_INTERVAL_TEST, { openIntervalTest });

    expect(openMaterialTest).toHaveBeenCalledOnce();
    expect(openFocusTest).toHaveBeenCalledOnce();
    expect(openIntervalTest).toHaveBeenCalledOnce();
  });

  it('keeps Paste enabled for the system clipboard; Paste In Place needs the object clipboard', () => {
    useProjectStore.setState({ project: makeProject() });
    useUiStore.setState({ hasClipboard: false });

    // Paste stays enabled with a project open even when the in-app object
    // clipboard is empty: the system clipboard may hold an image/SVG, and a
    // disabled native menu item swallows Cmd+V on macOS entirely.
    const emptyState = getAppCommandState();
    expect(emptyState.items).toContainEqual(expect.objectContaining({
      id: APP_COMMANDS.EDIT_PASTE,
      enabled: true,
    }));
    expect(emptyState.items).toContainEqual(expect.objectContaining({
      id: APP_COMMANDS.EDIT_PASTE_IN_PLACE,
      enabled: false,
    }));

    useUiStore.setState({ hasClipboard: true });
    const populatedState = getAppCommandState();
    expect(populatedState.items).toContainEqual(expect.objectContaining({
      id: APP_COMMANDS.EDIT_PASTE,
      enabled: true,
    }));
    expect(populatedState.items).toContainEqual(expect.objectContaining({
      id: APP_COMMANDS.EDIT_PASTE_IN_PLACE,
      enabled: true,
    }));
  });

  it('enables Save Processed Bitmap for clone-backed raster selections', () => {
    const base = makeProject().objects[0];
    const raster = {
      ...base,
      id: 'img-1',
      data: { type: 'raster_image' as const, asset_key: 'asset-1', original_width_px: 100, original_height_px: 100 },
    };
    const clone = {
      ...base,
      id: 'clone-1',
      data: { type: 'virtual_clone' as const, source_id: 'img-1' },
    };
    useProjectStore.setState({
      project: makeProject({ objects: [raster, clone] }),
      selectedObjectIds: ['clone-1'],
    });

    expect(getAppCommandState().items).toContainEqual(expect.objectContaining({
      id: APP_COMMANDS.FILE_SAVE_PROCESSED_BITMAP,
      enabled: true,
    }));
  });

  it('enables Refresh Image only when the selected raster asset has source_path', () => {
    const base = makeProject().objects[0];
    const raster = {
      ...base,
      id: 'img-1',
      data: { type: 'raster_image' as const, asset_key: 'asset-1', original_width_px: 100, original_height_px: 100 },
    };
    useProjectStore.setState({
      project: makeProject({ objects: [raster], assets: [] }),
      selectedObjectIds: ['img-1'],
    });

    expect(getAppCommandState().items).toContainEqual(expect.objectContaining({
      id: APP_COMMANDS.EDIT_IMAGE_REFRESH,
      enabled: false,
    }));

    useProjectStore.setState({
      project: makeProject({
        objects: [raster],
        assets: [{
          id: 'asset-1',
          original_filename: 'image.png',
          media_type: 'png',
          byte_size: 10,
          width_px: 100,
          height_px: 100,
          source_path: '/tmp/image.png',
        }],
      }),
      selectedObjectIds: ['img-1'],
    });

    expect(getAppCommandState().items).toContainEqual(expect.objectContaining({
      id: APP_COMMANDS.EDIT_IMAGE_REFRESH,
      enabled: true,
    }));
  });

  it('disables Select Contained Shapes for open vector path references', () => {
    const base = makeProject().objects[0];
    const openPath = {
      ...base,
      id: 'path-1',
      data: { type: 'vector_path' as const, path_data: 'M0 0 L10 0', closed: false },
    };
    const closedPath = {
      ...base,
      id: 'path-2',
      data: { type: 'vector_path' as const, path_data: 'M0 0 L10 0 Z', closed: true },
    };

    useProjectStore.setState({
      project: makeProject({ objects: [openPath] }),
      selectedObjectIds: ['path-1'],
    });
    expect(getAppCommandState().items).toContainEqual(expect.objectContaining({
      id: APP_COMMANDS.EDIT_SELECT_CONTAINED_SHAPES,
      enabled: false,
    }));

    useProjectStore.setState({
      project: makeProject({ objects: [closedPath] }),
      selectedObjectIds: ['path-2'],
    });
    expect(getAppCommandState().items).toContainEqual(expect.objectContaining({
      id: APP_COMMANDS.EDIT_SELECT_CONTAINED_SHAPES,
      enabled: true,
    }));
  });

  it('keeps Import disabled before a project exists', () => {
    useProjectStore.setState({ project: null, selectedLayerId: null });

    expect(getAppCommandState().items).toContainEqual(expect.objectContaining({
      id: APP_COMMANDS.FILE_IMPORT,
      enabled: false,
    }));
    expect(getAppCommandState().items).toContainEqual(expect.objectContaining({
      id: APP_COMMANDS.FILE_PRINT_BLACK,
      enabled: false,
    }));
    expect(getAppCommandState().items).toContainEqual(expect.objectContaining({
      id: APP_COMMANDS.FILE_PRINT_COLORS,
      enabled: false,
    }));
    expect(getAppCommandState().items).toContainEqual(expect.objectContaining({
      id: APP_COMMANDS.FILE_NEW_WINDOW,
      enabled: true,
    }));
  });

  it('activates Warp and Deform selection tools for unlocked vector-compatible selections', async () => {
    const object = makeProjectObject({
      id: 'path-1',
      data: { type: 'vector_path' as const, path_data: 'M 0 0 L 10 0 L 10 10 Z', closed: true },
    });
    useProjectStore.setState({
      project: makeProject({ objects: [object] }),
      selectedObjectIds: ['path-1'],
    });

    await executeAppCommand(APP_COMMANDS.TOOLS_WARP_SELECTION);
    expect(useUiStore.getState().activeTool).toBe('warp_selection');

    await executeAppCommand(APP_COMMANDS.TOOLS_DEFORM_SELECTION);
    expect(useUiStore.getState().activeTool).toBe('deform_selection');
  });

  it('routes native preference commands to dialog launchers', async () => {
    const openImportPreferences = vi.fn();
    const openExportPreferences = vi.fn();
    const openPreferencesFolder = vi.fn();
    const resetPreferences = vi.fn();
    const openHotkeyEditor = vi.fn();

    await executeAppCommand(APP_COMMANDS.FILE_PREFS_IMPORT, { openImportPreferences });
    await executeAppCommand(APP_COMMANDS.FILE_PREFS_EXPORT, { openExportPreferences });
    await executeAppCommand(APP_COMMANDS.FILE_PREFS_OPEN_FOLDER, { openPreferencesFolder });
    await executeAppCommand(APP_COMMANDS.FILE_PREFS_RESET_DEFAULTS, { resetPreferences });
    await executeAppCommand(APP_COMMANDS.FILE_PREFS_EDIT_HOTKEYS, { openHotkeyEditor });

    expect(openImportPreferences).toHaveBeenCalledOnce();
    expect(openExportPreferences).toHaveBeenCalledOnce();
    expect(openPreferencesFolder).toHaveBeenCalledOnce();
    expect(resetPreferences).toHaveBeenCalledOnce();
    expect(openHotkeyEditor).not.toHaveBeenCalled();
  });

  it('routes App Preferences and Edit Settings to the same settings dialog action', async () => {
    const openSettings = vi.fn();

    await executeAppCommand(APP_COMMANDS.APP_PREFERENCES, { openSettings });
    await executeAppCommand(APP_COMMANDS.EDIT_SETTINGS, { openSettings });

    expect(openSettings).toHaveBeenCalledTimes(2);
  });

  it('routes Arrange Copy Along Path to the dialog with the last selected vector as guide', async () => {
    const project = makeProject({
      objects: [
        {
          ...makeProject().objects[0],
          id: 'shape-1',
          data: { type: 'shape' as const, kind: 'rectangle' as const, width: 10, height: 10, corner_radius: 0 },
        },
        {
          ...makeProject().objects[0],
          id: 'path-1',
          data: { type: 'vector_path' as const, path_data: 'M0 0 L10 0', closed: false },
        },
      ],
    });
    const openCopyAlongPath = vi.fn();
    useProjectStore.setState({
      project,
      selectedObjectIds: ['shape-1', 'path-1'],
    });

    await executeAppCommand(APP_COMMANDS.ARRANGE_COPY_ALONG_PATH, { openCopyAlongPath });

    expect(openCopyAlongPath).toHaveBeenCalledWith(['shape-1'], 'path-1');
  });

  it('locks and unlocks only selected objects that need the requested state change', async () => {
    const objects = [
      makeProjectObject({ id: 'outer', bounds: { min: { x: 0, y: 0 }, max: { x: 100, y: 100 } } }),
      makeProjectObject({ id: 'peer', bounds: { min: { x: 120, y: 0 }, max: { x: 130, y: 10 } }, locked: true }),
    ];
    const lockObjects = vi.fn();
    const unlockObjects = vi.fn();
    useProjectStore.setState({
      project: makeProject({ objects }),
      selectedObjectIds: ['outer', 'peer'],
      lockObjects,
      unlockObjects,
    });

    await executeAppCommand(APP_COMMANDS.ARRANGE_LOCK);
    await executeAppCommand(APP_COMMANDS.ARRANGE_UNLOCK);

    expect(lockObjects).toHaveBeenCalledWith(['outer']);
    expect(unlockObjects).toHaveBeenCalledWith(['peer']);
  });

  it('reports Arrange enable predicates from the ordered selection and lock state', () => {
    const objects = [
      makeProjectObject({ id: 'outer', bounds: { min: { x: 0, y: 0 }, max: { x: 100, y: 100 } } }),
      makeProjectObject({ id: 'child', bounds: { min: { x: 20, y: 20 }, max: { x: 30, y: 30 } } }),
      makeProjectObject({ id: 'peer', bounds: { min: { x: 120, y: 0 }, max: { x: 130, y: 10 } }, locked: true }),
    ];
    const stateItem = (id: string) => [...getAppCommandState().items].reverse().find((item) => item.id === id);

    useProjectStore.setState({
      project: makeProject({ objects }),
      selectedObjectIds: ['outer', 'peer'],
    });
    expect(stateItem(APP_COMMANDS.ARRANGE_ALIGN_LEFT)).toMatchObject({ enabled: true });
    expect(stateItem(APP_COMMANDS.ARRANGE_MOVE_H_TOGETHER)).toMatchObject({ enabled: true });
    expect(stateItem(APP_COMMANDS.ARRANGE_DISTRIBUTE_H_CENTERED)).toMatchObject({ enabled: false });
    expect(stateItem(APP_COMMANDS.ARRANGE_LOCK)).toMatchObject({ enabled: true });
    expect(stateItem(APP_COMMANDS.ARRANGE_UNLOCK)).toMatchObject({ enabled: true });

    useProjectStore.setState({ selectedObjectIds: ['outer', 'child', 'peer'] });
    expect(stateItem(APP_COMMANDS.ARRANGE_DISTRIBUTE_H_CENTERED)).toMatchObject({ enabled: true });
    expect(stateItem(APP_COMMANDS.ARRANGE_AUTO_GROUP)).toMatchObject({ enabled: true });

    useProjectStore.setState({
      project: makeProject({ objects: objects.map((object) => ({ ...object, locked: true })) }),
      selectedObjectIds: ['outer', 'child', 'peer'],
    });
    expect(stateItem(APP_COMMANDS.ARRANGE_ALIGN_LEFT)).toMatchObject({ enabled: false });
    expect(stateItem(APP_COMMANDS.ARRANGE_MOVE_H_TOGETHER)).toMatchObject({ enabled: false });
    expect(stateItem(APP_COMMANDS.ARRANGE_DISTRIBUTE_H_CENTERED)).toMatchObject({ enabled: false });
    expect(stateItem(APP_COMMANDS.ARRANGE_AUTO_GROUP)).toMatchObject({ enabled: false });
    expect(stateItem(APP_COMMANDS.ARRANGE_LOCK)).toMatchObject({ enabled: false });
    expect(stateItem(APP_COMMANDS.ARRANGE_UNLOCK)).toMatchObject({ enabled: true });
  });

  it('reports Window dynamic checked and enabled state', () => {
    const project = makeProject();
    useProjectStore.setState({ project, selectedObjectIds: [] });
    usePreviewStore.setState({ previewWindowOpen: true });
    useUiStore.setState({
      sidePanelsVisible: false,
      viewStyle: 'filled_coarse',
      panelLayout: {
        ...useUiStore.getState().panelLayout,
        hiddenPanelIds: ['console'],
        toolbarVisibility: { ...DEFAULT_TOOLBAR_VISIBILITY, arrangeLong: true, docking: false },
      },
    });

    const stateItem = (id: string) => [...getAppCommandState().items].reverse().find((item) => item.id === id);

    expect(stateItem(APP_COMMANDS.WINDOW_PREVIEW)).toMatchObject({ enabled: true, checked: true });
    expect(stateItem(APP_COMMANDS.WINDOW_ZOOM_TO_PAGE)).toMatchObject({ enabled: true });
    expect(stateItem(APP_COMMANDS.WINDOW_FRAME_SELECTION)).toMatchObject({ enabled: false });
    expect(stateItem(APP_COMMANDS.WINDOW_SIDE_PANELS)).toMatchObject({ checked: false });
    expect(stateItem(APP_COMMANDS.WINDOW_VIEW_STYLE_FILLED_COARSE)).toMatchObject({ checked: true });
    expect(stateItem(APP_COMMANDS.WINDOW_VIEW_STYLE_WIREFRAME_SMOOTH)).toMatchObject({ checked: false });
    expect(stateItem(APP_COMMANDS.WINDOW_PANEL_CONSOLE)).toMatchObject({ checked: false });
    expect(stateItem(APP_COMMANDS.WINDOW_TOOLBAR_ARRANGE_LONG)).toMatchObject({ checked: true });
    expect(stateItem(APP_COMMANDS.WINDOW_TOOLBAR_DOCKING)).toMatchObject({ checked: false });

    useProjectStore.setState({ selectedObjectIds: [project.objects[0].id] });
    expect(stateItem(APP_COMMANDS.WINDOW_FRAME_SELECTION)).toMatchObject({ enabled: true });
  });

  it('executes Window zoom commands with the registered canvas size', async () => {
    const project = makeProject({
      workspace: { bed_width_mm: 400, bed_height_mm: 200, origin: 'top_left' },
      objects: [
        makeProjectObject({
          id: 'wide',
          bounds: { min: { x: 100, y: 50 }, max: { x: 300, y: 150 } },
        }),
      ],
    });
    useProjectStore.setState({ project, selectedObjectIds: ['wide'] });
    setCanvasViewportSize({ width: 1000, height: 500 });

    await executeAppCommand(APP_COMMANDS.WINDOW_ZOOM_TO_PAGE);
    expect(useUiStore.getState().zoom).toBe(105);

    await executeAppCommand(APP_COMMANDS.WINDOW_FRAME_SELECTION);
    expect(useUiStore.getState().zoom).toBe(210);
  });

  it('leaves Window zoom commands as no-ops before the canvas size is registered', async () => {
    const project = makeProject();
    useProjectStore.setState({ project, selectedObjectIds: [project.objects[0].id] });
    useUiStore.setState({ zoom: 135, viewportOffset: { x: 12, y: 34 } });
    clearCanvasViewportSize();

    await executeAppCommand(APP_COMMANDS.WINDOW_ZOOM_TO_PAGE);
    await executeAppCommand(APP_COMMANDS.WINDOW_FRAME_SELECTION);

    expect(useUiStore.getState().zoom).toBe(135);
    expect(useUiStore.getState().viewportOffset).toEqual({ x: 12, y: 34 });
  });

  it('executes Window view and toolbar toggles through app commands', async () => {
    const updateSettings = vi.spyOn(appService, 'updateSettings').mockImplementation(async (updates) => makeAppSettings({
      antialiasing: updates.antialiasing ?? false,
      filled_rendering: updates.filled_rendering ?? false,
    }));
    useUiStore.setState({
      viewStyle: 'wireframe_smooth',
      panelLayout: {
        ...useUiStore.getState().panelLayout,
        toolbarVisibility: { ...DEFAULT_TOOLBAR_VISIBILITY },
      },
    });

    await executeAppCommand(APP_COMMANDS.WINDOW_VIEW_STYLE_FILLED_SMOOTH);
    expect(useUiStore.getState().viewStyle).toBe('filled_smooth');
    expect(updateSettings).toHaveBeenNthCalledWith(1, {
      antialiasing: true,
      filled_rendering: true,
    });

    await executeAppCommand(APP_COMMANDS.WINDOW_TOGGLE_WIREFRAME_FILLED);
    expect(useUiStore.getState().viewStyle).toBe('wireframe_smooth');
    expect(updateSettings).toHaveBeenNthCalledWith(2, {
      antialiasing: true,
      filled_rendering: false,
    });

    await executeAppCommand(APP_COMMANDS.WINDOW_TOOLBAR_DOCKING);
    expect(useUiStore.getState().panelLayout.toolbarVisibility.docking).toBe(false);
  });

  it('maps Arrange commands to the explicit backend command variants', async () => {
    const project = makeProject({
      objects: [
        makeProjectObject({ id: 'a' }),
        makeProjectObject({ id: 'b', bounds: { min: { x: 20, y: 0 }, max: { x: 30, y: 10 } } }),
        makeProjectObject({ id: 'c', bounds: { min: { x: 40, y: 0 }, max: { x: 50, y: 10 } } }),
      ],
    });
    const alignObjects = vi.fn().mockResolvedValue(undefined);
    const distributeObjects = vi.fn().mockResolvedValue(undefined);
    const moveObjectsTogether = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({
      project,
      selectedObjectIds: ['a', 'b', 'c'],
      alignObjects,
      distributeObjects,
      moveObjectsTogether,
    } as never);

    await executeAppCommand(APP_COMMANDS.ARRANGE_ALIGN_CENTERS);
    await executeAppCommand(APP_COMMANDS.ARRANGE_ALIGN_CENTER_VERTICAL);
    await executeAppCommand(APP_COMMANDS.ARRANGE_ALIGN_CENTER_HORIZONTAL);
    await executeAppCommand(APP_COMMANDS.ARRANGE_DISTRIBUTE_V_SPACED);
    await executeAppCommand(APP_COMMANDS.ARRANGE_DISTRIBUTE_H_CENTERED);
    await executeAppCommand(APP_COMMANDS.ARRANGE_MOVE_H_TOGETHER);

    expect(alignObjects).toHaveBeenNthCalledWith(1, ['a', 'b', 'c'], 'centers_xy');
    expect(alignObjects).toHaveBeenNthCalledWith(2, ['a', 'b', 'c'], 'centers_v');
    expect(alignObjects).toHaveBeenNthCalledWith(3, ['a', 'b', 'c'], 'centers_h');
    expect(distributeObjects).toHaveBeenNthCalledWith(1, ['a', 'b', 'c'], 'v_spaced');
    expect(distributeObjects).toHaveBeenNthCalledWith(2, ['a', 'b', 'c'], 'h_centered');
    expect(moveObjectsTogether).toHaveBeenCalledWith('horizontal');
  });

  it('opens the Nest Selected settings dialog before running nesting', async () => {
    const project = makeProject({
      objects: [
        makeProjectObject({ id: 'container' }),
        makeProjectObject({ id: 'part' }),
      ],
    });
    const openNest = vi.fn();
    useProjectStore.setState({
      project,
      selectedObjectIds: ['container', 'part'],
    });

    await executeAppCommand(APP_COMMANDS.ARRANGE_NEST_SELECTED, { openNest });

    expect(openNest).toHaveBeenCalledWith(['container', 'part']);
  });

  it('uses registered dialog actions when menu callers execute Close Selected Paths With Tolerance', async () => {
    const openCloseSelectedPathsWithTolerance = vi.fn();
    const cleanup = setAppCommandDialogActions({ openCloseSelectedPathsWithTolerance });
    const project = makeProject();
    useProjectStore.setState({
      project,
      selectedObjectIds: [project.objects[0].id],
    });

    try {
      await executeAppCommand(APP_COMMANDS.EDIT_CLOSE_SELECTED_PATHS_WITH_TOLERANCE);
    } finally {
      cleanup();
    }

    expect(openCloseSelectedPathsWithTolerance).toHaveBeenCalledWith([project.objects[0].id]);
  });

  it('routes Boolean Assistant to its dialog action with the ordered selection', async () => {
    const openBooleanAssistant = vi.fn();
    const project = makeProject({
      objects: [
        makeProjectObject({ id: 'shape-a' }),
        makeProjectObject({ id: 'shape-b', bounds: { min: { x: 5, y: 5 }, max: { x: 15, y: 15 } } }),
      ],
    });
    useProjectStore.setState({
      project,
      selectedObjectIds: ['shape-b', 'shape-a'],
    });

    await executeAppCommand(APP_COMMANDS.TOOLS_BOOLEAN_ASSISTANT, { openBooleanAssistant });

    expect(openBooleanAssistant).toHaveBeenCalledWith(['shape-b', 'shape-a']);
  });

  it('dispatches the Boolean Assistant bridge event when dialog actions are not registered', async () => {
    const received: string[][] = [];
    const onOpen = (event: Event) => {
      received.push((event as CustomEvent<string[]>).detail);
    };
    window.addEventListener(BOOLEAN_ASSISTANT_OPEN_EVENT, onOpen);
    const project = makeProject({
      objects: [
        makeProjectObject({ id: 'shape-a' }),
        makeProjectObject({ id: 'shape-b', bounds: { min: { x: 5, y: 5 }, max: { x: 15, y: 15 } } }),
      ],
    });
    useProjectStore.setState({
      project,
      selectedObjectIds: ['shape-a', 'shape-b'],
    });

    await executeAppCommand(APP_COMMANDS.TOOLS_BOOLEAN_ASSISTANT);

    expect(received).toEqual([['shape-a', 'shape-b']]);
    window.removeEventListener(BOOLEAN_ASSISTANT_OPEN_EVENT, onOpen);
  });

  it('confirms before deleting duplicates', async () => {
    const project = makeProject();
    const deleteDuplicates = vi.fn().mockResolvedValue(undefined);
    const confirmDeleteDuplicates = vi.fn().mockResolvedValue(true);
    useProjectStore.setState({
      project,
      selectedObjectIds: [project.objects[0].id],
      deleteDuplicates,
    });

    await executeAppCommand(APP_COMMANDS.EDIT_DELETE_DUPLICATES, { confirmDeleteDuplicates });

    expect(confirmDeleteDuplicates).toHaveBeenCalledWith([project.objects[0].id]);
    expect(deleteDuplicates).toHaveBeenCalledWith([project.objects[0].id]);
  });

  it('skips duplicate deletion when the confirmation is cancelled', async () => {
    const project = makeProject();
    const deleteDuplicates = vi.fn().mockResolvedValue(undefined);
    const confirmDeleteDuplicates = vi.fn().mockResolvedValue(false);
    useProjectStore.setState({
      project,
      selectedObjectIds: [project.objects[0].id],
      deleteDuplicates,
    });

    await executeAppCommand(APP_COMMANDS.EDIT_DELETE_DUPLICATES, { confirmDeleteDuplicates });

    expect(confirmDeleteDuplicates).toHaveBeenCalledWith([project.objects[0].id]);
    expect(deleteDuplicates).not.toHaveBeenCalled();
  });

  it('imports with the same selected-layer behavior as the toolbar button', async () => {
    const project = makeProject();
    const importFiles = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({
      project,
      selectedLayerId: project.layers[0].id,
      importFiles,
    });

    await executeAppCommand(APP_COMMANDS.FILE_IMPORT);

    expect(importFiles).toHaveBeenCalledWith(project.layers[0].id);
  });

  it('keeps the toolbar empty-layer import behavior for new projects', async () => {
    const importFiles = vi.fn().mockResolvedValue(undefined);
    useProjectStore.setState({
      project: makeProject({ layers: [], objects: [] }),
      selectedLayerId: null,
      importFiles,
    });

    await executeAppCommand(APP_COMMANDS.FILE_IMPORT);

    expect(importFiles).toHaveBeenCalledWith('');
  });

  it('normalizes native menu shortcut keys for duplicate-dispatch gating', () => {
    expect(nativeMenuShortcutKey({
      key: 'S',
      metaKey: true,
      altKey: false,
      shiftKey: true,
    })).toBe('meta+shift+s');
    expect(nativeMenuShortcutKey({
      key: '.',
      metaKey: false,
      altKey: false,
      shiftKey: false,
    })).toBe('period');
    expect(nativeMenuShortcutKey({
      key: '[',
      metaKey: true,
      altKey: true,
      shiftKey: false,
    })).toBe('meta+alt+[');
    expect(nativeMenuShortcutKey({
      key: ']',
      metaKey: true,
      altKey: false,
      shiftKey: true,
    })).toBe('meta+shift+]');
  });

  it('recognizes native-owned shortcuts that are not file menu commands', () => {
    const cases: KeyboardEventInit[] = [
      { key: 'B', metaKey: true, shiftKey: true },
      { key: 'P', metaKey: true },
      { key: 'P', metaKey: true, shiftKey: true },
      { key: 'H', metaKey: true, shiftKey: true },
      { key: 'H', altKey: true, shiftKey: true },
      { key: 'V', altKey: true, shiftKey: true },
      { key: 'ArrowLeft', altKey: true },
      { key: 'ArrowRight', altKey: true },
      { key: 'ArrowUp', altKey: true },
      { key: 'ArrowDown', altKey: true },
      { key: 'PageUp', altKey: true },
      { key: 'PageDown', altKey: true },
      { key: 'PageUp' },
      { key: 'PageDown', metaKey: true },
      { key: '[', metaKey: true, altKey: true },
      { key: ']', metaKey: true, altKey: true },
      { key: ']', metaKey: true, shiftKey: true },
      { key: '[', metaKey: true, shiftKey: true },
      { key: '.', metaKey: false },
      { key: ',', metaKey: false },
      { key: '0', metaKey: true },
      { key: '=', metaKey: true },
      { key: '-', metaKey: true },
      { key: 'A', metaKey: true, shiftKey: true },
      { key: 'W', altKey: true, shiftKey: true },
      { key: 'Tab', metaKey: true },
      { key: 'T', altKey: true },
      { key: 'I', altKey: true },
      { key: 'O', altKey: true },
      { key: 'J', altKey: true },
      { key: 'O', altKey: true, shiftKey: true },
      { key: 'D', altKey: true },
      { key: 'F1' },
    ];

    for (const init of cases) {
      expect(isNativeMenuOwnedShortcut(new KeyboardEvent('keydown', init), true)).toBe(true);
    }
  });

  it('does not claim command shortcuts while text input is focused', () => {
    const input = document.createElement('input');
    document.body.append(input);
    input.focus();

    try {
      expect(isNativeMenuOwnedShortcut(new KeyboardEvent('keydown', { key: 'Backspace' }), true)).toBe(false);
      expect(isNativeMenuOwnedShortcut(new KeyboardEvent('keydown', { key: 'r' }), true)).toBe(false);
    } finally {
      input.remove();
    }
  });

  it('ignores native plain tool commands while text input is focused', async () => {
    const input = document.createElement('input');
    document.body.append(input);
    input.focus();
    useUiStore.setState({ activeTool: 'select' });

    try {
      await executeAppCommand(APP_COMMANDS.TOOLS_RECTANGLE, {}, { source: 'native-menu' });
      expect(useUiStore.getState().activeTool).toBe('select');
    } finally {
      input.remove();
    }
  });

  it('ignores native plain tool commands while canvas text editing is active', async () => {
    const project = makeProject();
    useProjectStore.setState({ project });
    useUiStore.setState({ activeTool: 'select', textEditObjectId: project.objects[0].id });

    await executeAppCommand(APP_COMMANDS.TOOLS_TEXT, {}, { source: 'native-menu' });

    expect(useUiStore.getState().activeTool).toBe('select');
  });

  it('clears the native export accelerator while text editing is active', () => {
    const project = makeProject();
    useProjectStore.setState({ project });
    useUiStore.setState({ textEditObjectId: project.objects[0].id });

    const state = getAppCommandState();
    expect(state.items).toContainEqual(expect.objectContaining({
      id: APP_COMMANDS.FILE_EXPORT,
      enabled: true,
      accelerator: null,
    }));
  });

  it('clears the native Select tool Escape accelerator while the node tool owns Escape', () => {
    const project = makeProject();
    useProjectStore.setState({ project });
    useUiStore.setState({ activeTool: 'node' });

    const state = getAppCommandState();

    expect(state.items).toContainEqual(expect.objectContaining({
      id: APP_COMMANDS.TOOLS_SELECT,
      enabled: true,
      accelerator: null,
    }));
  });

  it('keeps a customized Select tool accelerator active while editing nodes', () => {
    const project = makeProject();
    useProjectStore.setState({ project });
    useUiStore.setState({ activeTool: 'node' });
    useAppStore.setState({
      settings: makeAppSettings({
        custom_hotkeys: { [APP_COMMANDS.TOOLS_SELECT]: 'F2' },
      }),
    });

    const state = getAppCommandState();

    expect(state.items).toContainEqual(expect.objectContaining({
      id: APP_COMMANDS.TOOLS_SELECT,
      enabled: true,
      accelerator: 'F2',
    }));
  });

  it('reports recent files for native submenu rebuilds', () => {
    useAppStore.setState({
      settings: makeAppSettings({
        recent_files: [{ name: 'Job', path: '/tmp/job.lzrproj', opened_at: 'now' }],
      }),
    });

    expect(getAppCommandState().recentFiles).toEqual([{ name: 'Job', path: '/tmp/job.lzrproj' }]);
  });
});

describe('language dispatch', () => {
  afterEach(() => {
    vi.restoreAllMocks();
    useAppStore.setState(initialAppState, true);
  });

  it.each([
    ['language.de', 'de'],
    ['language.es-ES', 'es-ES'],
    ['language.zh-TW', 'zh-TW'],
    ['language.ja', 'ja'],
  ])('routes %s through the store so the i18n effect fires', async (commandId, expectedCode) => {
    // Critical: dispatch must go through useAppStore.updateSettings,
    // not appService.updateSettings directly. The store update is what
    // publishes the new settings to subscribers, including App.tsx's
    // i18n.changeLanguage effect. Calling appService directly would
    // persist on disk but leave the in-memory store stale until the
    // next fetchSettings — visible bug: backend updated, UI stays in
    // old language until restart.
    const storeSpy = vi.spyOn(useAppStore.getState(), 'updateSettings').mockResolvedValue();

    await executeAppCommand(commandId);

    expect(storeSpy).toHaveBeenCalledWith({ display_language: expectedCode });
  });

  it('routes every SUPPORTED_LOCALES code through the same dispatch', async () => {
    const storeSpy = vi.spyOn(useAppStore.getState(), 'updateSettings').mockResolvedValue();
    const codes = [
      'en', 'de', 'es-ES', 'es-419', 'fr', 'it', 'pt-BR', 'nl', 'pl', 'cs',
      'sv', 'nb', 'da', 'fi', 'hu', 'tr', 'el', 'ru', 'sl',
      'ja', 'ko', 'zh-CN', 'zh-TW',
    ];
    for (const code of codes) {
      storeSpy.mockClear();
      await executeAppCommand(`language.${code}`);
      expect(storeSpy).toHaveBeenCalledWith({ display_language: code });
    }
  });
});
