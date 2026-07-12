import { appService } from '../services/appService';
import { persistenceService } from '../services/persistenceService';
import { printService, type PrintMode } from '../services/printService';
import { useNotificationStore } from '../stores/notificationStore';
import i18n, { DEFAULT_LOCALE, SUPPORTED_LOCALES } from '../i18n';
import { wrapBackendError } from '../i18n/errors';
import { useAppStore } from '../stores/appStore';
import { usePreviewStore } from '../stores/previewStore';
import { useProjectStore } from '../stores/projectStore';
import { renderOptionsFromViewStyle, useUiStore, type ToolType, type ViewStyle } from '../stores/uiStore';
import { useUndoStore } from '../stores/undoStore';
import { guardUnsavedChanges } from '../stores/unsavedGuardStore';
import { useMachineStore } from '../stores/machineStore';
import {
  clearClipboard,
  hasClipboardData,
  clipboardCopy,
  clipboardCut,
  clipboardDuplicate,
  clipboardPaste,
  clipboardPasteInPlace,
} from '../utils/clipboard';
import { pasteClipboardArtworkFromSystem } from '../utils/systemClipboard';
import { isNativeMenuActive } from '../utils/platform';
import { findAutoGroupCandidates } from '../utils/autoGroupCandidates';
import {
  jogLaser,
  moveLaserToSelection,
  moveSelectedToLaserPosition,
  moveSelectedToPageAnchor,
  nestSelected,
} from './arrangeActions';
import { APP_COMMANDS, type AppCommandId } from './appCommandIds';
import {
  imageObjectHasSourcePath,
  isClosedVectorCompatible,
  pickLastSelectedVectorGuide,
  resolveEffectiveData,
} from './selectionContext';
import { findCommandForKeyboardEvent, nativeAcceleratorForCommand, nativeAcceleratorUpdates } from './commandRegistry';
import { zoomToFitBounds } from '../canvas/ViewportTransform';
import { getCanvasViewportSize } from '../canvas/canvasViewportRegistry';
import {
  WINDOW_PANEL_BY_COMMAND,
  WINDOW_PANEL_MENU_ITEMS,
  WINDOW_TOOLBAR_BY_COMMAND,
  WINDOW_TOOLBAR_MENU_ITEMS,
  WINDOW_VIEW_STYLE_BY_COMMAND,
  WINDOW_VIEW_STYLE_ITEMS,
} from './windowMenuDefinitions';

export const QUICK_HELP_DOCS_URL = 'https://beambench.com/docs';

export interface AppCommandDialogActions {
  openAbout?: () => void;
  openSettings?: () => void;
  openNotes?: () => void;
  openImportPreferences?: () => void;
  openExportPreferences?: () => void;
  openPreferencesFolder?: () => void;
  resetPreferences?: () => void;
  openHotkeyEditor?: () => void;
  openTraceImage?: (objectId: string) => void;
  openAdjustImage?: (objectId: string) => void;
  openBarcode?: (layerId: string) => void;
  openOffset?: (objectIds: string[]) => void;
  openBooleanAssistant?: (objectIds: string[]) => void;
  openCloseSelectedPathsWithTolerance?: (objectIds: string[]) => void;
  confirmDeleteDuplicates?: (objectIds: string[]) => Promise<boolean>;
  openGridArray?: (objectIds: string[]) => void;
  openCircularArray?: (objectIds: string[]) => void;
  openDock?: (objectIds: string[]) => void;
  openNest?: (objectIds: string[]) => void;
  openCopyAlongPath?: (objectIds: string[], pathObjectId: string) => void;
  openReportBug?: () => void;
  openMaterialTest?: () => void;
  openFocusTest?: () => void;
  openIntervalTest?: () => void;
}

export interface NativeMenuItemState {
  id: AppCommandId;
  enabled?: boolean;
  checked?: boolean;
  title?: string;
  /**
   * Native accelerator update semantics:
   * omitted keeps the current/default accelerator, string sets it, null clears it.
   */
  accelerator?: string | null;
}

export interface NativeRecentFileState {
  name: string;
  path: string;
}

export interface NativeMenuStateUpdate {
  items: NativeMenuItemState[];
  recentFiles?: NativeRecentFileState[];
}

export interface AppCommandContext {
  filePath?: string;
  source?: 'native-menu' | 'shortcut' | 'menu';
}

let registeredDialogActions: AppCommandDialogActions = {};

export const BOOLEAN_ASSISTANT_OPEN_EVENT = 'beam-bench-open-boolean-assistant';

export function setAppCommandDialogActions(actions: AppCommandDialogActions): () => void {
  registeredDialogActions = actions;
  return () => {
    if (registeredDialogActions === actions) {
      registeredDialogActions = {};
    }
  };
}

const NATIVE_FOCUS_GUARDED_COMMANDS = new Set<string>([
  APP_COMMANDS.TOOLS_SELECT,
  APP_COMMANDS.TOOLS_NODE,
  APP_COMMANDS.TOOLS_LINE,
  APP_COMMANDS.TOOLS_RECTANGLE,
  APP_COMMANDS.TOOLS_ELLIPSE,
  APP_COMMANDS.TOOLS_TEXT,
  APP_COMMANDS.TOOLS_TRIANGLE,
  APP_COMMANDS.TOOLS_PENTAGON,
  APP_COMMANDS.TOOLS_POLYGON,
  APP_COMMANDS.TOOLS_OCTAGON,
  APP_COMMANDS.TOOLS_STAR,
  APP_COMMANDS.TOOLS_DUAL_STAR,
  APP_COMMANDS.TOOLS_POSITION_LASER,
  APP_COMMANDS.TOOLS_MEASURE,
  APP_COMMANDS.TOOLS_WARP_SELECTION,
  APP_COMMANDS.TOOLS_DEFORM_SELECTION,
  APP_COMMANDS.WINDOW_PREVIEW,
]);

function activeElementAcceptsTextInput(): boolean {
  if (typeof document === 'undefined') return false;
  const active = document.activeElement as HTMLElement | null;
  if (!active) return false;
  return active.tagName === 'INPUT' || active.tagName === 'TEXTAREA' || active.isContentEditable;
}

function shouldIgnoreNativeFocusGuardedCommand(commandId: string, context: AppCommandContext): boolean {
  return context.source === 'native-menu'
    && NATIVE_FOCUS_GUARDED_COMMANDS.has(commandId)
    && (activeElementAcceptsTextInput() || useUiStore.getState().textEditObjectId !== null);
}

function hasSelectedUnlockedObjects(): boolean {
  const ps = useProjectStore.getState();
  const selectedIds = ps.selectedObjectIds;
  const selected = ps.project?.objects.filter((object) => selectedIds.includes(object.id)) ?? [];
  return selected.length > 0 && selected.every((object) => !object.locked);
}

function selectedRasterImageId(): string | null {
  const ps = useProjectStore.getState();
  const selectedIds = ps.selectedObjectIds;
  const objects = ps.project?.objects ?? [];
  const selected = selectedIds
    .map((id) => objects.find((object) => object.id === id) ?? null)
    .filter((object): object is (typeof objects)[number] => object !== null);
  return selected.length === 1 && resolveEffectiveData(selected[0], objects)?.type === 'raster_image'
    ? selected[0].id
    : null;
}

function selectedDefaultName(): string | null {
  return useProjectStore.getState().project?.metadata.project_name ?? null;
}

function getSelectionBounds() {
  const ps = useProjectStore.getState();
  const project = ps.project;
  if (!project || ps.selectedObjectIds.length === 0) return null;
  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;
  for (const id of ps.selectedObjectIds) {
    const object = project.objects.find((candidate) => candidate.id === id);
    if (!object) continue;
    minX = Math.min(minX, object.bounds.min.x);
    minY = Math.min(minY, object.bounds.min.y);
    maxX = Math.max(maxX, object.bounds.max.x);
    maxY = Math.max(maxY, object.bounds.max.y);
  }
  if (!Number.isFinite(minX) || !Number.isFinite(minY) || !Number.isFinite(maxX) || !Number.isFinite(maxY)) {
    return null;
  }
  return { min: { x: minX, y: minY }, max: { x: maxX, y: maxY } };
}

function zoomToBounds(bounds: NonNullable<ReturnType<typeof getSelectionBounds>>): void {
  const size = getCanvasViewportSize();
  if (!size) return;
  const result = zoomToFitBounds(bounds, size.width, size.height);
  useUiStore.getState().zoomToFit(result.offset, result.zoom);
}

function zoomToPage(): void {
  const project = useProjectStore.getState().project;
  if (!project) return;
  zoomToBounds({
    min: { x: 0, y: 0 },
    max: {
      x: project.workspace.bed_width_mm,
      y: project.workspace.bed_height_mm,
    },
  });
}

function frameSelection(): void {
  const bounds = getSelectionBounds();
  if (bounds) {
    zoomToBounds(bounds);
  }
}

async function persistViewStyle(viewStyle: ViewStyle): Promise<void> {
  const renderOptions = renderOptionsFromViewStyle(viewStyle);
  await runCommand(() => useAppStore.getState().updateSettings({
    antialiasing: renderOptions.antialiasing,
    filled_rendering: renderOptions.filledRendering,
  }));
}

function setWindowViewStyle(commandId: AppCommandId): boolean {
  const viewStyle = WINDOW_VIEW_STYLE_BY_COMMAND[commandId];
  if (!viewStyle) return false;
  useUiStore.getState().setViewStyle(viewStyle);
  void persistViewStyle(viewStyle);
  return true;
}

function toggleWindowPanel(commandId: AppCommandId): boolean {
  const panelId = WINDOW_PANEL_BY_COMMAND[commandId];
  if (!panelId) return false;
  const ui = useUiStore.getState();
  if (panelId === 'camera') {
    ui.toggleCameraWindow();
  } else {
    ui.togglePanelVisibility(panelId);
  }
  return true;
}

function toggleWindowToolbar(commandId: AppCommandId): boolean {
  const toolbarId = WINDOW_TOOLBAR_BY_COMMAND[commandId];
  if (!toolbarId) return false;
  useUiStore.getState().toggleToolbarVisibility(toolbarId);
  return true;
}

function exportArtworkFromCurrentSelection(): Promise<string> {
  const ps = useProjectStore.getState();
  const selectedIds = [...ps.selectedObjectIds];
  return persistenceService.exportArtwork({
    selectionOnly: selectedIds.length > 0,
    selectedIds,
    defaultName: selectedDefaultName(),
  });
}

function printCurrentProject(mode: PrintMode): Promise<void> {
  if (!useProjectStore.getState().project) return Promise.resolve();
  return printService.printProject(mode);
}

async function runCommand(action: () => Promise<unknown> | unknown): Promise<void> {
  try {
    await action();
  } catch (error) {
    if (error instanceof Error && error.message === 'Export cancelled') return;
    if (String(error).toLowerCase().includes('cancelled')) return;
    useNotificationStore.getState().push(wrapBackendError(String(error)), 'error');
  }
}

async function confirmDeleteDuplicates(
  dialogs: AppCommandDialogActions,
  selectedIds: string[],
): Promise<boolean> {
  if (dialogs.confirmDeleteDuplicates) {
    return dialogs.confirmDeleteDuplicates(selectedIds);
  }

  useNotificationStore.getState().push(i18n.t('notifications.delete_duplicates_needs_dialog'), 'error');
  return false;
}

export async function executeAppCommand(
  commandId: AppCommandId | string,
  dialogs: AppCommandDialogActions = {},
  context: AppCommandContext = {},
): Promise<void> {
  const dialogActions = { ...registeredDialogActions, ...dialogs };
  const ps = useProjectStore.getState();
  const ui = useUiStore.getState();
  const preview = usePreviewStore.getState();
  const selectedIds = [...ps.selectedObjectIds];
  const hasSelection = selectedIds.length > 0;
  const unlockedSelection = hasSelectedUnlockedObjects();
  const selectedObjects = ps.project?.objects.filter((object) => selectedIds.includes(object.id)) ?? [];
  const lockedSelectedIds = selectedObjects.filter((object) => object.locked).map((object) => object.id);
  const unlockedSelectedIds = selectedObjects.filter((object) => !object.locked).map((object) => object.id);
  const canAlign = selectedIds.length >= 2 && selectedObjects.some((object) => !object.locked);
  const canDistribute = selectedIds.length >= 3 && selectedObjects.some((object) => !object.locked);
  const canMoveTogether = selectedIds.length >= 2 && selectedObjects.some((object) => !object.locked);
  const canAutoGroup = selectedIds.length >= 2 && findAutoGroupCandidates(ps.project, selectedIds).length > 0;
  const allObjects = ps.project?.objects ?? [];
  const effectiveType = (object: (typeof selectedObjects)[number]) => resolveEffectiveData(object, allObjects)?.type;
  const isVectorType = (type: string | undefined) =>
    type === 'vector_path' || type === 'shape' || type === 'polygon' || type === 'star';
  const selectedTextObject = selectedObjects.find((object) => effectiveType(object) === 'text') ?? null;
  const selectedRasterObject = selectedObjects.find((object) => effectiveType(object) === 'raster_image') ?? null;
  const selectedPathObject = selectedObjects.find((object) => isVectorType(effectiveType(object))) ?? null;
  const selectedMaskObjects = selectedObjects.filter((object) => isVectorType(effectiveType(object)));
  const deformCompatibleSelection = selectedObjects.length > 0
    && selectedObjects.every((object) => {
      const type = effectiveType(object);
      return type === 'vector_path' || type === 'shape' || type === 'text'
        || type === 'polygon' || type === 'star' || type === 'raster_image' || type === 'barcode';
    });

  if (shouldIgnoreNativeFocusGuardedCommand(commandId, context)) return;
  if (setWindowViewStyle(commandId as AppCommandId)) return;
  if (toggleWindowPanel(commandId as AppCommandId)) return;
  if (toggleWindowToolbar(commandId as AppCommandId)) return;

  // Language selection: every `language.<code>` command id maps to a
  // display_language update. Go through the store so settings (and the
  // i18next-changeLanguage effect in App.tsx) react synchronously — calling
  // appService.updateSettings directly would write to disk but leave the
  // in-memory store stale until the next fetchSettings.
  if (typeof commandId === 'string' && commandId.startsWith('language.')) {
    const code = commandId.slice('language.'.length);
    await useAppStore.getState().updateSettings({ display_language: code });
    return;
  }

  switch (commandId) {
    case APP_COMMANDS.APP_ABOUT:
    case APP_COMMANDS.HELP_ABOUT:
      dialogActions.openAbout?.();
      return;
    case APP_COMMANDS.HELP_QUICK_HELP:
      await runCommand(() => appService.openExternalUrl(QUICK_HELP_DOCS_URL));
      return;
    case APP_COMMANDS.APP_PREFERENCES:
      dialogActions.openSettings?.();
      return;
    case APP_COMMANDS.APP_QUIT:
      void appService.requestWindowClose();
      return;
    case APP_COMMANDS.FILE_NEW:
      guardUnsavedChanges(() => {
        ps.createProject('Untitled Project');
      });
      return;
    case APP_COMMANDS.FILE_NEW_WINDOW:
      await runCommand(() => appService.openNewWindow());
      return;
    case APP_COMMANDS.FILE_OPEN_RECENT:
      if (context.filePath) {
        const filePath = context.filePath;
        guardUnsavedChanges(() =>
          runCommand(async () => {
            await ps.openProjectFromPath(filePath);
            await useAppStore.getState().fetchSettings();
          }),
        );
      }
      return;
    case APP_COMMANDS.FILE_OPEN:
      guardUnsavedChanges(() =>
        runCommand(async () => {
          await ps.openProject();
          await useAppStore.getState().fetchSettings();
        }),
      );
      return;
    case APP_COMMANDS.FILE_SAVE:
      await runCommand(async () => {
        await ps.saveProject();
        await useAppStore.getState().fetchSettings();
      });
      return;
    case APP_COMMANDS.FILE_SAVE_AS:
      await runCommand(async () => {
        await ps.saveProjectAs();
        await useAppStore.getState().fetchSettings();
      });
      return;
    case APP_COMMANDS.FILE_EXPORT:
      await runCommand(() => exportArtworkFromCurrentSelection());
      return;
    case APP_COMMANDS.FILE_SAVE_MACHINE_FILES:
      if (!ps.project || preview.state !== 'current') return;
      await runCommand(() => ps.exportGcode());
      return;
    case APP_COMMANDS.LASER_MATERIAL_TEST:
      dialogActions.openMaterialTest?.();
      return;
    case APP_COMMANDS.LASER_FOCUS_TEST:
      dialogActions.openFocusTest?.();
      return;
    case APP_COMMANDS.LASER_INTERVAL_TEST:
      dialogActions.openIntervalTest?.();
      return;
    case APP_COMMANDS.FILE_PRINT_BLACK:
      await runCommand(() => printCurrentProject('black'));
      return;
    case APP_COMMANDS.FILE_PRINT_COLORS:
      await runCommand(() => printCurrentProject('color'));
      return;
    case APP_COMMANDS.FILE_IMPORT: {
      await runCommand(async () => {
        const state = useProjectStore.getState();
        if (!state.project) return;
        const layerId = state.selectedLayerId ?? state.project.layers[0]?.id ?? '';
        await state.importFiles(layerId);
      });
      return;
    }
    case APP_COMMANDS.FILE_NOTES:
      ui.toggleNotesDialog();
      return;
    case APP_COMMANDS.FILE_PREFS_IMPORT:
      dialogActions.openImportPreferences?.();
      return;
    case APP_COMMANDS.FILE_PREFS_EXPORT:
      dialogActions.openExportPreferences?.();
      return;
    case APP_COMMANDS.FILE_PREFS_OPEN_FOLDER:
      dialogActions.openPreferencesFolder?.();
      return;
    case APP_COMMANDS.FILE_PREFS_RESET_DEFAULTS:
      dialogActions.resetPreferences?.();
      return;
    case APP_COMMANDS.FILE_PREFS_EDIT_HOTKEYS:
      // Shelved for beta. Keep the command id stable while the editor is hidden.
      return;
    case APP_COMMANDS.FILE_SAVE_PROCESSED_BITMAP: {
      const objectId = selectedRasterImageId();
      if (objectId) await runCommand(() => persistenceService.saveProcessedBitmap(objectId));
      return;
    }
    case APP_COMMANDS.FILE_SAVE_BACKGROUND_CAPTURE:
      return;
    case APP_COMMANDS.EDIT_UNDO:
      useUndoStore.getState().undo();
      return;
    case APP_COMMANDS.EDIT_REDO:
      useUndoStore.getState().redo();
      return;
    case APP_COMMANDS.EDIT_SELECT_ALL:
      ps.selectAllObjects();
      return;
    case APP_COMMANDS.EDIT_INVERT_SELECTION: {
      const project = ps.project;
      if (!project) return;
      const selected = new Set(selectedIds);
      ps.selectObjects(project.objects.map((object) => object.id).filter((id) => !selected.has(id)));
      return;
    }
    case APP_COMMANDS.EDIT_CUT:
      if (unlockedSelection) await runCommand(() => clipboardCut(selectedIds));
      return;
    case APP_COMMANDS.EDIT_COPY:
      clipboardCopy(selectedIds);
      return;
    case APP_COMMANDS.EDIT_PASTE:
      await runCommand(async () => {
        if (hasClipboardData()) {
          await clipboardPaste();
          return;
        }
        await pasteClipboardArtworkFromSystem();
      });
      return;
    case APP_COMMANDS.EDIT_PASTE_IN_PLACE:
      await runCommand(() => clipboardPasteInPlace());
      return;
    case APP_COMMANDS.EDIT_DUPLICATE:
      if (unlockedSelection) await runCommand(() => clipboardDuplicate(selectedIds));
      return;
    case APP_COMMANDS.EDIT_DELETE:
      if (unlockedSelection) await runCommand(() => ps.removeObjects(selectedIds));
      return;
    case APP_COMMANDS.EDIT_SETTINGS:
      dialogActions.openSettings?.();
      return;
    case APP_COMMANDS.EDIT_CONVERT_TO_PATH:
      if (selectedIds.length === 1) await runCommand(() => ps.convertToPath(selectedIds[0]));
      return;
    case APP_COMMANDS.EDIT_CONVERT_TO_BITMAP:
      if (selectedIds.length === 1) await runCommand(() => ps.convertToBitmap(selectedIds[0], 300));
      return;
    case APP_COMMANDS.EDIT_CLOSE_PATH:
      if (selectedIds.length > 0) {
        for (const objectId of selectedIds) await runCommand(() => ps.closePath(objectId));
      }
      return;
    case APP_COMMANDS.EDIT_CLOSE_SELECTED_PATHS_WITH_TOLERANCE:
      if (selectedIds.length > 0) dialogActions.openCloseSelectedPathsWithTolerance?.(selectedIds);
      return;
    case APP_COMMANDS.EDIT_AUTO_JOIN_SELECTED_SHAPES:
      if (unlockedSelection) await runCommand(() => ps.autoJoinShapes(selectedIds, 0.05));
      return;
    case APP_COMMANDS.EDIT_CLOSE_AND_JOIN:
      if (selectedIds.length >= 2 && unlockedSelection) await runCommand(() => ps.closeAndJoin(selectedIds));
      return;
    case APP_COMMANDS.EDIT_OPTIMIZE_SELECTED_SHAPES:
      if (unlockedSelection) await runCommand(() => ps.optimizeShapes(selectedIds));
      return;
    case APP_COMMANDS.EDIT_DELETE_DUPLICATES:
      if (await confirmDeleteDuplicates(dialogActions, selectedIds)) {
        await runCommand(() => ps.deleteDuplicates(selectedIds));
      }
      return;
    case APP_COMMANDS.EDIT_SELECT_OPEN_SHAPES:
      await runCommand(() => ps.selectOpenShapes());
      return;
    case APP_COMMANDS.EDIT_SELECT_OPEN_SHAPES_SET_TO_FILL:
      await runCommand(() => ps.selectOpenShapesSetToFill());
      return;
    case APP_COMMANDS.EDIT_SELECT_ALL_SHAPES_IN_CURRENT_LAYER:
      if (ps.selectedLayerId) await runCommand(() => ps.selectAllShapesInCurrentLayer());
      return;
    case APP_COMMANDS.EDIT_SELECT_CONTAINED_SHAPES:
      await runCommand(() => ps.selectContainedShapes());
      return;
    case APP_COMMANDS.EDIT_SELECT_SHAPES_SMALLER_THAN_SELECTED:
      await runCommand(() => ps.selectShapesSmallerThanSelected());
      return;
    case APP_COMMANDS.EDIT_IMAGE_REFRESH: {
      const objectId = selectedRasterImageId();
      if (objectId) await runCommand(() => ps.refreshImage(objectId));
      return;
    }
    case APP_COMMANDS.EDIT_IMAGE_REPLACE: {
      const objectId = selectedRasterImageId();
      if (objectId) await runCommand(() => ps.replaceImage(objectId));
      return;
    }
    case APP_COMMANDS.EDIT_IMAGE_REPLACE_TO_FIT: {
      const objectId = selectedRasterImageId();
      if (objectId) await runCommand(() => ps.replaceImageToFit(objectId));
      return;
    }
    case APP_COMMANDS.ARRANGE_GROUP:
      if (selectedIds.length >= 2 && unlockedSelection) await runCommand(() => ps.groupObjects(selectedIds));
      return;
    case APP_COMMANDS.ARRANGE_UNGROUP: {
      const selected = ps.project?.objects.find((object) => object.id === selectedIds[0]) ?? null;
      if (selectedIds.length === 1 && unlockedSelection && selected?.data.type === 'group') {
        await runCommand(() => ps.ungroupObjects(selectedIds[0]));
      }
      return;
    }
    case APP_COMMANDS.ARRANGE_AUTO_GROUP:
      if (canAutoGroup) await runCommand(() => ps.autoGroupObjects(selectedIds));
      return;
    case APP_COMMANDS.TOOLS_SELECT:
      ui.setActiveTool('select');
      return;
    case APP_COMMANDS.TOOLS_NODE:
      ui.setActiveTool('node');
      return;
    case APP_COMMANDS.TOOLS_LINE:
      ui.setActiveTool('line');
      return;
    case APP_COMMANDS.TOOLS_RECTANGLE:
      ui.setLastShapeSubTool('rect');
      ui.setActiveTool('rect');
      return;
    case APP_COMMANDS.TOOLS_ELLIPSE:
      ui.setLastShapeSubTool('ellipse');
      ui.setActiveTool('ellipse');
      return;
    case APP_COMMANDS.TOOLS_TRIANGLE:
      ui.setLastShapeSubTool('triangle');
      ui.setActiveTool('polygon');
      return;
    case APP_COMMANDS.TOOLS_PENTAGON:
      ui.setLastShapeSubTool('pentagon');
      ui.setActiveTool('polygon');
      return;
    case APP_COMMANDS.TOOLS_TEXT:
      ui.setActiveTool('text');
      return;
    case APP_COMMANDS.TOOLS_POLYGON:
      ui.setLastShapeSubTool('polygon');
      ui.setActiveTool('polygon');
      return;
    case APP_COMMANDS.TOOLS_OCTAGON:
      ui.setLastShapeSubTool('octagon');
      ui.setActiveTool('polygon');
      return;
    case APP_COMMANDS.TOOLS_STAR:
      ui.setLastShapeSubTool('star');
      ui.setActiveTool('star');
      return;
    case APP_COMMANDS.TOOLS_DUAL_STAR:
      ui.setLastShapeSubTool('dual_star');
      ui.setActiveTool('star');
      return;
    case APP_COMMANDS.TOOLS_POSITION_LASER:
      ui.setActiveTool('laser_position');
      return;
    case APP_COMMANDS.TOOLS_MEASURE:
      ui.setActiveTool('measure');
      return;
    case APP_COMMANDS.TOOLS_TABS:
      ui.setActiveTool('tabs');
      return;
    case APP_COMMANDS.TOOLS_TRIM:
      ui.setActiveTool('trim');
      return;
    case APP_COMMANDS.TOOLS_BARCODE: {
      let layerId = ps.selectedLayerId ?? ps.project?.layers[0]?.id ?? null;
      if (!layerId && ps.project) {
        await runCommand(() => ps.addLayer('Line', 'line'));
        const next = useProjectStore.getState();
        layerId = next.selectedLayerId ?? next.project?.layers[0]?.id ?? null;
      }
      if (layerId) dialogActions.openBarcode?.(layerId);
      return;
    }
    case APP_COMMANDS.TOOLS_OFFSET:
      if (unlockedSelection) dialogActions.openOffset?.(selectedIds);
      return;
    case APP_COMMANDS.TOOLS_TRACE_IMAGE: {
      const objectId = selectedRasterImageId();
      if (objectId) dialogActions.openTraceImage?.(objectId);
      return;
    }
    case APP_COMMANDS.TOOLS_ADJUST_IMAGE: {
      const objectId = selectedRasterImageId();
      if (objectId) dialogActions.openAdjustImage?.(objectId);
      return;
    }
    case APP_COMMANDS.TOOLS_APPLY_PATH_TO_TEXT:
      if (selectedTextObject && selectedPathObject && selectedTextObject.id !== selectedPathObject.id) {
        await runCommand(() => ps.applyPathToText(selectedTextObject.id, selectedPathObject.id));
      }
      return;
    case APP_COMMANDS.TOOLS_APPLY_MASK_TO_IMAGE:
      if (selectedRasterObject && selectedMaskObjects.length > 0) {
        const maskIds = selectedMaskObjects
          .filter((object) => object.id !== selectedRasterObject.id && isClosedVectorCompatible(object, allObjects))
          .map((object) => object.id);
        if (maskIds.length === 0) {
          useNotificationStore.getState().push(i18n.t('notifications.image_mask_requires_closed'), 'error');
          return;
        }
        await runCommand(() => ps.assignImageMask(
          selectedRasterObject.id,
          maskIds,
          'keep_inside',
        ));
      }
      return;
    case APP_COMMANDS.TOOLS_CROP_IMAGE:
      if (selectedRasterObject && selectedPathObject && selectedRasterObject.id !== selectedPathObject.id) {
        await runCommand(() => ps.cropImage(selectedRasterObject.id, selectedPathObject.id));
      }
      return;
    case APP_COMMANDS.TOOLS_WARP_SELECTION:
      if (unlockedSelection && deformCompatibleSelection) {
        ui.setActiveTool('warp_selection');
      } else {
        useNotificationStore.getState().push(i18n.t('notifications.warp_requires_selection'), 'warning');
      }
      return;
    case APP_COMMANDS.TOOLS_DEFORM_SELECTION:
      if (unlockedSelection && deformCompatibleSelection) {
        ui.setActiveTool('deform_selection');
      } else {
        useNotificationStore.getState().push(i18n.t('notifications.deform_requires_selection'), 'warning');
      }
      return;
    case APP_COMMANDS.TOOLS_CUT_SHAPES: {
      if (selectedIds.length >= 2) {
        await runCommand(() => ps.cutShapes(selectedIds));
      }
      return;
    }
    case APP_COMMANDS.TOOLS_BOOLEAN_UNION:
      if (selectedIds.length >= 2) await runCommand(() => ps.booleanUnion(selectedIds[0], selectedIds[1]));
      return;
    case APP_COMMANDS.TOOLS_BOOLEAN_SUBTRACT:
      if (selectedIds.length >= 2) await runCommand(() => ps.booleanSubtract(selectedIds[0], selectedIds[1]));
      return;
    case APP_COMMANDS.TOOLS_BOOLEAN_INTERSECTION:
      if (selectedIds.length >= 2) await runCommand(() => ps.booleanIntersection(selectedIds[0], selectedIds[1]));
      return;
    case APP_COMMANDS.TOOLS_BOOLEAN_WELD:
      if (selectedIds.length >= 2) await runCommand(() => ps.booleanWeld(selectedIds));
      return;
    case APP_COMMANDS.TOOLS_BOOLEAN_ASSISTANT:
      if (selectedIds.length >= 2) {
        if (dialogActions.openBooleanAssistant) {
          dialogActions.openBooleanAssistant(selectedIds);
        } else {
          window.dispatchEvent(new CustomEvent<string[]>(BOOLEAN_ASSISTANT_OPEN_EVENT, { detail: selectedIds }));
        }
      }
      return;
    case APP_COMMANDS.ARRANGE_TWO_POINT_ROTATE_SCALE:
      ui.setActiveTool('two_point_rotate_scale');
      return;
    case APP_COMMANDS.ARRANGE_ALIGN_LEFT:
      if (canAlign) await runCommand(() => ps.alignObjects(selectedIds, 'left'));
      return;
    case APP_COMMANDS.ARRANGE_ALIGN_RIGHT:
      if (canAlign) await runCommand(() => ps.alignObjects(selectedIds, 'right'));
      return;
    case APP_COMMANDS.ARRANGE_ALIGN_TOP:
      if (canAlign) await runCommand(() => ps.alignObjects(selectedIds, 'top'));
      return;
    case APP_COMMANDS.ARRANGE_ALIGN_BOTTOM:
      if (canAlign) await runCommand(() => ps.alignObjects(selectedIds, 'bottom'));
      return;
    case APP_COMMANDS.ARRANGE_ALIGN_CENTERS:
      if (canAlign) await runCommand(() => ps.alignObjects(selectedIds, 'centers_xy'));
      return;
    case APP_COMMANDS.ARRANGE_ALIGN_CENTER_HORIZONTAL:
      if (canAlign) await runCommand(() => ps.alignObjects(selectedIds, 'centers_h'));
      return;
    case APP_COMMANDS.ARRANGE_ALIGN_CENTER_VERTICAL:
      if (canAlign) await runCommand(() => ps.alignObjects(selectedIds, 'centers_v'));
      return;
    case APP_COMMANDS.ARRANGE_DISTRIBUTE_HORIZONTAL:
    case APP_COMMANDS.ARRANGE_DISTRIBUTE_H_CENTERED:
      if (canDistribute) await runCommand(() => ps.distributeObjects(selectedIds, 'h_centered'));
      return;
    case APP_COMMANDS.ARRANGE_DISTRIBUTE_VERTICAL:
    case APP_COMMANDS.ARRANGE_DISTRIBUTE_V_CENTERED:
      if (canDistribute) await runCommand(() => ps.distributeObjects(selectedIds, 'v_centered'));
      return;
    case APP_COMMANDS.ARRANGE_DISTRIBUTE_H_SPACED:
      if (canDistribute) await runCommand(() => ps.distributeObjects(selectedIds, 'h_spaced'));
      return;
    case APP_COMMANDS.ARRANGE_DISTRIBUTE_V_SPACED:
      if (canDistribute) await runCommand(() => ps.distributeObjects(selectedIds, 'v_spaced'));
      return;
    case APP_COMMANDS.ARRANGE_FRONT:
      if (selectedIds.length === 1) await runCommand(() => ps.pushDrawOrder(selectedIds[0], 'front'));
      return;
    case APP_COMMANDS.ARRANGE_FORWARD:
      if (selectedIds.length === 1) await runCommand(() => ps.pushDrawOrder(selectedIds[0], 'forward'));
      return;
    case APP_COMMANDS.ARRANGE_BACKWARD:
      if (selectedIds.length === 1) await runCommand(() => ps.pushDrawOrder(selectedIds[0], 'backward'));
      return;
    case APP_COMMANDS.ARRANGE_BACK:
      if (selectedIds.length === 1) await runCommand(() => ps.pushDrawOrder(selectedIds[0], 'back'));
      return;
    case APP_COMMANDS.ARRANGE_FLIP_HORIZONTAL:
      if (unlockedSelection) await runCommand(() => ps.flipObjects(selectedIds, 'horizontal'));
      return;
    case APP_COMMANDS.ARRANGE_FLIP_VERTICAL:
      if (unlockedSelection) await runCommand(() => ps.flipObjects(selectedIds, 'vertical'));
      return;
    case APP_COMMANDS.ARRANGE_ROTATE_CW:
      if (unlockedSelection) await runCommand(() => ps.rotateObjects(selectedIds, 90));
      return;
    case APP_COMMANDS.ARRANGE_ROTATE_CCW:
      if (unlockedSelection) await runCommand(() => ps.rotateObjects(selectedIds, -90));
      return;
    case APP_COMMANDS.ARRANGE_MIRROR_ACROSS_LINE:
      if (selectedIds.length >= 2) await runCommand(() => ps.mirrorAcrossLine());
      return;
    case APP_COMMANDS.ARRANGE_GRID_ARRAY:
      if (unlockedSelection) dialogActions.openGridArray?.(selectedIds);
      return;
    case APP_COMMANDS.ARRANGE_CIRCULAR_ARRAY:
      if (unlockedSelection) dialogActions.openCircularArray?.(selectedIds);
      return;
    case APP_COMMANDS.ARRANGE_MAKE_SAME_WIDTH:
      if (selectedIds.length >= 2) await runCommand(() => ps.makeSameSize('width', false));
      return;
    case APP_COMMANDS.ARRANGE_MAKE_SAME_HEIGHT:
      if (selectedIds.length >= 2) await runCommand(() => ps.makeSameSize('height', false));
      return;
    case APP_COMMANDS.ARRANGE_MOVE_H_TOGETHER:
      if (canMoveTogether) await runCommand(() => ps.moveObjectsTogether('horizontal'));
      return;
    case APP_COMMANDS.ARRANGE_MOVE_V_TOGETHER:
      if (canMoveTogether) await runCommand(() => ps.moveObjectsTogether('vertical'));
      return;
    case APP_COMMANDS.ARRANGE_DOCK_LEFT:
      if (unlockedSelection) await runCommand(() => ps.dockObjects(selectedIds, 'left', ui.dockSettings));
      return;
    case APP_COMMANDS.ARRANGE_DOCK_RIGHT:
      if (unlockedSelection) await runCommand(() => ps.dockObjects(selectedIds, 'right', ui.dockSettings));
      return;
    case APP_COMMANDS.ARRANGE_DOCK_UP:
      if (unlockedSelection) await runCommand(() => ps.dockObjects(selectedIds, 'up', ui.dockSettings));
      return;
    case APP_COMMANDS.ARRANGE_DOCK_DOWN:
      if (unlockedSelection) await runCommand(() => ps.dockObjects(selectedIds, 'down', ui.dockSettings));
      return;
    case APP_COMMANDS.ARRANGE_DOCK:
      if (hasSelection) dialogActions.openDock?.(selectedIds);
      return;
    case APP_COMMANDS.ARRANGE_BREAK_APART:
      if (selectedIds.length === 1 && unlockedSelection) await runCommand(() => ps.breakApart(selectedIds[0]));
      return;
    case APP_COMMANDS.ARRANGE_COPY_ALONG_PATH: {
      const guide = ps.project ? pickLastSelectedVectorGuide(selectedIds, ps.project.objects) : null;
      const sourceIds = selectedIds.filter((id) => id !== guide?.id);
      if (guide && sourceIds.length > 0) dialogActions.openCopyAlongPath?.(sourceIds, guide.id);
      return;
    }
    case APP_COMMANDS.ARRANGE_RUBBER_BAND_OUTLINE:
      // Shelved for beta until this creates a tighter, geometry-aware outline.
      return;
    case APP_COMMANDS.ARRANGE_NEST_SELECTED:
      if (unlockedSelection) {
        if (dialogActions.openNest) {
          dialogActions.openNest(selectedIds);
        } else {
          await runCommand(() => nestSelected());
        }
      }
      return;
    case APP_COMMANDS.ARRANGE_MOVE_TO_LASER_POSITION:
      if (unlockedSelection) await runCommand(() => moveSelectedToLaserPosition());
      return;
    case APP_COMMANDS.ARRANGE_MOVE_TO_PAGE_CENTER:
      if (unlockedSelection) await runCommand(() => moveSelectedToPageAnchor('center'));
      return;
    case APP_COMMANDS.ARRANGE_MOVE_TO_UPPER_LEFT:
      if (unlockedSelection) await runCommand(() => moveSelectedToPageAnchor('upper_left'));
      return;
    case APP_COMMANDS.ARRANGE_MOVE_TO_UPPER_RIGHT:
      if (unlockedSelection) await runCommand(() => moveSelectedToPageAnchor('upper_right'));
      return;
    case APP_COMMANDS.ARRANGE_MOVE_TO_LOWER_LEFT:
      if (unlockedSelection) await runCommand(() => moveSelectedToPageAnchor('lower_left'));
      return;
    case APP_COMMANDS.ARRANGE_MOVE_TO_LOWER_RIGHT:
      if (unlockedSelection) await runCommand(() => moveSelectedToPageAnchor('lower_right'));
      return;
    case APP_COMMANDS.ARRANGE_MOVE_TO_LEFT:
      if (unlockedSelection) await runCommand(() => moveSelectedToPageAnchor('left'));
      return;
    case APP_COMMANDS.ARRANGE_MOVE_TO_RIGHT:
      if (unlockedSelection) await runCommand(() => moveSelectedToPageAnchor('right'));
      return;
    case APP_COMMANDS.ARRANGE_MOVE_TO_TOP:
      if (unlockedSelection) await runCommand(() => moveSelectedToPageAnchor('top'));
      return;
    case APP_COMMANDS.ARRANGE_MOVE_TO_BOTTOM:
      if (unlockedSelection) await runCommand(() => moveSelectedToPageAnchor('bottom'));
      return;
    case APP_COMMANDS.ARRANGE_MOVE_LASER_TO_SELECTION_CENTER:
      if (hasSelection) await runCommand(() => moveLaserToSelection('center'));
      return;
    case APP_COMMANDS.ARRANGE_MOVE_LASER_TO_SELECTION_UPPER_LEFT:
      if (hasSelection) await runCommand(() => moveLaserToSelection('upper_left'));
      return;
    case APP_COMMANDS.ARRANGE_MOVE_LASER_TO_SELECTION_UPPER_RIGHT:
      if (hasSelection) await runCommand(() => moveLaserToSelection('upper_right'));
      return;
    case APP_COMMANDS.ARRANGE_MOVE_LASER_TO_SELECTION_LOWER_LEFT:
      if (hasSelection) await runCommand(() => moveLaserToSelection('lower_left'));
      return;
    case APP_COMMANDS.ARRANGE_MOVE_LASER_TO_SELECTION_LOWER_RIGHT:
      if (hasSelection) await runCommand(() => moveLaserToSelection('lower_right'));
      return;
    case APP_COMMANDS.ARRANGE_MOVE_LASER_TO_SELECTION_LEFT:
      if (hasSelection) await runCommand(() => moveLaserToSelection('left'));
      return;
    case APP_COMMANDS.ARRANGE_MOVE_LASER_TO_SELECTION_RIGHT:
      if (hasSelection) await runCommand(() => moveLaserToSelection('right'));
      return;
    case APP_COMMANDS.ARRANGE_MOVE_LASER_TO_SELECTION_TOP:
      if (hasSelection) await runCommand(() => moveLaserToSelection('top'));
      return;
    case APP_COMMANDS.ARRANGE_MOVE_LASER_TO_SELECTION_BOTTOM:
      if (hasSelection) await runCommand(() => moveLaserToSelection('bottom'));
      return;
    case APP_COMMANDS.ARRANGE_JOG_LASER_LEFT:
      await runCommand(() => jogLaser('left'));
      return;
    case APP_COMMANDS.ARRANGE_JOG_LASER_RIGHT:
      await runCommand(() => jogLaser('right'));
      return;
    case APP_COMMANDS.ARRANGE_JOG_LASER_UP:
      await runCommand(() => jogLaser('up'));
      return;
    case APP_COMMANDS.ARRANGE_JOG_LASER_DOWN:
      await runCommand(() => jogLaser('down'));
      return;
    case APP_COMMANDS.ARRANGE_LOCK:
      if (unlockedSelectedIds.length > 0) await runCommand(() => ps.lockObjects(unlockedSelectedIds));
      return;
    case APP_COMMANDS.ARRANGE_UNLOCK:
      if (lockedSelectedIds.length > 0) await runCommand(() => ps.unlockObjects(lockedSelectedIds));
      return;
    case APP_COMMANDS.WINDOW_SIDE_PANELS:
      ui.toggleSidePanels();
      return;
    case APP_COMMANDS.WINDOW_PREVIEW:
      if (ps.project) preview.togglePreview();
      return;
    case APP_COMMANDS.WINDOW_REFRESH_PREVIEW:
      if (ps.project) await runCommand(() => preview.refreshPreview());
      return;
    case APP_COMMANDS.WINDOW_ZOOM_TO_PAGE:
      zoomToPage();
      return;
    case APP_COMMANDS.WINDOW_ZOOM_IN:
      if (ps.project) ui.zoomIn();
      return;
    case APP_COMMANDS.WINDOW_ZOOM_OUT:
      if (ps.project) ui.zoomOut();
      return;
    case APP_COMMANDS.WINDOW_FRAME_SELECTION:
      frameSelection();
      return;
    case APP_COMMANDS.WINDOW_TOGGLE_WIREFRAME_FILLED:
      ui.toggleFilledRendering();
      await persistViewStyle(useUiStore.getState().viewStyle);
      return;
    case APP_COMMANDS.WINDOW_RESET_LAYOUT:
      ui.resetLayout();
      return;
    case APP_COMMANDS.TOOLS_CONNECTION_DIAGNOSTICS:
      ui.showPanel('connection_diagnostics');
      return;
    case APP_COMMANDS.HELP_REPORT_BUG:
      dialogActions.openReportBug?.();
      return;
    default:
      return;
  }
}

export function getAppCommandState(): NativeMenuStateUpdate {
  const ps = useProjectStore.getState();
  const ui = useUiStore.getState();
  const preview = usePreviewStore.getState();
  const undo = useUndoStore.getState();
  const recentFiles = useAppStore.getState().settings?.recent_files ?? [];
  const customHotkeys = useAppStore.getState().settings?.custom_hotkeys ?? {};
  const projectLoaded = ps.project !== null;
  const selectedIds = ps.selectedObjectIds;
  const selectedObjects = ps.project?.objects.filter((object) => selectedIds.includes(object.id)) ?? [];
  const hasSelection = selectedIds.length > 0;
  const unlockedSelection = selectedObjects.length > 0 && selectedObjects.every((object) => !object.locked);
  const hasLockedSelection = selectedObjects.some((object) => object.locked);
  const hasUnlockedSelection = selectedObjects.some((object) => !object.locked);
  const singleSelection = selectedObjects.length === 1 ? selectedObjects[0] : null;
  const allObjects = ps.project?.objects ?? [];
  const rasterSelected = singleSelection !== null
    && resolveEffectiveData(singleSelection, allObjects)?.type === 'raster_image';
  const refreshableRasterSelected = imageObjectHasSourcePath(singleSelection, ps.project?.assets ?? []);
  const vectorSelection = selectedObjects.length > 0
    && selectedObjects.every((object) => {
      const type = resolveEffectiveData(object, allObjects)?.type;
      return type === 'vector_path' || type === 'shape' || type === 'text' || type === 'polygon' || type === 'star';
    });
  const effectiveType = (object: (typeof selectedObjects)[number]) => resolveEffectiveData(object, allObjects)?.type;
  const isVectorType = (type: string | undefined) =>
    type === 'vector_path' || type === 'shape' || type === 'polygon' || type === 'star';
  const selectedTextObject = selectedObjects.find((object) => effectiveType(object) === 'text') ?? null;
  const selectedRasterObject = selectedObjects.find((object) => effectiveType(object) === 'raster_image') ?? null;
  const selectedPathObject = selectedObjects.find((object) => isVectorType(effectiveType(object))) ?? null;
  const selectedMaskObjects = selectedObjects.filter((object) => isVectorType(effectiveType(object)));
  const singleVectorSelection = singleSelection !== null && vectorSelection;
  const singleClosedVectorCompatibleSelection =
    singleSelection !== null && isClosedVectorCompatible(singleSelection, allObjects);
  const canBoolean = selectedObjects.length === 2 && unlockedSelection;
  const canWeld = selectedObjects.length >= 2 && unlockedSelection;
  const canApplyPathToText = selectedObjects.length === 2
    && Boolean(selectedTextObject && selectedPathObject && selectedTextObject.id !== selectedPathObject.id);
  const canApplyMaskToImage = selectedRasterObject !== null
    && selectedMaskObjects.some((object) =>
      object.id !== selectedRasterObject?.id && isClosedVectorCompatible(object, allObjects)
    );
  const canCropImage = selectedObjects.length === 2
    && Boolean(selectedRasterObject && selectedPathObject && selectedRasterObject.id !== selectedPathObject.id);
  const deformCompatibleSelection = selectedObjects.length > 0
    && selectedObjects.every((object) => {
      const type = effectiveType(object);
      return type === 'vector_path' || type === 'shape' || type === 'text'
        || type === 'polygon' || type === 'star' || type === 'raster_image' || type === 'barcode';
    });
  const canAlign = selectedIds.length >= 2 && selectedObjects.some((object) => !object.locked);
  const canDistribute = selectedIds.length >= 3 && selectedObjects.some((object) => !object.locked);
  const canMoveTogether = selectedIds.length >= 2 && selectedObjects.some((object) => !object.locked);
  const canAutoGroup = selectedIds.length >= 2 && findAutoGroupCandidates(ps.project, selectedIds).length > 0;
  const accel = (id: AppCommandId) => nativeAcceleratorForCommand(id, customHotkeys);
  const viewStyleItems = WINDOW_VIEW_STYLE_ITEMS.map((item) => ({
    id: item.commandId,
    enabled: true,
    checked: ui.viewStyle === item.viewStyle,
  }));
  const panelItems = WINDOW_PANEL_MENU_ITEMS.map((item) => ({
    id: item.commandId,
    enabled: true,
    checked: !ui.panelLayout.hiddenPanelIds.includes(item.panelId),
  }));
  const toolbarItems = WINDOW_TOOLBAR_MENU_ITEMS.map((item) => ({
    id: item.commandId,
    enabled: true,
    checked: ui.panelLayout.toolbarVisibility[item.toolbarId],
  }));
  const toolsSelectAccelerator = accel(APP_COMMANDS.TOOLS_SELECT);
  const displayLanguage = useAppStore.getState().settings?.display_language ?? DEFAULT_LOCALE;
  const languageItems = SUPPORTED_LOCALES.map((code) => ({
    id: `language.${code}` as AppCommandId,
    enabled: true,
    checked: code === displayLanguage,
  }));

  return {
    recentFiles: recentFiles.map((file) => ({ name: file.name, path: file.path })),
    items: [
      ...nativeAcceleratorUpdates(customHotkeys),
      { id: APP_COMMANDS.FILE_PREFS_IMPORT, enabled: true },
      { id: APP_COMMANDS.FILE_PREFS_EXPORT, enabled: true },
      { id: APP_COMMANDS.FILE_PREFS_OPEN_FOLDER, enabled: true },
      { id: APP_COMMANDS.FILE_PREFS_RESET_DEFAULTS, enabled: true },
      { id: APP_COMMANDS.FILE_NEW_WINDOW, enabled: true },
      { id: APP_COMMANDS.FILE_SAVE, enabled: projectLoaded, accelerator: accel(APP_COMMANDS.FILE_SAVE) },
      { id: APP_COMMANDS.FILE_SAVE_AS, enabled: projectLoaded, accelerator: accel(APP_COMMANDS.FILE_SAVE_AS) },
      {
        id: APP_COMMANDS.FILE_EXPORT,
        enabled: projectLoaded,
        title: i18n.t(hasSelection ? 'menus.file.export_selection' : 'menus.file.export'),
        accelerator: projectLoaded && !ui.textEditObjectId
          ? nativeAcceleratorForCommand(APP_COMMANDS.FILE_EXPORT, customHotkeys)
          : null,
      },
      {
        id: APP_COMMANDS.FILE_SAVE_MACHINE_FILES,
        enabled: projectLoaded && preview.state === 'current',
        accelerator: accel(APP_COMMANDS.FILE_SAVE_MACHINE_FILES),
      },
      { id: APP_COMMANDS.FILE_PRINT_BLACK, enabled: projectLoaded, accelerator: accel(APP_COMMANDS.FILE_PRINT_BLACK) },
      { id: APP_COMMANDS.FILE_PRINT_COLORS, enabled: projectLoaded, accelerator: accel(APP_COMMANDS.FILE_PRINT_COLORS) },
      { id: APP_COMMANDS.FILE_SAVE_PROCESSED_BITMAP, enabled: rasterSelected },
      { id: APP_COMMANDS.FILE_IMPORT, enabled: projectLoaded, accelerator: accel(APP_COMMANDS.FILE_IMPORT) },
      { id: APP_COMMANDS.FILE_NOTES, enabled: projectLoaded, title: i18n.t(ui.showNotesDialog ? 'menus.file.hide_notes' : 'menus.file.show_notes') },
      { id: APP_COMMANDS.EDIT_UNDO, enabled: undo.canUndo, accelerator: accel(APP_COMMANDS.EDIT_UNDO) },
      { id: APP_COMMANDS.EDIT_REDO, enabled: undo.canRedo, accelerator: accel(APP_COMMANDS.EDIT_REDO) },
      { id: APP_COMMANDS.EDIT_SELECT_ALL, enabled: projectLoaded, accelerator: accel(APP_COMMANDS.EDIT_SELECT_ALL) },
      { id: APP_COMMANDS.EDIT_INVERT_SELECTION, enabled: projectLoaded, accelerator: accel(APP_COMMANDS.EDIT_INVERT_SELECTION) },
      { id: APP_COMMANDS.EDIT_CUT, enabled: unlockedSelection, accelerator: accel(APP_COMMANDS.EDIT_CUT) },
      { id: APP_COMMANDS.EDIT_COPY, enabled: hasSelection, accelerator: accel(APP_COMMANDS.EDIT_COPY) },
      // Paste must stay enabled whenever a project is open: the system
      // clipboard may hold an image/SVG the app can't detect synchronously,
      // and on macOS a disabled native menu item swallows Cmd+V entirely.
      // The handler falls through to the system clipboard when the in-app
      // clipboard is empty. Paste In Place is in-app-clipboard only.
      { id: APP_COMMANDS.EDIT_PASTE, enabled: projectLoaded, accelerator: accel(APP_COMMANDS.EDIT_PASTE) },
      { id: APP_COMMANDS.EDIT_PASTE_IN_PLACE, enabled: projectLoaded && ui.hasClipboard, accelerator: accel(APP_COMMANDS.EDIT_PASTE_IN_PLACE) },
      { id: APP_COMMANDS.EDIT_DUPLICATE, enabled: unlockedSelection, accelerator: accel(APP_COMMANDS.EDIT_DUPLICATE) },
      { id: APP_COMMANDS.EDIT_DELETE, enabled: unlockedSelection, accelerator: accel(APP_COMMANDS.EDIT_DELETE) },
      { id: APP_COMMANDS.EDIT_SETTINGS, enabled: true },
      { id: APP_COMMANDS.EDIT_CONVERT_TO_PATH, enabled: singleVectorSelection, accelerator: accel(APP_COMMANDS.EDIT_CONVERT_TO_PATH) },
      { id: APP_COMMANDS.EDIT_CONVERT_TO_BITMAP, enabled: singleSelection !== null, accelerator: accel(APP_COMMANDS.EDIT_CONVERT_TO_BITMAP) },
      { id: APP_COMMANDS.EDIT_CLOSE_PATH, enabled: vectorSelection, accelerator: accel(APP_COMMANDS.EDIT_CLOSE_PATH) },
      { id: APP_COMMANDS.EDIT_CLOSE_SELECTED_PATHS_WITH_TOLERANCE, enabled: vectorSelection },
      { id: APP_COMMANDS.EDIT_AUTO_JOIN_SELECTED_SHAPES, enabled: vectorSelection, accelerator: accel(APP_COMMANDS.EDIT_AUTO_JOIN_SELECTED_SHAPES) },
      { id: APP_COMMANDS.EDIT_CLOSE_AND_JOIN, enabled: selectedObjects.length >= 2 && vectorSelection },
      { id: APP_COMMANDS.EDIT_OPTIMIZE_SELECTED_SHAPES, enabled: vectorSelection, accelerator: accel(APP_COMMANDS.EDIT_OPTIMIZE_SELECTED_SHAPES) },
      { id: APP_COMMANDS.EDIT_DELETE_DUPLICATES, enabled: projectLoaded, accelerator: accel(APP_COMMANDS.EDIT_DELETE_DUPLICATES) },
      { id: APP_COMMANDS.EDIT_SELECT_OPEN_SHAPES, enabled: projectLoaded },
      { id: APP_COMMANDS.EDIT_SELECT_OPEN_SHAPES_SET_TO_FILL, enabled: projectLoaded },
      { id: APP_COMMANDS.EDIT_SELECT_ALL_SHAPES_IN_CURRENT_LAYER, enabled: projectLoaded && ps.selectedLayerId !== null },
      { id: APP_COMMANDS.EDIT_SELECT_CONTAINED_SHAPES, enabled: singleClosedVectorCompatibleSelection },
      { id: APP_COMMANDS.EDIT_SELECT_SHAPES_SMALLER_THAN_SELECTED, enabled: hasSelection },
      { id: APP_COMMANDS.EDIT_IMAGE_REFRESH, enabled: refreshableRasterSelected },
      { id: APP_COMMANDS.EDIT_IMAGE_REPLACE, enabled: rasterSelected },
      { id: APP_COMMANDS.EDIT_IMAGE_REPLACE_TO_FIT, enabled: rasterSelected },
      { id: APP_COMMANDS.ARRANGE_GROUP, enabled: selectedObjects.length >= 2 && unlockedSelection, accelerator: accel(APP_COMMANDS.ARRANGE_GROUP) },
      {
        id: APP_COMMANDS.ARRANGE_UNGROUP,
        enabled: singleSelection?.data.type === 'group' && unlockedSelection,
        accelerator: accel(APP_COMMANDS.ARRANGE_UNGROUP),
      },
      { id: APP_COMMANDS.ARRANGE_AUTO_GROUP, enabled: canAutoGroup },
      {
        id: APP_COMMANDS.TOOLS_SELECT,
        enabled: projectLoaded,
        accelerator: ui.activeTool === 'node' && toolsSelectAccelerator === 'Esc'
          ? null
          : toolsSelectAccelerator,
      },
      { id: APP_COMMANDS.TOOLS_LINE, enabled: projectLoaded, accelerator: accel(APP_COMMANDS.TOOLS_LINE) },
      { id: APP_COMMANDS.TOOLS_RECTANGLE, enabled: projectLoaded, accelerator: accel(APP_COMMANDS.TOOLS_RECTANGLE) },
      { id: APP_COMMANDS.TOOLS_ELLIPSE, enabled: projectLoaded, accelerator: accel(APP_COMMANDS.TOOLS_ELLIPSE) },
      { id: APP_COMMANDS.TOOLS_TRIANGLE, enabled: projectLoaded },
      { id: APP_COMMANDS.TOOLS_PENTAGON, enabled: projectLoaded },
      { id: APP_COMMANDS.TOOLS_POLYGON, enabled: projectLoaded },
      { id: APP_COMMANDS.TOOLS_OCTAGON, enabled: projectLoaded },
      { id: APP_COMMANDS.TOOLS_STAR, enabled: projectLoaded },
      { id: APP_COMMANDS.TOOLS_DUAL_STAR, enabled: projectLoaded },
      { id: APP_COMMANDS.TOOLS_NODE, enabled: projectLoaded, accelerator: accel(APP_COMMANDS.TOOLS_NODE) },
      { id: APP_COMMANDS.TOOLS_TRIM, enabled: projectLoaded, accelerator: accel(APP_COMMANDS.TOOLS_TRIM) },
      { id: APP_COMMANDS.TOOLS_TABS, enabled: projectLoaded, accelerator: accel(APP_COMMANDS.TOOLS_TABS) },
      { id: APP_COMMANDS.TOOLS_TEXT, enabled: projectLoaded, accelerator: accel(APP_COMMANDS.TOOLS_TEXT) },
      { id: APP_COMMANDS.TOOLS_POSITION_LASER, enabled: projectLoaded, accelerator: accel(APP_COMMANDS.TOOLS_POSITION_LASER) },
      { id: APP_COMMANDS.TOOLS_MEASURE, enabled: projectLoaded, accelerator: accel(APP_COMMANDS.TOOLS_MEASURE) },
      { id: APP_COMMANDS.TOOLS_BARCODE, enabled: projectLoaded },
      { id: APP_COMMANDS.TOOLS_OFFSET, enabled: unlockedSelection, accelerator: accel(APP_COMMANDS.TOOLS_OFFSET) },
      { id: APP_COMMANDS.TOOLS_BOOLEAN_WELD, enabled: canWeld, accelerator: accel(APP_COMMANDS.TOOLS_BOOLEAN_WELD) },
      { id: APP_COMMANDS.TOOLS_BOOLEAN_UNION, enabled: canBoolean, accelerator: accel(APP_COMMANDS.TOOLS_BOOLEAN_UNION) },
      { id: APP_COMMANDS.TOOLS_BOOLEAN_SUBTRACT, enabled: canBoolean, accelerator: accel(APP_COMMANDS.TOOLS_BOOLEAN_SUBTRACT) },
      { id: APP_COMMANDS.TOOLS_BOOLEAN_INTERSECTION, enabled: canBoolean, accelerator: accel(APP_COMMANDS.TOOLS_BOOLEAN_INTERSECTION) },
      { id: APP_COMMANDS.TOOLS_BOOLEAN_ASSISTANT, enabled: canBoolean, accelerator: accel(APP_COMMANDS.TOOLS_BOOLEAN_ASSISTANT) },
      { id: APP_COMMANDS.TOOLS_CUT_SHAPES, enabled: selectedObjects.length >= 2 && unlockedSelection, accelerator: accel(APP_COMMANDS.TOOLS_CUT_SHAPES) },
      { id: APP_COMMANDS.TOOLS_ADJUST_IMAGE, enabled: rasterSelected, accelerator: accel(APP_COMMANDS.TOOLS_ADJUST_IMAGE) },
      { id: APP_COMMANDS.TOOLS_TRACE_IMAGE, enabled: rasterSelected, accelerator: accel(APP_COMMANDS.TOOLS_TRACE_IMAGE) },
      { id: APP_COMMANDS.TOOLS_APPLY_PATH_TO_TEXT, enabled: canApplyPathToText },
      { id: APP_COMMANDS.TOOLS_APPLY_MASK_TO_IMAGE, enabled: canApplyMaskToImage },
      { id: APP_COMMANDS.TOOLS_CROP_IMAGE, enabled: canCropImage },
      { id: APP_COMMANDS.TOOLS_WARP_SELECTION, enabled: deformCompatibleSelection && unlockedSelection },
      { id: APP_COMMANDS.TOOLS_DEFORM_SELECTION, enabled: deformCompatibleSelection && unlockedSelection },
      { id: APP_COMMANDS.ARRANGE_ALIGN_CENTERS, enabled: canAlign },
      { id: APP_COMMANDS.ARRANGE_ALIGN_LEFT, enabled: canAlign },
      { id: APP_COMMANDS.ARRANGE_ALIGN_RIGHT, enabled: canAlign },
      { id: APP_COMMANDS.ARRANGE_ALIGN_TOP, enabled: canAlign },
      { id: APP_COMMANDS.ARRANGE_ALIGN_BOTTOM, enabled: canAlign },
      { id: APP_COMMANDS.ARRANGE_ALIGN_CENTER_VERTICAL, enabled: canAlign, accelerator: accel(APP_COMMANDS.ARRANGE_ALIGN_CENTER_VERTICAL) },
      { id: APP_COMMANDS.ARRANGE_ALIGN_CENTER_HORIZONTAL, enabled: canAlign, accelerator: accel(APP_COMMANDS.ARRANGE_ALIGN_CENTER_HORIZONTAL) },
      { id: APP_COMMANDS.ARRANGE_DISTRIBUTE_V_SPACED, enabled: canDistribute },
      { id: APP_COMMANDS.ARRANGE_DISTRIBUTE_V_CENTERED, enabled: canDistribute },
      { id: APP_COMMANDS.ARRANGE_DISTRIBUTE_H_SPACED, enabled: canDistribute },
      { id: APP_COMMANDS.ARRANGE_DISTRIBUTE_H_CENTERED, enabled: canDistribute },
      { id: APP_COMMANDS.ARRANGE_FRONT, enabled: singleSelection !== null, accelerator: accel(APP_COMMANDS.ARRANGE_FRONT) },
      { id: APP_COMMANDS.ARRANGE_FORWARD, enabled: singleSelection !== null, accelerator: accel(APP_COMMANDS.ARRANGE_FORWARD) },
      { id: APP_COMMANDS.ARRANGE_BACKWARD, enabled: singleSelection !== null, accelerator: accel(APP_COMMANDS.ARRANGE_BACKWARD) },
      { id: APP_COMMANDS.ARRANGE_BACK, enabled: singleSelection !== null, accelerator: accel(APP_COMMANDS.ARRANGE_BACK) },
      { id: APP_COMMANDS.ARRANGE_FLIP_HORIZONTAL, enabled: unlockedSelection, accelerator: accel(APP_COMMANDS.ARRANGE_FLIP_HORIZONTAL) },
      { id: APP_COMMANDS.ARRANGE_FLIP_VERTICAL, enabled: unlockedSelection, accelerator: accel(APP_COMMANDS.ARRANGE_FLIP_VERTICAL) },
      { id: APP_COMMANDS.ARRANGE_MIRROR_ACROSS_LINE, enabled: selectedObjects.length >= 2 && unlockedSelection },
      { id: APP_COMMANDS.ARRANGE_ROTATE_CW, enabled: unlockedSelection, accelerator: accel(APP_COMMANDS.ARRANGE_ROTATE_CW) },
      { id: APP_COMMANDS.ARRANGE_ROTATE_CCW, enabled: unlockedSelection, accelerator: accel(APP_COMMANDS.ARRANGE_ROTATE_CCW) },
      { id: APP_COMMANDS.ARRANGE_TWO_POINT_ROTATE_SCALE, enabled: unlockedSelection, accelerator: accel(APP_COMMANDS.ARRANGE_TWO_POINT_ROTATE_SCALE) },
      { id: APP_COMMANDS.ARRANGE_NEST_SELECTED, enabled: unlockedSelection },
      { id: APP_COMMANDS.ARRANGE_GRID_ARRAY, enabled: unlockedSelection },
      { id: APP_COMMANDS.ARRANGE_CIRCULAR_ARRAY, enabled: unlockedSelection },
      { id: APP_COMMANDS.ARRANGE_MOVE_H_TOGETHER, enabled: canMoveTogether, accelerator: accel(APP_COMMANDS.ARRANGE_MOVE_H_TOGETHER) },
      { id: APP_COMMANDS.ARRANGE_MOVE_V_TOGETHER, enabled: canMoveTogether, accelerator: accel(APP_COMMANDS.ARRANGE_MOVE_V_TOGETHER) },
      { id: APP_COMMANDS.ARRANGE_DOCK_LEFT, enabled: unlockedSelection },
      { id: APP_COMMANDS.ARRANGE_DOCK_RIGHT, enabled: unlockedSelection },
      { id: APP_COMMANDS.ARRANGE_DOCK_UP, enabled: unlockedSelection },
      { id: APP_COMMANDS.ARRANGE_DOCK_DOWN, enabled: unlockedSelection },
      { id: APP_COMMANDS.ARRANGE_MOVE_TO_LASER_POSITION, enabled: unlockedSelection },
      { id: APP_COMMANDS.ARRANGE_MOVE_TO_PAGE_CENTER, enabled: unlockedSelection, accelerator: accel(APP_COMMANDS.ARRANGE_MOVE_TO_PAGE_CENTER) },
      { id: APP_COMMANDS.ARRANGE_MOVE_TO_UPPER_LEFT, enabled: unlockedSelection },
      { id: APP_COMMANDS.ARRANGE_MOVE_TO_UPPER_RIGHT, enabled: unlockedSelection },
      { id: APP_COMMANDS.ARRANGE_MOVE_TO_LOWER_LEFT, enabled: unlockedSelection },
      { id: APP_COMMANDS.ARRANGE_MOVE_TO_LOWER_RIGHT, enabled: unlockedSelection },
      { id: APP_COMMANDS.ARRANGE_MOVE_TO_LEFT, enabled: unlockedSelection },
      { id: APP_COMMANDS.ARRANGE_MOVE_TO_RIGHT, enabled: unlockedSelection },
      { id: APP_COMMANDS.ARRANGE_MOVE_TO_TOP, enabled: unlockedSelection },
      { id: APP_COMMANDS.ARRANGE_MOVE_TO_BOTTOM, enabled: unlockedSelection },
      { id: APP_COMMANDS.ARRANGE_MOVE_LASER_TO_SELECTION_CENTER, enabled: hasSelection },
      { id: APP_COMMANDS.ARRANGE_MOVE_LASER_TO_SELECTION_UPPER_LEFT, enabled: hasSelection },
      { id: APP_COMMANDS.ARRANGE_MOVE_LASER_TO_SELECTION_UPPER_RIGHT, enabled: hasSelection },
      { id: APP_COMMANDS.ARRANGE_MOVE_LASER_TO_SELECTION_LOWER_LEFT, enabled: hasSelection },
      { id: APP_COMMANDS.ARRANGE_MOVE_LASER_TO_SELECTION_LOWER_RIGHT, enabled: hasSelection },
      { id: APP_COMMANDS.ARRANGE_MOVE_LASER_TO_SELECTION_LEFT, enabled: hasSelection },
      { id: APP_COMMANDS.ARRANGE_MOVE_LASER_TO_SELECTION_RIGHT, enabled: hasSelection },
      { id: APP_COMMANDS.ARRANGE_MOVE_LASER_TO_SELECTION_TOP, enabled: hasSelection },
      { id: APP_COMMANDS.ARRANGE_MOVE_LASER_TO_SELECTION_BOTTOM, enabled: hasSelection },
      { id: APP_COMMANDS.ARRANGE_JOG_LASER_LEFT, enabled: useMachineStore.getState().machineStatus?.run_state === 'idle', accelerator: accel(APP_COMMANDS.ARRANGE_JOG_LASER_LEFT) },
      { id: APP_COMMANDS.ARRANGE_JOG_LASER_RIGHT, enabled: useMachineStore.getState().machineStatus?.run_state === 'idle', accelerator: accel(APP_COMMANDS.ARRANGE_JOG_LASER_RIGHT) },
      { id: APP_COMMANDS.ARRANGE_JOG_LASER_UP, enabled: useMachineStore.getState().machineStatus?.run_state === 'idle', accelerator: accel(APP_COMMANDS.ARRANGE_JOG_LASER_UP) },
      { id: APP_COMMANDS.ARRANGE_JOG_LASER_DOWN, enabled: useMachineStore.getState().machineStatus?.run_state === 'idle', accelerator: accel(APP_COMMANDS.ARRANGE_JOG_LASER_DOWN) },
      { id: APP_COMMANDS.ARRANGE_BREAK_APART, enabled: singleVectorSelection, accelerator: accel(APP_COMMANDS.ARRANGE_BREAK_APART) },
      { id: APP_COMMANDS.ARRANGE_COPY_ALONG_PATH, enabled: selectedObjects.length >= 2 && vectorSelection },
      { id: APP_COMMANDS.ARRANGE_LOCK, enabled: hasUnlockedSelection },
      { id: APP_COMMANDS.ARRANGE_UNLOCK, enabled: hasLockedSelection },
      { id: APP_COMMANDS.WINDOW_SIDE_PANELS, enabled: true, checked: ui.sidePanelsVisible, accelerator: accel(APP_COMMANDS.WINDOW_SIDE_PANELS) },
      { id: APP_COMMANDS.WINDOW_PREVIEW, enabled: projectLoaded, checked: preview.previewWindowOpen, accelerator: accel(APP_COMMANDS.WINDOW_PREVIEW) },
      {
        id: APP_COMMANDS.WINDOW_REFRESH_PREVIEW,
        enabled: projectLoaded && (preview.previewWindowOpen || preview.state !== 'idle'),
        accelerator: accel(APP_COMMANDS.WINDOW_REFRESH_PREVIEW),
      },
      { id: APP_COMMANDS.WINDOW_ZOOM_TO_PAGE, enabled: projectLoaded, accelerator: accel(APP_COMMANDS.WINDOW_ZOOM_TO_PAGE) },
      { id: APP_COMMANDS.WINDOW_ZOOM_IN, enabled: projectLoaded, accelerator: accel(APP_COMMANDS.WINDOW_ZOOM_IN) },
      { id: APP_COMMANDS.WINDOW_ZOOM_OUT, enabled: projectLoaded, accelerator: accel(APP_COMMANDS.WINDOW_ZOOM_OUT) },
      { id: APP_COMMANDS.WINDOW_FRAME_SELECTION, enabled: hasSelection, accelerator: accel(APP_COMMANDS.WINDOW_FRAME_SELECTION) },
      ...viewStyleItems,
      {
        id: APP_COMMANDS.WINDOW_TOGGLE_WIREFRAME_FILLED,
        enabled: true,
        accelerator: accel(APP_COMMANDS.WINDOW_TOGGLE_WIREFRAME_FILLED),
      },
      ...panelItems,
      ...toolbarItems,
      ...languageItems,
    ],
  };
}

// macOS-only duplicate-dispatch guard. This mirrors native accelerators that can
// become effective; isNativeMenuOwnedShortcut still returns false off macOS.
const NATIVE_MENU_OWNED_SHORTCUTS = new Set([
  'meta+comma',
  'meta+n',
  'meta+o',
  'meta+s',
  'meta+shift+s',
  'meta+i',
  'meta+alt+n',
  'meta+q',
  'meta+p',
  'meta+shift+p',
  'meta+z',
  'meta+shift+z',
  'meta+a',
  'meta+shift+i',
  'meta+shift+c',
  'meta+x',
  'meta+c',
  'meta+v',
  'alt+v',
  'meta+d',
  'delete',
  'backspace',
  'meta+g',
  'meta+u',
  'alt+j',
  'alt+shift+o',
  'alt+d',
  'escape',
  'meta+l',
  'meta+r',
  'meta+e',
  'meta+`',
  'meta+k',
  'meta+tab',
  'meta+t',
  'meta+shift+l',
  'meta+m',
  'alt+o',
  'meta+w',
  'meta+b',
  'alt+shift+c',
  'alt+t',
  'alt+i',
  'alt+b',
  'alt+left',
  'alt+right',
  'alt+up',
  'alt+down',
  'alt+pageup',
  'alt+pagedown',
  'alt+shift+h',
  'alt+shift+v',
  'meta+2',
  'alt+plus',
  'alt+-',
  'alt+*',
  'meta+shift+b',
  'pageup',
  'pagedown',
  'meta+pageup',
  'meta+pagedown',
  'meta+shift+h',
  'meta+shift+v',
  'meta+shift+m',
  'meta+alt+[',
  'meta+alt+]',
  'meta+shift+[',
  'meta+shift+]',
  'period',
  'comma',
  'meta+0',
  'meta+=',
  'meta+-',
  'meta+shift+a',
  'alt+p',
  'alt+shift+w',
  'p',
  'f12',
  'f1',
  'alt+x',
  'alt+shift+l',
]);

export function nativeMenuShortcutKey(event: Pick<KeyboardEvent, 'key' | 'metaKey' | 'altKey' | 'shiftKey'>): string {
  const parts: string[] = [];
  if (event.metaKey) parts.push('meta');
  if (event.altKey) parts.push('alt');
  if (event.shiftKey) parts.push('shift');
  const key = event.key.toLowerCase();
  parts.push(key === '.' ? 'period' : key === ',' ? 'comma' : key.startsWith('arrow') ? key.slice(5) : key);
  return parts.join('+');
}

export function isNativeMenuOwnedShortcut(event: KeyboardEvent, nativeMenuActive = isNativeMenuActive()): boolean {
  if (!nativeMenuActive) return false;
  if (activeElementAcceptsTextInput() || useUiStore.getState().textEditObjectId !== null) return false;
  if (NATIVE_MENU_OWNED_SHORTCUTS.has(nativeMenuShortcutKey(event))) return true;
  const customHotkeys = useAppStore.getState().settings?.custom_hotkeys ?? {};
  return findCommandForKeyboardEvent(event, customHotkeys) !== null;
}

export function setToolFromCommand(tool: ToolType): void {
  useUiStore.getState().setActiveTool(tool);
}

export function resetAppCommandTestState(): void {
  clearClipboard();
}
